<!--
  Pasted text or a file → cards (v1.3, "Add cards" › Paste / From a file). Three steps on one
  page: 1 the source (a text box, or a file dropped / picked), 2 the columns mapped to the
  chosen note type's fields with the first cards as they will look and the duplicate rule in
  plain words, 3 the import with its progress and the shared result ("12 cards added · Undo ·
  Open deck · Study now"). Parsing, mapping and the import itself are Rust's
  (`io.import.preview` / `io.import.start`, one undo step); this screen pre-fills the mapping
  from the header or the last one used with the note type.
-->
<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import { api, type ApiError } from '$lib/api';
  import { isTauri } from '$lib/api/transport';
  import type {
    ColumnTarget,
    DuplicatePolicy,
    EffectiveAttributeDto,
    ImportFormat,
    ImportIssue,
    ImportPreview,
    ImportReport,
    ImportSource,
    JobDto,
    NoteTypeDto,
    RenderedSides,
    TextOptionsDto,
  } from '$lib/api/types';
  import { formatCount, formatMegabytes } from '$lib/format';
  import { t, td } from '$lib/i18n/index.svelte';
  import { Button, Checkbox, CircleAlert, FolderOpen, Icon, Info, Select, Skeleton, Textarea, Upload } from '$lib/ui';
  import CardView from '../cards/CardView.svelte';
  import { moduleKnownOn, trackModules } from '../cards/modules.svelte';
  import AddedSummary from '../notes/AddedSummary.svelte';
  import { blankRow, type Row } from '../notes/create';
  import { fieldRole, fieldWithRole, fillOrder } from '../notes/roles';
  import type { ScreenCtx } from '../types';
  import { pickFile } from './files';
  import { followJob, resultOf } from './job';
  import JobProgress from './JobProgress.svelte';
  import { decodeColumn, encodeColumn, loadMapping, prefillMapping, resolveColumn, saveMapping, type MappingOrigin } from './mapping';
  import Separators from './Separators.svelte';

  /** Where the cards go, resolved by the page's shared destination picker. */
  export interface Destination {
    /** A deck is chosen or can be made (new deck with a title). */
    ready: boolean;
    /** Existing deck for the preview's classifications (`null`: new deck / none yet). */
    deckId: string | null;
    /** Parent of a package's decks (`null` = top level). */
    packageParent: string | null;
    /** The deck the rows go into, created now if it is new (`null`: could not). */
    ensure: () => Promise<{ id: string; created: boolean } | null>;
  }

  interface Props {
    ctx: ScreenCtx;
    kind: 'paste' | 'file';
    nt: NoteTypeDto | null;
    notetypes: NoteTypeDto[];
    onchoosetype: (id: string) => void;
    destination: Destination;
    /** Paste only: put the rows into the grid instead (edit before saving). */
    onrows?: (rows: Row[]) => void;
    /** A finished import (the page refreshes what it shows). */
    ondone?: () => void;
  }
  let { ctx, kind, nt, notetypes, onchoosetype, destination, onrows, ondone }: Props = $props();

  const EXTENSIONS = ['csv', 'tsv', 'txt', 'json', 'apkg', 'colpkg', 'fcpack'];
  const BINARY = ['apkg', 'colpkg', 'fcpack'];
  const FORMATS: ImportFormat[] = ['text', 'quizlet', 'json', 'anki', 'fcpack'];
  const POLICIES: DuplicatePolicy[] = ['skip', 'update', 'add'];
  /** Rows the editor grid takes at most ("Edit in the grid"). */
  const GRID_MAX = 200;

  let text = $state('');
  /** The chosen file: a path (app) or its text (browser / a dropped text file). */
  let file = $state<{ name: string; path: string | null; text: string | null; size: number | null } | null>(null);
  let format = $state<ImportFormat | 'auto'>('auto');
  let options = $state<TextOptionsDto | null>(null);
  let preview = $state<ImportPreview | null>(null);
  let loading = $state(false);
  let columns = $state<ColumnTarget[]>([]);
  let header = $state(false);
  let origin = $state<MappingOrigin>('order');
  let duplicates = $state<DuplicatePolicy>('skip');
  let includeHistory = $state(true);
  let createMissing = $state(false);
  let advanced = $state(false);
  let attributes = $state<EffectiveAttributeDto[]>([]);
  let job = $state<JobDto | null>(null);
  let result = $state<{ report: ImportReport; deckId: string | null; steps: number } | null>(null);
  let error = $state<string | null>(null);
  let cards = $state<(RenderedSides | null)[]>([]);
  let dragging = $state(false);
  let fileInput = $state<HTMLInputElement | null>(null);

  const ext = (name: string) => name.split('.').pop()?.toLowerCase() ?? '';
  const source = $derived<ImportSource | null>(
    kind === 'paste'
      ? text.trim()
        ? { kind: 'text', text }
        : null
      : file
        ? file.path
          ? { kind: 'file', path: file.path }
          : { kind: 'text', text: file.text ?? '' }
        : null,
  );
  /** Format asked for: the advanced choice, else (a browser-read file) its extension. */
  const askedFormat = $derived<ImportFormat | null>(
    format !== 'auto' ? format : file && !file.path && ext(file.name) === 'json' ? 'json' : null,
  );
  const isPackage = $derived(preview?.package != null);
  const isText = $derived(!!preview && !isPackage && preview.format !== 'json');
  const dataRows = $derived(preview ? preview.rows.slice(header ? 1 : 0) : []);
  const totalCards = $derived(preview ? (isPackage ? (preview.package?.notes ?? 0) : Math.max(0, preview.total_rows - (header && isText ? 1 : 0))) : 0);
  const step = $derived(result ? 3 : job ? 3 : preview ? 2 : 1);
  const attributesOn = $derived(moduleKnownOn('attributes'));
  const canStart = $derived(!!preview && !job && totalCards > 0 && (isPackage || destination.ready) && (!isText || columns.some((c) => c.kind === 'field')));

  /** Field choices of a column: role order first, each with what it holds. */
  const columnOptions = $derived([
    { value: 'ignore', label: t('io.map.ignore') },
    ...fillOrder(nt).map((f) => {
      const role = fieldRole(nt, f);
      return { value: `field:${f.key}`, label: role ? `${f.name} · ${t(`add.role.${role}`)}` : f.name };
    }),
    { value: 'tags', label: t('io.map.tags') },
    ...(attributesOn ? attributes.map((a) => ({ value: `attr:${a.def.id}`, label: a.def.name })) : []),
  ]);
  const policyOptions = $derived(POLICIES.map((p) => ({ value: p, label: t(`io.dup.${p}`) })));
  const formatOptions = $derived([
    { value: 'auto' as ImportFormat | 'auto', label: t('io.import.format_auto') },
    ...FORMATS.map((f) => ({ value: f as ImportFormat | 'auto', label: t(`io.formats.${f}`) })),
  ]);
  /** Fields that decide "already there" (the note type's unique fields). */
  const uniqueNames = $derived((nt?.fields ?? []).filter((f) => f.unique).map((f) => f.name));
  /** Columns with text that go nowhere. */
  const dropped = $derived(
    isText ? columns.map((c, i) => (c.kind === 'ignore' && dataRows.some((r) => (r[i] ?? '').trim()) ? i + 1 : 0)).filter(Boolean) : [],
  );
  /** A note type that keeps an example sentence, offered when this one would drop a column. */
  const withExample = $derived(
    nt && !fieldWithRole(nt, 'example') && dropped.length > 0 ? (notetypes.find((n) => fieldWithRole(n, 'example')) ?? null) : null,
  );

  function fail(e: unknown) {
    error = td((e as ApiError).messageKey ?? 'errors.internal.unexpected');
  }

  // Classifications offered as column targets (only while the module is on).
  $effect(() => {
    trackModules();
    const deck = destination.deckId;
    attributes = [];
    if (deck && attributesOn) void api.attribute.effective(deck).then((r) => (attributes = r.attributes), () => {});
  });

  async function runPreview(keepOptions: boolean) {
    error = null;
    if (!source || !nt) {
      preview = null;
      options = null;
      return;
    }
    loading = true;
    const before = preview?.columns;
    try {
      const p = await api.call('io.import.preview', {
        source,
        format: askedFormat,
        text_options: keepOptions ? $state.snapshot(options) : null,
        notetype_id: nt.id,
        deck_id: destination.deckId,
        limit: 20,
      });
      preview = p;
      options = p.text_options;
      // Fresh data (or another column count): start from the pre-filled mapping again.
      if (!keepOptions || before !== p.columns || columns.length !== p.columns) prefill(p);
    } catch (e) {
      preview = null;
      fail(e);
    } finally {
      loading = false;
    }
  }

  function prefill(p: ImportPreview) {
    if (!nt) return;
    const attrs = attributes.map((a) => ({ id: a.def.id, key: a.def.key, name: a.def.name }));
    const m = prefillMapping(p.rows, p.columns, nt, loadMapping(nt.id), attrs);
    // Classification / tag columns the backend recognised in a header it also matched.
    columns = m.columns.map((c, i) => resolveColumn(c.kind === 'ignore' && m.header && p.suggested_columns[i]?.kind !== 'field' ? (p.suggested_columns[i] ?? c) : c, nt));
    header = m.header;
    origin = m.origin;
  }

  // Re-read when the source changes (pasting: debounced) or another note type is chosen.
  let timer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    const fresh = source;
    void nt?.id;
    void askedFormat;
    untrack(() => {
      // New data replaces the last result (clearing the box after an add keeps it).
      if (fresh) result = null;
      clearTimeout(timer);
      timer = setTimeout(() => void runPreview(false), kind === 'paste' ? 300 : 0);
    });
    return () => clearTimeout(timer);
  });

  // The first cards as they will look (debounced; text rows only).
  $effect(() => {
    const rows = dataRows.slice(0, 3);
    const cols = $state.snapshot(columns);
    const type = nt;
    const deck = destination.deckId;
    if (!isText || !type || rows.length === 0) {
      cards = [];
      return;
    }
    let cancelled = false;
    const id = setTimeout(() => {
      void Promise.all(
        rows.map((cells) => {
          const fields: Record<string, string> = {};
          cols.forEach((c, i) => {
            if (c.kind === 'field' && cells[i]) fields[c.key] = cells[i]!;
          });
          return api.call('cards.preview', { notetype_id: type.id, definition: null, fields, tags: [], deck_id: deck }).then(
            (r) => r.cards[0] ?? null,
            () => null,
          );
        }),
      ).then((list) => {
        if (!cancelled) cards = list;
      });
    }, 200);
    return () => {
      cancelled = true;
      clearTimeout(id);
    };
  });

  // ---- step 1: the file ----

  async function choose() {
    if (isTauri()) {
      const picked = await pickFile([{ name: t('io.file.filter'), extensions: EXTENSIONS }], t('io.path_prompt'));
      if (picked) usePath(picked);
    } else {
      fileInput?.click();
    }
  }

  function usePath(path: string) {
    error = null;
    if (!EXTENSIONS.includes(ext(path))) {
      error = t('io.file.unsupported', { list: EXTENSIONS.map((e) => `.${e}`).join(', ') });
      return;
    }
    file = { name: path.split(/[\\/]/).pop() ?? path, path, text: null, size: null };
  }

  async function useFile(f: File | undefined) {
    if (!f) return;
    error = null;
    if (!EXTENSIONS.includes(ext(f.name))) {
      error = t('io.file.unsupported', { list: EXTENSIONS.map((e) => `.${e}`).join(', ') });
      return;
    }
    if (BINARY.includes(ext(f.name))) {
      // A browser has no path to give the backend; the app's picker / drop does.
      error = t('io.file.binary_in_browser');
      return;
    }
    file = { name: f.name, path: null, text: await f.text(), size: f.size };
  }

  function ondrop(e: DragEvent) {
    e.preventDefault();
    dragging = false;
    void useFile(e.dataTransfer?.files?.[0]);
  }

  // In the app the window gets dropped files as paths (Tauri's drag-and-drop).
  onMount(() => {
    if (kind !== 'file' || !isTauri()) return;
    let unlisten: (() => void) | null = null;
    let gone = false;
    void import('@tauri-apps/api/webview')
      .then(({ getCurrentWebview }) =>
        getCurrentWebview().onDragDropEvent((ev) => {
          const p = ev.payload;
          if (p.type === 'over' || p.type === 'enter') dragging = true;
          else if (p.type === 'leave') dragging = false;
          else if (p.type === 'drop') {
            dragging = false;
            if (p.paths[0] && !job) usePath(p.paths[0]);
          }
        }),
      )
      .then((u) => {
        if (gone) u();
        else unlisten = u;
      })
      .catch(() => {});
    return () => {
      gone = true;
      unlisten?.();
    };
  });

  function reset() {
    file = null;
    text = '';
    preview = null;
    options = null;
    result = null;
    error = null;
  }

  // ---- step 3: import ----

  function issueText(issue: ImportIssue): string {
    const msg = td(issue.message.key, issue.message.params);
    return issue.row > 0 ? `${t('io.import.row', { row: issue.row })}: ${msg}` : msg;
  }

  const reasons = $derived.by(() => {
    if (!result) return [];
    const r = result.report;
    const lines = r.skipped_duplicates > 0 ? [t('io.result.duplicates', { count: r.skipped_duplicates })] : [];
    return [...lines, ...r.issues.map(issueText)];
  });

  async function start() {
    if (!source || !preview || !nt || !canStart) return;
    error = null;
    result = null;
    let target: { id: string; created: boolean } | null = null;
    if (!isPackage) {
      target = await destination.ensure();
      if (!target) return;
    }
    try {
      const started = await api.call('io.import.start', {
        source,
        format: preview.format,
        text_options: isPackage ? null : $state.snapshot(options),
        deck_id: isPackage ? destination.packageParent : target!.id,
        notetype_id: isText ? nt.id : null,
        columns: isText ? $state.snapshot(columns) : null,
        skip_first_row: isText ? header : false,
        duplicates,
        create_missing_options: createMissing,
        include_history: includeHistory,
      });
      if (isText) {
        saveMapping(nt.id, { header: header ? (preview.rows[0] ?? null) : null, columns: columns.map(encodeColumn) });
      }
      job = started;
      const done = await followJob(started, (j) => (job = j));
      job = null;
      if (done.status === 'cancelled') error = t('io.job.cancelled');
      else if (done.error) error = td(done.error.message_key);
      else {
        const report = resultOf<ImportReport>(done);
        if (report) {
          result = { report, deckId: target?.id ?? null, steps: 1 + (target?.created ? 1 : 0) };
          if (kind === 'paste') text = '';
          preview = null;
          ondone?.();
        }
      }
    } catch (e) {
      job = null;
      fail(e);
    }
  }

  async function undo() {
    const steps = result?.steps ?? 0;
    for (let i = 0; i < steps; i++) await api.call('oplog.undo', {});
  }

  /** Paste: the rows into the grid (≤ 200, mapped fields only). */
  function toGrid() {
    if (!onrows || !preview) return;
    const rows = dataRows.map((cells) => {
      const row = blankRow();
      columns.forEach((c, i) => {
        if (c.kind === 'field' && cells[i] !== undefined) row.fields[c.key] = cells[i]!;
      });
      return row;
    });
    onrows(rows);
    text = '';
  }

  function onkey(e: KeyboardEvent) {
    // Ctrl+Enter adds, as everywhere on the add page.
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter' && !e.defaultPrevented && !e.repeat && canStart) {
      e.preventDefault();
      void start();
    }
  }

  const stepLabels = $derived(
    kind === 'paste'
      ? [t('io.steps.paste'), t('io.steps.check'), t('io.steps.add')]
      : [t('io.steps.file'), t('io.steps.map'), t('io.steps.import')],
  );
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div class="flow" onkeydown={onkey}>
  <ol class="steps" aria-label={t('io.steps.label')}>
    {#each stepLabels as label, i (i)}
      <li class:on={step === i + 1} class:done={step > i + 1} aria-current={step === i + 1 ? 'step' : undefined}>
        <span class="no num">{i + 1}</span>{label}
      </li>
    {/each}
  </ol>

  {#if result}
    <AddedSummary
      {ctx}
      added={result.report.added_notes}
      updated={result.report.updated_notes}
      skipped={result.report.skipped_duplicates}
      {reasons}
      deckId={result.deckId}
      onundo={undo}
    />
    <div><Button size="sm" variant="ghost" onclick={reset}>{kind === 'paste' ? t('io.again_paste') : t('io.again_file')}</Button></div>
  {/if}

  {#if kind === 'paste' && !job}
    <Textarea
      label={t('io.paste.label')}
      hint={t('io.paste.hint')}
      bind:value={text}
      rows={6}
      autoGrow
      maxRows={14}
      placeholder={t('io.paste.placeholder')}
      spellcheck="false"
      data-testid="paste-text"
    />
  {:else if kind === 'file' && !job}
    {#if file}
      <div class="file-line">
        <Icon icon={Upload} size={16} />
        <span class="fname">{file.name}</span>
        {#if file.size !== null}<span class="muted num">{formatMegabytes(file.size)}</span>{/if}
        <Button size="sm" variant="ghost" onclick={() => void choose()}>{t('io.file.change')}</Button>
      </div>
    {:else}
      <div
        class="drop"
        class:dragging
        role="group"
        aria-label={t('io.file.drop_label')}
        ondragover={(e) => {
          e.preventDefault();
          dragging = true;
        }}
        ondragleave={() => (dragging = false)}
        {ondrop}
      >
        <Icon icon={Upload} size={20} />
        <p class="drop-title">{t('io.file.drop')}</p>
        <Button variant="primary" icon={FolderOpen} onclick={() => void choose()}>{t('io.file.choose')}</Button>
        <p class="muted small">{t('io.file.formats')}</p>
        <p class="muted small example">{t('io.file.example')}</p>
      </div>
    {/if}
    <input
      bind:this={fileInput}
      class="hidden-input"
      type="file"
      accept={EXTENSIONS.map((e) => `.${e}`).join(',')}
      tabindex="-1"
      aria-hidden="true"
      data-testid="file-input"
      onchange={(e) => {
        void useFile(e.currentTarget.files?.[0]);
        e.currentTarget.value = '';
      }}
    />
  {/if}

  {#if error}<p class="note bad" role="alert"><Icon icon={CircleAlert} size={16} /><span>{error}</span></p>{/if}

  {#if loading && !preview}
    <Skeleton lines={3} />
  {/if}

  {#if preview && !job}
    <section class="map" aria-label={stepLabels[1]}>
      {#if isPackage && preview.package}
        <p class="lead">
          {t('io.package.contents', {
            notes: formatCount(preview.package.notes),
            cards: formatCount(preview.package.cards),
            decks: formatCount(preview.package.decks.length),
            media: formatCount(preview.package.media),
          })}
        </p>
        <p class="muted small">{t('io.package.where')}</p>
        <Checkbox label={t('io.import.include_history')} description={t('io.package.history_hint')} bind:checked={includeHistory} />
      {:else if preview.total_rows === 0}
        <p class="muted">{kind === 'paste' ? t('io.paste.nothing') : t('io.file.empty')}</p>
      {:else}
        {#if options && isText}
          <Separators {options} confidence={preview.confidence} onchange={(o) => ((options = o), void runPreview(true))} />
        {/if}
        <p class="lead">
          {t('io.map.found', { count: totalCards })}
          <span class="muted">· {t(`io.map.origin.${origin}`)}</span>
        </p>

        {#if isText}
          <div class="table-wrap">
            <table>
              <thead>
                <tr>
                  {#each columns as column, i (i)}
                    <th scope="col">
                      <Select
                        size="sm"
                        hideLabel
                        label={t('io.import.column', { n: i + 1 })}
                        options={columnOptions}
                        value={encodeColumn(column)}
                        onchange={(v) => {
                          columns[i] = decodeColumn(v);
                          origin = 'order';
                        }}
                      />
                    </th>
                  {/each}
                </tr>
              </thead>
              <tbody>
                {#each preview.rows.slice(0, 6) as row, r (r)}
                  <tr class:header={header && r === 0}>
                    {#each columns as _c, c (c)}<td class:off={columns[c]?.kind === 'ignore'}>{row[c] ?? ''}</td>{/each}
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
          <Checkbox label={t('io.import.skip_first_row')} bind:checked={header} />
          {#if dropped.length > 0}
            <p class="note warn" role="note">
              <Icon icon={Info} size={16} />
              <span>{t('io.map.dropped', { cols: dropped.join(', ') })}</span>
              {#if withExample}
                <Button size="sm" variant="ghost" onclick={() => onchoosetype(withExample!.id)}>{t('io.map.use_type', { name: withExample.name })}</Button>
              {/if}
            </p>
          {/if}
          {#if cards.length > 0}
            <div class="cards" aria-label={t('io.map.cards')}>
              <h3>{t('io.map.cards')}</h3>
              <ol>
                {#each cards as c, i (i)}
                  <li>
                    {#if c}
                      <div class="side"><span class="lbl">{t('io.map.front')}</span><CardView sides={c} interactive={false} /></div>
                      <div class="side back"><span class="lbl">{t('io.map.back')}</span><CardView sides={c} showAnswer interactive={false} /></div>
                    {:else}
                      <p class="muted small">{t('io.map.no_card')}</p>
                    {/if}
                  </li>
                {/each}
              </ol>
            </div>
          {/if}
        {/if}

        <div class="dup">
          <Select label={t('io.dup.label')} options={policyOptions} bind:value={duplicates} size="sm" />
          <p class="muted small">
            {uniqueNames.length > 0
              ? t(`io.dup.explain.${duplicates}`, { fields: uniqueNames.join(', ') })
              : t('io.dup.explain.none')}
          </p>
        </div>
        {#if attributesOn && columns.some((c) => c.kind === 'attribute')}
          <Checkbox label={t('io.import.create_missing_options')} bind:checked={createMissing} />
        {/if}
      {/if}

      {#if kind === 'file'}
        <details class="adv" bind:open={advanced}>
          <summary>{t('io.file.advanced')}</summary>
          <Select label={t('io.import.format')} options={formatOptions} bind:value={format} size="sm" />
        </details>
      {/if}
    </section>

    <div class="go">
      <Button variant="primary" icon={Upload} kbd="Mod+Enter" disabled={!canStart} onclick={() => void start()} data-testid="import-start">
        {kind === 'paste' ? t('io.paste.add', { count: totalCards }) : t('io.file.import', { count: totalCards })}
      </Button>
      {#if !isPackage && !destination.ready}<span class="muted small">{t('io.need_deck')}</span>{/if}
      {#if kind === 'paste' && onrows && isText && totalCards > 0 && totalCards <= GRID_MAX}
        <Button variant="ghost" onclick={toGrid}>{t('io.paste.to_grid')}</Button>
      {/if}
    </div>
  {/if}

  {#if job}
    <JobProgress {job} />
  {/if}
</div>

<style>
  .flow {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  .steps {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1) var(--space-4);
    margin: 0;
    padding: 0;
    list-style: none;
    font-size: var(--text-xs);
    color: var(--text-3);
  }
  .steps li {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
  }
  .steps li.on {
    color: var(--text-1);
    font-weight: var(--weight-medium);
  }
  .steps li.done {
    color: var(--text-2);
  }
  .no {
    display: inline-grid;
    place-items: center;
    width: 18px;
    height: 18px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-pill);
    font-size: 11px;
  }
  .steps li.on .no {
    border-color: var(--accent);
    background: var(--accent);
    color: var(--accent-fg);
  }
  .drop {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-6) var(--space-4);
    border: 1px dashed var(--border-strong);
    border-radius: var(--radius-card);
    background: var(--surface-1);
    color: var(--text-2);
    text-align: center;
  }
  .drop.dragging {
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .drop-title {
    margin: 0;
    color: var(--text-1);
    font-size: var(--text-md);
    font-weight: var(--weight-medium);
  }
  .example {
    font-family: var(--font-mono);
  }
  .file-line {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-wrap: wrap;
    color: var(--text-2);
  }
  .fname {
    color: var(--text-1);
    font-family: var(--font-mono);
    font-size: var(--text-sm);
    overflow-wrap: anywhere;
  }
  .hidden-input {
    display: none;
  }
  .map {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    padding: var(--space-4);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--surface-1);
  }
  .lead {
    margin: 0;
    font-size: var(--text-sm);
    color: var(--text-1);
  }
  .muted {
    color: var(--text-2);
    margin: 0;
  }
  .small {
    font-size: var(--text-sm);
  }
  .table-wrap {
    border: 1px solid var(--border);
    border-radius: var(--radius-control);
    overflow: auto;
    max-height: 320px;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: var(--text-sm);
  }
  th {
    position: sticky;
    top: 0;
    z-index: 1;
    padding: var(--space-2);
    background: var(--surface-2);
    border-bottom: 1px solid var(--border);
    text-align: start;
    font-weight: var(--weight-medium);
    min-width: 10rem;
  }
  td {
    padding: var(--space-2) var(--space-3);
    border-bottom: 1px solid var(--border);
    white-space: pre-wrap;
    color: var(--text-1);
    vertical-align: top;
  }
  td.off {
    color: var(--text-3);
    text-decoration: line-through;
  }
  tbody tr:last-child td {
    border-bottom: none;
  }
  tr.header td {
    color: var(--text-2);
    font-style: italic;
    text-decoration: none;
  }
  .note {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    align-items: center;
    margin: 0;
    font-size: var(--text-sm);
    color: var(--text-2);
  }
  .note :global(svg) {
    flex: none;
  }
  .bad {
    color: var(--danger);
  }
  .warn {
    color: var(--warning);
  }
  .cards h3 {
    margin: 0 0 var(--space-2);
    font-size: var(--text-xs);
    font-weight: var(--weight-medium);
    color: var(--text-2);
  }
  .cards ol {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(260px, 1fr));
    gap: var(--space-2);
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .cards li {
    display: grid;
    grid-template-columns: 1fr 1fr;
    border: 1px solid var(--border);
    border-radius: var(--radius-control);
    background: var(--surface-0);
    overflow: hidden;
    font-size: var(--text-sm);
  }
  .side {
    min-width: 0;
    padding: var(--space-2) var(--space-3);
    max-height: 140px;
    overflow: hidden;
  }
  .side.back {
    border-left: 1px solid var(--border);
  }
  .lbl {
    display: block;
    margin-bottom: var(--space-1);
    font-size: var(--text-xs);
    color: var(--text-3);
  }
  .dup {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    max-width: 36rem;
  }
  .adv summary {
    cursor: pointer;
    color: var(--text-2);
    font-size: var(--text-sm);
  }
  .adv :global(.fc-field) {
    margin-top: var(--space-2);
    max-width: 20rem;
  }
  .go {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    flex-wrap: wrap;
  }
</style>
