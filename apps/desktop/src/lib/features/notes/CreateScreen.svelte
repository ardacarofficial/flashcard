<!--
  Create (docs/11 §3): set details (title, description, deck, note type, classifications) and
  numbered term | definition rows. Tab moves field to field across rows, Enter on the last
  field of the last row adds a row. "Save" is ONE `notes.create_many` command — one undo
  step, a new deck included. Paste to import uses the v0.5 separator detection.
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
    Card,
    ClipboardPaste,
    EmptyState,
    IconButton,
    ImagePlus,
    Input,
    PageHeader,
    Plus,
    SegmentedControl,
    Select,
    Skeleton,
    Sparkles,
    Textarea,
    toast,
    Trash2,
    X,
  } from '$lib/ui';
  import { mediaSnippet } from '../cards/content';
  import { resolveMedia, uploadMedia } from '../cards/media';
  import { moduleKnownOn, moduleOn, trackModules } from '../cards/modules.svelte';
  import { draftKeeper } from '../maintenance/drafts';
  import type { ScreenCtx } from '../types';
  import { appendRows, blankRow, blankRows, editableFields, isBlank, toDrafts, type Row } from './create';
  import NoteEditor from '../cards/NoteEditor.svelte';
  import PastePanel from './PastePanel.svelte';

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
  let pasteOpen = $state(false);
  let thumbs = $state<Record<number, string>>({});
  let uploading = $state<number | null>(null);

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
    if (nt?.id === CLASSIC_NOTETYPE_ID || nt?.builtin === 'classic') {
      if (i === 0) return t('create.term');
      if (i === 1) return t('create.definition');
    }
    return name;
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
    else if (rich) mode = 'existing';
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
    } else if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
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
      const message = dupes
        ? t('create.saved_with_duplicates', { count: r.note_ids.length, dupes })
        : t('create.saved', { count: r.note_ids.length });
      // Keep the duplicate rows on screen so they can be changed and saved again.
      const kept = r.duplicate_rows.map((i) => sent[i]!).filter(Boolean);
      reset();
      if (kept.length) {
        rows = [...kept, blankRow()];
        rowErrors = Object.fromEntries(kept.map((_, i) => [i, t('create.duplicate_row')]));
      }
      ctx.selectDeck(r.deck_id);
      toast.show(message, {
        tone: dupes ? 'warning' : 'success',
        action: { label: t('create.undo'), run: () => void ctx.guarded(() => api.call('oplog.undo', {})) },
      });
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
    if ((e.ctrlKey || e.metaKey) && e.key === 'Enter' && canSave) {
      e.preventDefault();
      void save();
    }
  }
</script>

<svelte:window onkeydown={onGlobalKey} />

<div class="create">
  <PageHeader title={t('create.heading')} subtitle={t('create.subtitle')}>
    {#snippet actions()}
      {#if aiOn}
        <Button variant="ghost" icon={Sparkles} onclick={() => ctx.navigate?.('/ai')}>{t('create.ai')}</Button>
      {/if}
      {#if !rich}
        <Button icon={ClipboardPaste} onclick={() => (pasteOpen = true)} data-testid="open-dialog-paste" disabled={!nt}>
          {t('create.paste.open')}
        </Button>
        <Button variant="primary" onclick={() => void save()} loading={busy} disabled={!canSave}>{t('create.save')}</Button>
      {/if}
    {/snippet}
  </PageHeader>

  {#if loadError}
    <Card padding="lg">
      <EmptyState title={t('create.load_failed')} description={loadError} action={{ label: t('create.retry'), onclick: () => void loadTypes() }} />
    </Card>
  {:else if !notetypes}
    <div class="skeleton" aria-busy="true" aria-label={t('create.loading')}>
      <Skeleton shape="card" height="180px" />
      <Skeleton shape="card" height="96px" />
      <Skeleton shape="card" height="96px" />
    </div>
  {:else}
    <form
      class="body"
      novalidate
      onsubmit={(e) => {
        e.preventDefault();
        void save();
      }}
    >
      <Card padding="lg" class="details">
        <div class="where">
          <SegmentedControl
            label={t('create.where')}
            options={[
              { value: 'new', label: t('create.mode_new'), disabled: rich },
              { value: 'existing', label: t('create.mode_existing'), disabled: ctx.decks.length === 0 },
            ]}
            bind:value={mode}
          />
        </div>

        {#if mode === 'new'}
          <Input
            label={t('create.title')}
            bind:value={title}
            size="lg"
            placeholder={t('create.title_placeholder')}
            error={titleError}
            required
            oninput={() => (titleError = null)}
          />
          <Textarea label={t('create.description')} bind:value={description} rows={2} autoGrow maxRows={6} placeholder={t('create.description_placeholder')} />
        {/if}

        <div class="grid">
          {#if mode === 'new'}
            <Select label={t('create.parent')} options={parentOptions} bind:value={parent} />
          {:else}
            <Select label={t('create.deck')} options={deckOptions} bind:value={deckId} placeholder={t('create.choose_deck')} />
          {/if}
          <Select label={t('create.notetype')} options={typeOptions} value={ntId} onchange={chooseType} />
          {#each defs as def (def.id)}
            <AttributeField {def} bind:value={attrs[def.id]!} />
          {/each}
        </div>
      </Card>

      {#if !rich}
        <div class="rows-head">
          <h2>{t('create.cards_heading')}</h2>
          <span class="count">{t('create.count', { count: filled })}</span>
        </div>
      {/if}

      {#if rich}
        {#if deckId}
          {@const path = paths.find((p) => p.id === deckId)?.path ?? ''}
          <Card padding="lg">
            <NoteEditor {deckId} deckName={path} fixedType={ntId} onsaved={() => ctx.selectDeck(deckId)} />
          </Card>
        {:else}
          <Card padding="lg"><EmptyState size="sm" title={t('create.choose_deck')} /></Card>
        {/if}
      {:else if fields.length === 0}
        <Card padding="lg">
          <EmptyState size="sm" title={t('create.no_fields')} description={t('create.no_fields_hint')} />
        </Card>
      {:else}
        <ol class="rows" bind:this={list} aria-label={t('create.cards_heading')}>
          {#each rows as row, i (row.key)}
            <li class="row" class:invalid={rowErrors[i]}>
              <div class="row-top">
                <span class="num" aria-hidden="true">{i + 1}</span>
                <div class="row-actions">
                  <IconButton
                    size="sm"
                    icon={ImagePlus}
                    label={t('create.add_image', { n: i + 1 })}
                    loading={uploading === row.key}
                    onclick={() => pickImage(i)}
                  />
                  <IconButton size="sm" icon={Trash2} label={t('create.remove_row', { n: i + 1 })} onclick={() => removeRow(i)} />
                </div>
              </div>
              <div class="cells" class:has-image={!!row.image} style:--cols={Math.min(fields.length, 3)}>
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
                      class="cell-input"
                    />
                    <span class="caption" aria-hidden="true">{caption(c, f.name)}</span>
                  </div>
                {/each}
                {#if row.image}
                  <div class="thumb">
                    {#if thumbs[row.key]}<img src={thumbs[row.key]} alt={t('create.image_alt', { n: i + 1 })} />{/if}
                    <IconButton size="sm" variant="secondary" icon={X} label={t('create.remove_image', { n: i + 1 })} onclick={() => (row.image = null)} />
                  </div>
                {/if}
              </div>
              {#if rowErrors[i]}<p class="row-error" role="alert">{rowErrors[i]}</p>{/if}
            </li>
          {/each}
        </ol>
        <Button size="lg" fullWidth icon={Plus} class="add-row" onclick={() => addRow()}>{t('create.add_row')}</Button>
        <p class="hint">{t('create.keys_hint')}</p>
      {/if}

      {#if error}<p class="form-error" role="alert">{error}</p>{/if}

      {#if !rich}
        <div class="footer">
          <Button variant="primary" size="lg" type="submit" loading={busy} disabled={!canSave}>{t('create.save')}</Button>
        </div>
      {/if}
    </form>
  {/if}
</div>

<input bind:this={fileInput} class="file" type="file" accept="image/*" tabindex="-1" aria-hidden="true" onchange={(e) => void onFile(e.currentTarget.files)} />

{#if nt}
  <PastePanel
    bind:open={pasteOpen}
    notetypeId={nt.id}
    deckId={targetDeck}
    columns={fields.map((f, c) => ({ key: f.key, label: caption(c, f.name) }))}
    onadd={onPasted}
  />
{/if}

<style>
  .create {
    max-width: 960px;
    margin: 0 auto;
    display: flex;
    flex-direction: column;
    gap: var(--space-6);
  }
  .skeleton,
  .body {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  .body :global(.details) {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(220px, 1fr));
    gap: var(--space-4);
    align-items: start;
  }
  .rows-head {
    display: flex;
    align-items: baseline;
    justify-content: space-between;
    margin-top: var(--space-2);
  }
  h2 {
    margin: 0;
    font-size: var(--text-lg);
    font-weight: var(--weight-semibold);
  }
  .count {
    color: var(--text-2);
    font-size: var(--text-sm);
  }
  .rows {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  .row {
    background: var(--surface-1);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    padding: var(--space-2) var(--space-4) var(--space-4) var(--space-4);
    transition: border-color var(--motion-fast) var(--ease);
  }
  .row:focus-within {
    border-color: var(--accent);
  }
  .row.invalid {
    border-color: var(--danger);
  }
  .row-top {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: var(--space-1);
  }
  .num {
    font-weight: var(--weight-semibold);
    color: var(--text-2);
    font-size: var(--text-sm);
    font-variant-numeric: tabular-nums;
  }
  .row-actions {
    display: flex;
    gap: var(--space-1);
  }
  .cells {
    display: grid;
    grid-template-columns: repeat(var(--cols), minmax(0, 1fr));
    gap: var(--space-6);
    align-items: start;
  }
  .cells.has-image {
    grid-template-columns: repeat(var(--cols), minmax(0, 1fr)) auto;
  }
  .cell {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    grid-column: span 1;
  }
  .cells .cell:last-of-type {
    grid-column: span 1;
  }
  .create :global(.fc-page-header) {
    margin-bottom: 0;
  }
  .cell :global(textarea) {
    border: none;
    border-bottom: 2px solid var(--border-strong);
    border-radius: 0;
    background: transparent;
    padding-inline: 0;
    resize: none;
    font-size: var(--text-md);
    box-shadow: none;
  }
  .cell :global(textarea:focus) {
    border-bottom-color: var(--accent);
    outline: none;
  }
  .caption {
    font-size: var(--text-xs);
    font-weight: var(--weight-semibold);
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--text-3);
  }
  .thumb {
    position: relative;
    width: 96px;
    height: 72px;
    border-radius: var(--radius-control);
    overflow: hidden;
    border: 1px solid var(--border);
    background: var(--surface-2);
  }
  .thumb img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .thumb :global(button) {
    position: absolute;
    top: var(--space-1);
    right: var(--space-1);
  }
  .row-error {
    margin: var(--space-2) 0 0;
    color: var(--danger);
    font-size: var(--text-sm);
  }
  .body :global(.add-row) {
    min-height: 64px;
    border: 2px dashed var(--border-strong);
    background: transparent;
    color: var(--text-2);
    box-shadow: none;
  }
  .body :global(.add-row:hover) {
    color: var(--accent);
    border-color: var(--accent);
    background: var(--accent-soft);
  }
  .hint {
    margin: 0;
    color: var(--text-3);
    font-size: var(--text-xs);
    text-align: center;
  }
  .form-error {
    margin: 0;
    color: var(--danger);
  }
  .footer {
    display: flex;
    justify-content: flex-end;
  }
  .file {
    display: none;
  }
  @media (max-width: 720px) {
    .cells {
      grid-template-columns: minmax(0, 1fr);
      gap: var(--space-3);
    }
  }
</style>
