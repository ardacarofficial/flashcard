<!--
  Add cards (v1.3): ONE destination with four ways in — one card (Quick Add, `N`), bulk (the
  grid), paste (text → detected separators → preview → add) and from a file (CSV/TSV, Anki,
  Quizlet, JSON, deck packages; guided: choose → map → import). All four share the setup row
  (new/existing deck, title or deck, parent, note type; description and classifications under
  "More") and end with the same confirmation ("12 cards added · Undo · Open deck · Study now").

  Bulk: a spreadsheet of numbered rows, one column per field. Tab moves field to field across
  rows, Enter on the last field of the last row adds a row, Ctrl+Enter saves. "Save" is ONE
  `notes.create_many` command — one undo step, a new deck included. Paste and file go through
  `io.import.*` (Rust parses, maps and imports; one undo step).
-->
<script lang="ts">
  import { onMount, tick, untrack } from 'svelte';
  import { api, CLASSIC_NOTETYPE_ID, type ApiError } from '$lib/api';
  import type { AttributeDefDto, MediaDto, NoteTypeDto } from '$lib/api/types';
  import AttributeField from '$lib/components/AttributeField.svelte';
  import { initialValue, toAssignments, type FieldValue } from '$lib/components/attribute-field';
  import { deckPaths } from '$lib/components/deck-paths';
  import { t, td } from '$lib/i18n/index.svelte';
  import {
    Button,
    ChevronDown,
    ClipboardPaste,
    Columns3,
    EmptyState,
    FileText,
    Icon,
    IconButton,
    ImagePlus,
    Input,
    PageHeader,
    Plus,
    SegmentedControl,
    Select,
    Skeleton,
    Sparkles,
    Tabs,
    Textarea,
    toast,
    Trash2,
    X,
  } from '$lib/ui';
  import { openQuickAdd } from '$lib/shell/commands.svelte';
  import { mediaSnippet } from '../cards/content';
  import { resolveMedia, uploadMedia } from '../cards/media';
  import { moduleKnownOn, moduleOn, trackModules } from '../cards/modules.svelte';
  import { draftKeeper } from '../maintenance/drafts';
  import type { ScreenCtx } from '../types';
  import { appendRows, blankRow, blankRows, classicColumn, editableFields, isBlank, toDrafts, type Row } from './create';
  import NoteEditor from '../cards/NoteEditor.svelte';
  import ImportFlow, { type Destination } from '../io/ImportFlow.svelte';
  import AddedSummary from './AddedSummary.svelte';
  import { ADD_WAYS, addWayRequest, lastAddWay, rememberAddWay, takeAddWay, type AddWay } from './add-way.svelte';

  let { ctx }: { ctx: ScreenCtx } = $props();

  const LAST_TYPE = 'fc.cards.last_notetype';
  const TOP = '';
  type Mode = 'new' | 'existing';

  let notetypes = $state<NoteTypeDto[] | null>(null);
  let loadError = $state<string | null>(null);
  let ntId = $state<string>(lastType());
  let mode = $state<Mode>(untrack(() => (ctx.selectedDeck ? 'existing' : 'new')));
  let title = $state('');
  let description = $state('');
  let parent = $state<string>(TOP);
  let deckId = $state<string | null>(untrack(() => ctx.selectedDeck));
  let defs = $state<AttributeDefDto[]>([]);
  let attrs = $state<Record<string, FieldValue>>({});
  let rows = $state<Row[]>(blankRows(3));
  let busy = $state(false);
  let error = $state<string | null>(null);
  let titleError = $state<string | null>(null);
  let rowErrors = $state<Record<number, string>>({});
  /** The way in shown below the tabs (a request from Quick Add / elsewhere, else the last one). */
  const asked = takeAddWay();
  let way = $state<AddWay>(asked?.way ?? lastAddWay());
  if (asked?.deckId) {
    mode = 'existing';
    deckId = asked.deckId;
  }
  /** The last bulk save (the shared confirmation above the grid). */
  let saved = $state<{ added: number; skipped: number; deckId: string; at: number } | null>(null);
  let thumbs = $state<Record<number, string>>({});
  let uploading = $state<number | null>(null);
  /** "More": description and classifications (opens by itself when one is required). */
  let moreOpen = $state(false);

  const nt = $derived(notetypes?.find((n) => n.id === ntId) ?? null);
  const fields = $derived(editableFields(nt));
  /**
   * Note types with fields a row cannot hold (audio, video, occlusion masks) get the full
   * note editor, one note at a time, into an existing deck.
   */
  const rich = $derived(!!nt && nt.fields.some((f) => !fields.includes(f) && f.kind !== 'image'));
  const filled = $derived(rows.filter((r) => !isBlank(r)).length);
  const paths = $derived(deckPaths(ctx.decks));
  const deckOptions = $derived(paths.map((p) => ({ value: p.id, label: p.path })));
  const parentOptions = $derived([{ value: TOP, label: t('create.top_level') }, ...deckOptions]);
  const typeOptions = $derived((notetypes ?? []).map((n) => ({ value: n.id, label: n.name })));
  const targetDeck = $derived(mode === 'new' ? parent || null : deckId);
  const canSave = $derived(!!nt && filled > 0 && !busy && (mode === 'new' ? !!title.trim() : !!deckId));
  const aiOn = $derived(moduleOn('ai') && !!ctx.navigate);
  const wayTabs = $derived([
    { id: 'single', label: t('add.way.single'), icon: Plus },
    { id: 'bulk', label: t('add.way.bulk'), icon: Columns3 },
    { id: 'paste', label: t('add.way.paste'), icon: ClipboardPaste },
    { id: 'file', label: t('add.way.file'), icon: FileText },
  ]);

  function chooseWay(id: string) {
    if (!(ADD_WAYS as readonly string[]).includes(id)) return;
    way = id as AddWay;
    rememberAddWay(way);
  }

  // A request while the page is open (Quick Add's "Paste" / "From a file").
  $effect(() => {
    void addWayRequest().seq;
    untrack(() => {
      const r = takeAddWay();
      if (!r) return;
      way = r.way;
      if (r.deckId && paths.some((p) => p.id === r.deckId)) {
        mode = 'existing';
        deckId = r.deckId;
      }
    });
  });

  /** The deck for paste / file: the chosen one, or the new one created now (its own undo step). */
  async function ensureDeck(): Promise<{ id: string; created: boolean } | null> {
    if (mode === 'existing') return deckId ? { id: deckId, created: false } : null;
    if (!title.trim()) {
      titleError = t('create.title_required');
      document.querySelector<HTMLInputElement>('.create .setup .title input')?.focus();
      return null;
    }
    try {
      const d = await api.call('decks.create', { path: title.trim(), parent_id: parent || null, description: description.trim() || null });
      ctx.selectDeck(d.id);
      deckId = d.id;
      mode = 'existing';
      title = '';
      description = '';
      return { id: d.id, created: true };
    } catch (e) {
      const err = e as ApiError;
      titleError = td(err.messageKey ?? 'errors.internal.unexpected');
      return null;
    }
  }

  const destination = $derived<Destination>({
    ready: mode === 'new' ? !!title.trim() : !!deckId,
    deckId: mode === 'existing' ? deckId : null,
    packageParent: mode === 'new' ? parent || null : deckId,
    ensure: ensureDeck,
  });

  /** "One card": Quick Add on the chosen deck (a new deck is created first). */
  async function single() {
    if (mode === 'existing') {
      openQuickAdd({ deckId });
      return;
    }
    if (!title.trim()) {
      // No decks at all: Quick Add itself says to make one first.
      if (ctx.decks.length === 0) return void openQuickAdd();
      titleError = t('create.title_required');
      document.querySelector<HTMLInputElement>('.create .setup .title input')?.focus();
      return;
    }
    const made = await ensureDeck();
    if (!made) return;
    // Quick Add picks from the shell's deck list: wait (briefly) until the new deck is in it.
    for (let i = 0; i < 20 && !paths.some((p) => p.id === made.id); i++) await new Promise((r) => setTimeout(r, 50));
    openQuickAdd({ deckId: made.id });
  }

  function lastType(): string {
    try {
      return localStorage.getItem(LAST_TYPE) ?? CLASSIC_NOTETYPE_ID;
    } catch {
      return CLASSIC_NOTETYPE_ID;
    }
  }

  function chooseType(id: string) {
    ntId = id;
    try {
      localStorage.setItem(LAST_TYPE, id);
    } catch {
      /* ignore */
    }
  }

  /** Caption of a field column: Term / Definition for Classic, else the field's own name. */
  function caption(i: number, name: string): string {
    const col = classicColumn(nt, i);
    return col ? t(`create.${col}`) : name;
  }

  async function loadTypes() {
    loadError = null;
    try {
      const r = await api.call('notetypes.list', {});
      notetypes = r.notetypes;
      if (!r.notetypes.some((n) => n.id === ntId)) ntId = CLASSIC_NOTETYPE_ID;
    } catch (e) {
      loadError = td((e as ApiError).messageKey ?? 'errors.internal.unexpected');
    }
  }

  onMount(() => {
    trackModules();
    void loadTypes();
  });

  // No decks yet: only "New deck" makes sense; the full editor needs an existing one.
  $effect(() => {
    if (ctx.decks.length === 0) mode = 'new';
    else if (rich && way === 'bulk') mode = 'existing';
  });

  // "Existing deck" without a choice yet: the selected deck (the shell's picker, the last save).
  $effect(() => {
    if (mode === 'existing' && !deckId && ctx.selectedDeck && paths.some((p) => p.id === ctx.selectedDeck)) {
      deckId = ctx.selectedDeck;
    }
  });

  // Classifications of the target deck (a new deck gets its parent's, or the collection's).
  $effect(() => {
    const deck = targetDeck;
    if (!moduleKnownOn('attributes')) {
      defs = [];
      return;
    }
    let cancelled = false;
    api.call('attributes.effective', { deck_id: deck, target: 'note' }).then(
      (list) => {
        if (cancelled) return;
        const d = list.attributes.map((a) => a.def).sort((a, b) => a.position - b.position);
        defs = d;
        if (d.some((def) => def.required)) moreOpen = true;
        untrack(() => {
          attrs = Object.fromEntries(d.map((def) => [def.id, attrs[def.id] ?? initialValue(def)]));
        });
      },
      () => (defs = []),
    );
    return () => {
      cancelled = true;
    };
  });

  // v1.0 crash safety: what is typed survives (restored once on open, dropped on save).
  interface Draft {
    title: string;
    description: string;
    notetype_id: string;
    rows: { fields: Record<string, string>; image: string | null }[];
  }
  const draft = draftKeeper<Draft>('notes.create_set');
  let restored = $state(false);
  onMount(() => {
    void draft.restore().then((d) => {
      if (d && Array.isArray(d.rows) && d.rows.length > 0) {
        title = d.title ?? '';
        description = d.description ?? '';
        if (d.notetype_id) ntId = d.notetype_id;
        rows = d.rows.map((r) => ({ ...blankRow(), fields: { ...r.fields }, image: r.image ?? null }));
      }
      restored = true;
    });
  });
  $effect(() => {
    if (!restored) return;
    const snap = $state.snapshot(rows);
    const worth = title.trim() || snap.some((r) => !isBlank(r));
    draft.save(worth ? { title, description, notetype_id: ntId, rows: snap.map((r) => ({ fields: r.fields, image: r.image })) } : null);
  });

  // Thumbnails of attached images.
  $effect(() => {
    for (const r of rows) {
      const img = r.image;
      const key = r.key;
      if (!img || untrack(() => thumbs[key])) continue;
      const hash = img.match(/media:([0-9a-f]{64})/)?.[1];
      if (!hash) continue;
      void resolveMedia(`<img src="media:${hash}" alt="">`).then((html) => {
        thumbs = { ...thumbs, [key]: html.match(/src="([^"]*)"/)?.[1] ?? '' };
      });
    }
  });

  // ---- rows and keyboard ----

  let list = $state<HTMLOListElement | null>(null);

  function fieldEl(row: number, col: number): HTMLTextAreaElement | null {
    return list?.querySelector<HTMLTextAreaElement>(`[data-cell="${row}:${col}"]`) ?? null;
  }

  async function focusCell(row: number, col: number) {
    await tick();
    fieldEl(row, col)?.focus();
  }

  function addRow(focus = true) {
    rows = [...rows, blankRow()];
    if (focus) void focusCell(rows.length - 1, 0);
  }

  function removeRow(i: number) {
    rows = rows.length > 1 ? rows.filter((_, k) => k !== i) : [blankRow()];
    rowErrors = {};
  }

  function onCellKey(e: KeyboardEvent, row: number, col: number) {
    const lastCol = col === fields.length - 1;
    if (e.key === 'Tab' && !e.shiftKey && lastCol) {
      // Straight to the next row's first field (the row's buttons come after it in Tab order).
      e.preventDefault();
      if (row === rows.length - 1) addRow();
      else void focusCell(row + 1, 0);
    } else if (e.key === 'Enter' && !e.shiftKey && !e.isComposing && !e.ctrlKey && !e.metaKey && !e.altKey) {
      // (Ctrl+Enter is not a cell move: it bubbles to the page and saves.)
      e.preventDefault();
      if (!lastCol) void focusCell(row, col + 1);
      else if (row === rows.length - 1) addRow();
      else void focusCell(row + 1, 0);
    }
  }

  let fileInput = $state<HTMLInputElement | null>(null);
  let imageRow = -1;

  function pickImage(i: number) {
    imageRow = i;
    fileInput?.click();
  }

  async function onFile(files: FileList | null) {
    const file = files?.[0];
    const i = imageRow;
    if (!file || i < 0) return;
    uploading = rows[i]?.key ?? null;
    try {
      const media: MediaDto = await uploadMedia(file);
      rows[i]!.image = mediaSnippet(media);
    } catch (e) {
      toast.show(td((e as ApiError).messageKey ?? 'errors.internal.unexpected'), { tone: 'danger' });
    } finally {
      uploading = null;
      if (fileInput) fileInput.value = '';
    }
  }

  function onPasted(added: Row[]) {
    rows = appendRows(rows, added.length ? added : [blankRow()]);
    if (!isBlank(rows[rows.length - 1]!)) rows = [...rows, blankRow()];
    chooseWay('bulk');
    toast.show(t('create.paste.added', { count: added.length }), { tone: 'success' });
  }

  // ---- save ----

  function reset() {
    title = '';
    description = '';
    rows = blankRows(3);
    thumbs = {};
    rowErrors = {};
  }

  async function save() {
    if (!nt) return;
    error = null;
    titleError = null;
    rowErrors = {};
    if (mode === 'new' && !title.trim()) {
      titleError = t('create.title_required');
      return;
    }
    busy = true;
    const sent = rows;
    try {
      const r = await api.call('notes.create_many', {
        notetype_id: nt.id,
        target:
          mode === 'new'
            ? { kind: 'new_deck', path: title.trim(), parent_id: parent || null, description: description.trim() || null }
            : { kind: 'deck', id: deckId! },
        notes: toDrafts(sent, nt),
        tags: [],
        attributes: toAssignments($state.snapshot(attrs)),
        allow_duplicate: null,
      });
      await draft.discard();
      const dupes = r.duplicate_rows.length;
      // Keep the duplicate rows on screen so they can be changed and saved again.
      const kept = r.duplicate_rows.map((i) => sent[i]!).filter(Boolean);
      reset();
      if (kept.length) {
        rows = [...kept, blankRow()];
        rowErrors = Object.fromEntries(kept.map((_, i) => [i, t('create.duplicate_row')]));
      }
      ctx.selectDeck(r.deck_id);
      // The next rows go to the deck just filled.
      deckId = r.deck_id;
      mode = 'existing';
      saved = { added: r.note_ids.length, skipped: dupes, deckId: r.deck_id, at: Date.now() };
    } catch (e) {
      const err = e as ApiError;
      const row = typeof err.details?.row === 'number' ? err.details.row : null;
      const message = td(err.messageKey ?? 'errors.internal.unexpected');
      if (row !== null) {
        rowErrors = { [row]: message };
        void focusCell(row, 0);
      } else if (err.code === 'CONFLICT_DECK_EXISTS' || err.code === 'VALIDATION_DECK_NAME') {
        titleError = message;
      } else {
        error = message;
      }
    } finally {
      busy = false;
    }
  }

  function onGlobalKey(e: KeyboardEvent) {
    // Not for a dialog above the page (Quick Add, paste): Ctrl+Enter is theirs there.
    if (e.defaultPrevented || document.querySelector('[aria-modal="true"]')) return;
    if (!(e.ctrlKey || e.metaKey) || e.key !== 'Enter' || rich || way !== 'bulk') return;
    if (canSave) {
      e.preventDefault();
      void save();
    } else if (nt && !busy && filled > 0 && mode === 'new' && !title.trim()) {
      // Rows typed but no title: say so and put the cursor there instead of doing nothing.
      e.preventDefault();
      titleError = t('create.title_required');
      document.querySelector<HTMLInputElement>('.create .setup .title input')?.focus();
    }
  }
</script>

<svelte:window onkeydown={onGlobalKey} />

<div class="create">
  <PageHeader title={t('create.heading')} subtitle={t('add.subtitle')}>
    {#snippet actions()}
      {#if aiOn}
        <Button variant="ghost" icon={Sparkles} onclick={() => ctx.navigate?.('/ai')}>{t('create.ai')}</Button>
      {/if}
    {/snippet}
  </PageHeader>

  <Tabs variant="pill" label={t('add.way.label')} tabs={wayTabs} value={way} onchange={chooseWay} class="ways" />

  {#if loadError}
    <EmptyState title={t('create.load_failed')} description={loadError} action={{ label: t('create.retry'), onclick: () => void loadTypes() }} />
  {:else if !notetypes}
    <div class="skeleton" aria-busy="true" aria-label={t('create.loading')}>
      <Skeleton shape="control" height="52px" />
      <Skeleton shape="card" height="180px" />
    </div>
  {:else}
    <div class="body">
      <div class="setup">
        <div class="mode">
          <span class="lbl" aria-hidden="true">{t('create.where')}</span>
          <SegmentedControl
            size="sm"
            label={t('create.where')}
            options={[
              { value: 'new', label: t('create.mode_new'), disabled: rich && way === 'bulk' },
              { value: 'existing', label: t('create.mode_existing'), disabled: ctx.decks.length === 0 },
            ]}
            bind:value={mode}
          />
        </div>
        {#if mode === 'new'}
          <Input
            class="title"
            size="sm"
            label={t('create.title')}
            bind:value={title}
            placeholder={t('create.title_placeholder')}
            error={titleError}
            required
            oninput={() => (titleError = null)}
          />
          <Select class="deck" size="sm" label={t('create.parent')} options={parentOptions} bind:value={parent} />
        {:else}
          <Select class="title" size="sm" label={t('create.deck')} options={deckOptions} bind:value={deckId} placeholder={t('create.choose_deck')} />
        {/if}
        <Select class="type" size="sm" label={t('create.notetype')} options={typeOptions} value={ntId} onchange={chooseType} />
        {#if mode === 'new' || defs.length > 0}
          <Button
            size="sm"
            variant="ghost"
            class="more-btn"
            aria-expanded={moreOpen}
            aria-controls="create-more"
            title={t('create.more_hint')}
            onclick={() => (moreOpen = !moreOpen)}
          >
            {t('create.more')}
            <span class="chev" class:open={moreOpen}><Icon icon={ChevronDown} size={14} /></span>
          </Button>
        {/if}
      </div>

      {#if moreOpen && (mode === 'new' || defs.length > 0)}
        <div class="more" id="create-more">
          {#if mode === 'new'}
            <Textarea
              class="desc"
              label={t('create.description')}
              bind:value={description}
              rows={1}
              autoGrow
              maxRows={6}
              placeholder={t('create.description_placeholder')}
            />
          {/if}
          {#each defs as def (def.id)}
            <AttributeField size="sm" {def} bind:value={attrs[def.id]!} />
          {/each}
        </div>
      {/if}

      {#if way === 'single'}
        <div class="panel single">
          <p class="lead">{t('add.single.lead')}</p>
          <p class="example">{t('add.single.example')}</p>
          <div><Button variant="primary" icon={Plus} kbd="N" onclick={() => void single()}>{t('add.single.open')}</Button></div>
        </div>
      {:else if way === 'paste' || way === 'file'}
        {#key way}
          <ImportFlow
            {ctx}
            kind={way}
            {nt}
            notetypes={notetypes ?? []}
            onchoosetype={chooseType}
            {destination}
            onrows={way === 'paste' && !rich ? onPasted : undefined}
          />
        {/key}
      {:else}
      <form
        class="bulk"
        novalidate
        onsubmit={(e) => {
          e.preventDefault();
          void save();
        }}
      >
      {#if saved}
        {#key saved.at}
          <AddedSummary
            {ctx}
            added={saved.added}
            skipped={saved.skipped}
            reasons={saved.skipped > 0 ? [t('create.duplicates_kept', { count: saved.skipped })] : []}
            deckId={saved.deckId}
            onundo={async () => {
              await api.call('oplog.undo', {});
            }}
          />
        {/key}
      {/if}
      {#if rich}
        {#if deckId}
          {@const path = paths.find((p) => p.id === deckId)?.path ?? ''}
          <div class="panel">
            <NoteEditor {deckId} deckName={path} fixedType={ntId} onsaved={() => ctx.selectDeck(deckId)} />
          </div>
        {:else}
          <EmptyState size="sm" title={t('create.choose_deck')} />
        {/if}
      {:else if fields.length === 0}
        <EmptyState size="sm" title={t('create.no_fields')} description={t('create.no_fields_hint')} />
      {:else}
        <div class="sheet" style:--cols={fields.length}>
          <div class="sheet-head" aria-hidden="true">
            <span></span>
            {#each fields as f, c (f.key)}<span class="col">{caption(c, f.name)}</span>{/each}
            <span></span>
          </div>
          <ol class="rows" bind:this={list} aria-label={t('create.cards_heading')}>
            {#each rows as row, i (row.key)}
              <li class="row" class:invalid={!!rowErrors[i]}>
                <span class="n num" aria-hidden="true">{i + 1}</span>
                {#each fields as f, c (f.key)}
                  <div class="cell">
                    <Textarea
                      label={`${caption(c, f.name)} ${i + 1}`}
                      hideLabel
                      rows={1}
                      autoGrow
                      maxRows={8}
                      value={row.fields[f.key] ?? ''}
                      oninput={(e) => {
                        row.fields[f.key] = e.currentTarget.value;
                        if (rowErrors[i]) rowErrors = {};
                      }}
                      onkeydown={(e) => onCellKey(e, i, c)}
                      data-cell="{i}:{c}"
                      aria-invalid={rowErrors[i] ? true : undefined}
                    />
                  </div>
                {/each}
                <div class="acts">
                  {#if row.image}
                    <span class="thumb">
                      {#if thumbs[row.key]}<img src={thumbs[row.key]} alt={t('create.image_alt', { n: i + 1 })} />{/if}
                      <IconButton size="sm" variant="secondary" icon={X} label={t('create.remove_image', { n: i + 1 })} onclick={() => (row.image = null)} />
                    </span>
                  {/if}
                  <IconButton
                    size="sm"
                    icon={ImagePlus}
                    label={t('create.add_image', { n: i + 1 })}
                    loading={uploading === row.key}
                    onclick={() => pickImage(i)}
                  />
                  <IconButton size="sm" icon={Trash2} label={t('create.remove_row', { n: i + 1 })} onclick={() => removeRow(i)} />
                </div>
                {#if rowErrors[i]}<p class="row-error" role="alert">{rowErrors[i]}</p>{/if}
              </li>
            {/each}
          </ol>
          <button type="button" class="add-row" onclick={() => addRow()}>
            <Icon icon={Plus} size={14} />
            <span>{t('create.add_row')}</span>
          </button>
        </div>
      {/if}

      {#if !rich}
        <div class="savebar">
          <span class="hint">{t('create.keys_hint')}</span>
          {#if error}<p class="form-error" role="alert">{error}</p>{/if}
          <span class="count num" aria-live="polite">{t('create.count', { count: filled })}</span>
          <Button variant="primary" type="submit" kbd="Mod+Enter" loading={busy} disabled={!canSave}>{t('create.save')}</Button>
        </div>
      {/if}
      </form>
      {/if}
    </div>
  {/if}
</div>

<input bind:this={fileInput} class="file" type="file" accept="image/*" tabindex="-1" aria-hidden="true" onchange={(e) => void onFile(e.currentTarget.files)} />

<style>
  .create {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  .create :global(.fc-page-header) {
    margin-bottom: 0;
  }
  .bulk {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  .single {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    align-items: flex-start;
  }
  .single .lead {
    margin: 0;
    color: var(--text-1);
  }
  .single .example {
    margin: 0 0 var(--space-2);
    color: var(--text-2);
    font-family: var(--font-mono);
    font-size: var(--text-sm);
  }
  .skeleton,
  .body {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }

  /* ---- the one setup row ---- */
  .setup {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-start;
    gap: var(--space-2) var(--space-3);
  }
  .mode {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }
  .lbl {
    font-size: var(--text-xs);
    font-weight: var(--weight-medium);
    color: var(--text-2);
  }
  .setup :global(.title) {
    flex: 2 1 160px;
  }
  .setup :global(.deck) {
    flex: 1 1 150px;
  }
  .setup :global(.type) {
    flex: 0 1 170px;
    min-width: 120px;
  }
  .setup :global(.more-btn) {
    /* Lines up with the controls under their labels (label line + gap). */
    margin-top: calc(var(--text-xs) * 1.45 + var(--space-1));
  }
  .chev {
    display: inline-flex;
    transition: transform var(--motion-fast) var(--ease);
  }
  .chev.open {
    transform: rotate(180deg);
  }
  .more {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(200px, 1fr));
    gap: var(--space-2) var(--space-3);
    padding: var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--surface-1);
  }
  .more :global(.desc) {
    grid-column: 1 / -1;
  }
  .more :global(textarea) {
    resize: none;
  }
  .panel {
    padding: var(--space-4);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--surface-1);
  }

  /* ---- the sheet: numbered rows, one column per field ---- */
  .sheet {
    --grid: 40px repeat(var(--cols), minmax(0, 1fr)) 96px;
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--surface-1);
    overflow: hidden;
  }
  .sheet-head,
  .row {
    display: grid;
    grid-template-columns: var(--grid);
  }
  .sheet-head {
    min-height: 30px;
    align-items: center;
    background: var(--surface-2);
    border-bottom: 1px solid var(--border);
    font-size: var(--text-xs);
    font-weight: var(--weight-medium);
    color: var(--text-2);
  }
  .sheet-head .col {
    padding: 0 var(--space-3);
    border-left: 1px solid var(--border);
    line-height: 30px;
  }
  .rows {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .row {
    align-items: stretch;
    border-bottom: 1px solid var(--border);
  }
  .row:focus-within {
    background: var(--surface-2);
  }
  .row.invalid {
    background: var(--danger-soft);
  }
  .n {
    padding: 9px var(--space-3) 0 0;
    text-align: right;
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .row:focus-within .n {
    color: var(--accent-text);
  }
  .row.invalid .n {
    color: var(--danger);
  }
  .cell {
    border-left: 1px solid var(--border);
    min-width: 0;
  }
  .cell :global(textarea) {
    display: block;
    min-height: 36px;
    padding: 8px var(--space-3);
    border: 0;
    border-radius: 0;
    background: transparent;
    resize: none;
  }
  .cell :global(textarea:hover:not(:disabled)) {
    background: var(--surface-2);
  }
  .cell :global(textarea:focus-visible) {
    background: var(--surface-0);
    outline: 2px solid var(--focus-ring);
    outline-offset: -2px;
  }
  .acts {
    display: flex;
    align-items: flex-start;
    justify-content: flex-end;
    gap: 2px;
    padding: 4px var(--space-2) 0 0;
    border-left: 1px solid var(--border);
  }
  .acts :global(.fc-iconbtn) {
    color: var(--text-3);
  }
  .acts :global(.fc-iconbtn:hover),
  .acts :global(.fc-iconbtn:focus-visible) {
    color: var(--text-1);
  }
  .thumb {
    position: relative;
    width: 28px;
    height: 28px;
    border-radius: var(--radius-control);
    overflow: hidden;
    border: 1px solid var(--border);
    background: var(--surface-2);
    flex: none;
  }
  .thumb img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .thumb :global(.fc-iconbtn) {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    opacity: 0;
  }
  .thumb:hover :global(.fc-iconbtn),
  .thumb :global(.fc-iconbtn:focus-visible) {
    opacity: 1;
  }
  .row-error {
    grid-column: 2 / -1;
    margin: 0;
    padding: 0 var(--space-3) var(--space-2);
    color: var(--danger);
    font-size: var(--text-xs);
  }
  .add-row {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    height: 36px;
    padding: 0 var(--space-3) 0 26px;
    border: 0;
    background: transparent;
    color: var(--text-2);
    font: inherit;
    font-size: var(--text-sm);
    cursor: pointer;
    text-align: left;
  }
  .add-row:hover {
    background: var(--surface-2);
    color: var(--text-1);
  }
  .add-row:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: -2px;
  }

  /* ---- save bar: sticks to the bottom while the rows scroll ---- */
  .savebar {
    position: sticky;
    bottom: 0;
    z-index: 1;
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-3) 0;
    border-top: 1px solid var(--border);
    background: var(--surface-0);
  }
  .hint {
    margin-right: auto;
    color: var(--text-2);
    font-size: var(--text-xs);
  }
  .count {
    color: var(--text-2);
    font-size: var(--text-sm);
    white-space: nowrap;
  }
  .form-error {
    margin: 0;
    color: var(--danger);
    font-size: var(--text-sm);
  }
  .file {
    display: none;
  }
  @media (max-width: 720px) {
    .sheet {
      --grid: 32px repeat(var(--cols), minmax(0, 1fr)) 64px;
    }
    .hint {
      display: none;
    }
    .savebar {
      justify-content: flex-end;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .chev {
      transition: none;
    }
  }
</style>
