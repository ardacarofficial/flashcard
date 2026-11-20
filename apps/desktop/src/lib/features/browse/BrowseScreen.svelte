<!--
  Search (docs/11 §2–3): query bar with syntax help and saved searches, a virtualised result
  table with a column chooser, a bulk actions bar for the selection and a card info side
  sheet. The query language, sorting and every change run in Rust (`search.*`, `bulk.*`).
-->
<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import { api, type ApiError } from '$lib/api';
  import type {
    AttributeDefDto,
    BulkResult,
    CardInfo,
    CardRow,
    CardSortKey,
    FlagDto,
    NoteTypeDto,
    QueryErrorDto,
    SavedQueryDto,
  } from '$lib/api/types';
  import { i18n, t, td } from '$lib/i18n/index.svelte';
  import { deckPaths } from '$lib/components/deck-paths';
  import {
    ArrowDown,
    ArrowUp,
    Button,
    Checkbox,
    Chip,
    CircleHelp,
    Columns3,
    Dialog,
    EmptyState,
    Icon,
    IconButton,
    Input,
    Menu,
    PageHeader,
    Search,
    SegmentedControl,
    Select,
    Skeleton,
    Star,
    Table,
    toast,
    type MenuItem,
    type TableColumn,
  } from '$lib/ui';
  import { moduleKnownOn, trackModules } from '../cards/modules.svelte';
  import NoteEditDialog from '../cards/NoteEditDialog.svelte';
  import type { ScreenCtx } from '../types';
  import BulkBar from './BulkBar.svelte';
  import CardInfoSheet from './CardInfoSheet.svelte';
  import { COLUMNS, loadColumns, saveColumns, stateLabel, stateTone, type ColumnId } from './columns';
  import QueryHelp from './QueryHelp.svelte';

  let { ctx }: { ctx: ScreenCtx } = $props();

  const PAGE = 100;

  let query = $state('');
  let ran = $state('');
  let notesOnly = $state(false);
  let sort = $state<CardSortKey>('sort_field');
  let descending = $state(false);
  let rows = $state<CardRow[]>([]);
  let total = $state(0);
  let loaded = $state(false);
  let loading = $state(false);
  let loadError = $state<string | null>(null);
  let queryError = $state<QueryErrorDto | null>(null);
  let selected = $state<Map<string, string>>(new Map()); // card → note
  let saved = $state<SavedQueryDto[]>([]);
  let info = $state<CardInfo | null>(null);
  let editing = $state<CardInfo | null>(null);
  let confirmDelete = $state(false);
  let attrs = $state<AttributeDefDto[]>([]);
  let flagDefs = $state<FlagDto[]>([]);
  let noteTypes = $state<NoteTypeDto[]>([]);
  let helpOpen = $state(false);
  let columnsOpen = $state(false);
  let saveOpen = $state(false);
  let saveName = $state('');
  let shown = $state<ColumnId[]>(loadColumns());
  let activeKey = $state<string | null>(null);

  const paths = $derived(deckPaths(ctx.decks));
  const cards = $derived([...selected.keys()]);
  const notes = $derived([...new Set(selected.values())]);
  /** Notes of a "select all" (cards outside the loaded page have no row to map from). */
  let allNotes = $state<string[] | null>(null);
  const noteSelection = $derived(allNotes ?? notes.filter(Boolean));
  const selectedKeys = $derived(new Set(selected.keys()));
  const allChecked = $derived(total > 0 && selected.size >= total);

  const fmtDate = (ms: number | null) => (ms === null ? t('common.none') : new Date(ms).toLocaleDateString(i18n.locale));

  const tableColumns = $derived<TableColumn<CardRow>[]>([
    { id: 'select', header: '', width: '44px' },
    ...COLUMNS.filter((c) => shown.includes(c.id)).map((c) => ({
      id: c.id,
      header: td(c.label) + (c.sort && c.sort === sort ? (descending ? ' ↓' : ' ↑') : ''),
      width: c.width,
      align: c.id === 'lapses' || c.id === 'reps' ? ('end' as const) : ('start' as const),
    })),
  ]);

  const sortOptions = $derived(
    COLUMNS.filter((c) => c.sort).map((c) => ({ value: c.sort as CardSortKey, label: td(c.label) })),
  );

  function errorText(e: QueryErrorDto): string {
    const msg = td(`errors.query.${e.kind}`, e.params);
    return `${msg} (${t('browse.error_at', { start: e.span.start + 1, end: e.span.end })})`;
  }

  async function check() {
    try {
      queryError = (await api.call('search.check', { query })).error;
    } catch {
      queryError = null;
    }
  }

  let timer: ReturnType<typeof setTimeout> | undefined;
  function onInput() {
    clearTimeout(timer);
    timer = setTimeout(() => void check(), 250);
  }

  async function run(append = false) {
    const offset = append ? rows.length : 0;
    loading = true;
    loadError = null;
    try {
      const page = await api.call('search.cards', { query, sort, descending, notes_only: notesOnly, offset, limit: PAGE });
      rows = append ? [...rows, ...page.items] : page.items;
      total = page.total;
      ran = query;
      queryError = null;
      if (!append) {
        selected = new Map();
        allNotes = null;
      }
    } catch (e) {
      const err = e as ApiError;
      if (err.code === 'VALIDATION_QUERY') {
        queryError = {
          kind: String(err.details.kind),
          span: { start: Number(err.details.start), end: Number(err.details.end) },
          params: Object.fromEntries(
            Object.entries(err.details)
              .filter(([k]) => !['kind', 'start', 'end'].includes(k))
              .map(([k, v]) => [k, String(v)]),
          ),
        };
      } else if (!loaded) {
        loadError = td(err.messageKey ?? 'errors.internal.unexpected');
      } else {
        await ctx.guarded(() => Promise.reject(e));
      }
    } finally {
      loaded = true;
      loading = false;
    }
  }

  async function refresh() {
    query = ran;
    await run();
  }

  async function loadSaved() {
    try {
      saved = (await api.call('saved_queries.list', {})).items;
    } catch {
      saved = [];
    }
  }

  function setSort(key: CardSortKey) {
    sort = key;
    void run();
  }

  function toggle(row: CardRow) {
    allNotes = null;
    const next = new Map(selected);
    if (next.has(row.card_id)) next.delete(row.card_id);
    else next.set(row.card_id, row.note_id);
    selected = next;
  }

  function clearSelection() {
    selected = new Map();
    allNotes = null;
  }

  async function selectAll() {
    await ctx.guarded(async () => {
      const ids = await api.call('search.ids', { query: ran });
      const byCard = new Map(rows.map((r) => [r.card_id, r.note_id]));
      selected = new Map(ids.card_ids.map((c) => [c, byCard.get(c) ?? '']));
      allNotes = ids.note_ids;
    });
  }

  async function bulk(fn: () => Promise<BulkResult>): Promise<boolean> {
    let message = '';
    const ok = await ctx.guarded(async () => {
      const r = await fn();
      message = t('browse.bulk.done', { changed: r.changed, skipped: r.skipped });
    });
    if (ok) {
      toast.show(message, { tone: 'success', action: { label: t('create.undo'), run: () => void ctx.guarded(() => api.call('oplog.undo', {})) } });
      await refresh();
    }
    return ok;
  }

  async function runOther(fn: () => Promise<unknown>): Promise<boolean> {
    let message: unknown = null;
    const ok = await ctx.guarded(async () => (message = await fn()));
    if (ok) {
      if (typeof message === 'string') toast.show(message, { tone: 'success' });
      await refresh();
    }
    return ok;
  }

  async function showInfo(cardId: string) {
    await ctx.guarded(async () => (info = await api.call('browse.card_info', { card_id: cardId })));
  }

  async function saveQuery() {
    const ok = await ctx.guarded(() => api.call('saved_queries.create', { name: saveName.trim(), query: ran || query, icon: null, position: null }));
    if (ok) {
      saveName = '';
      saveOpen = false;
      toast.show(t('browse.saved_done'), { tone: 'success' });
      await loadSaved();
    }
  }

  async function deleteNotes() {
    confirmDelete = false;
    const n = noteSelection.length;
    const ok = await ctx.guarded(() => api.call('notes.delete', { ids: noteSelection }));
    if (ok) {
      toast.show(t('browse.deleted', { count: n }), { action: { label: t('create.undo'), run: () => void ctx.guarded(() => api.call('oplog.undo', {})) } });
      await refresh();
    }
  }

  function useSaved(s: SavedQueryDto) {
    query = s.query;
    void run();
  }

  const savedItems = $derived<MenuItem[]>([
    ...saved.map((s) => ({ id: `q-${s.id}`, label: s.name, onselect: () => useSaved(s) })),
    ...(saved.length ? [{ id: 'sep', separator: true as const }] : []),
    { id: 'save', label: `${t('browse.save')}…`, disabled: !(ran || query).trim(), onselect: () => (saveOpen = true) },
    ...saved.map((s) => ({
      id: `del-${s.id}`,
      label: t('browse.delete_saved', { name: s.name }),
      danger: true,
      onselect: () => void ctx.guarded(() => api.call('saved_queries.delete', { id: s.id })).then(() => loadSaved()),
    })),
  ]);

  function toggleColumn(id: ColumnId, on: boolean) {
    shown = on ? COLUMNS.filter((c) => c.id === id || shown.includes(c.id)).map((c) => c.id) : shown.filter((c) => c !== id);
    saveColumns(shown);
  }

  function onShortcut(e: Event) {
    const action = (e as CustomEvent<string>).detail;
    if (action === 'browse.select_all') void selectAll();
    else if (action === 'browse.suspend' && cards.length) void bulk(() => api.call('bulk.set_suspended', { card_ids: cards, suspended: true }));
    else if (action === 'browse.info' && (activeKey ?? cards[0])) void showInfo((activeKey ?? cards[0])!);
    else if (action === 'browse.delete' && cards.length) confirmDelete = true;
  }

  // Bulk classification: only while the `attributes` module is on (otherwise hidden).
  $effect(() => {
    trackModules();
    if (!moduleKnownOn('attributes')) {
      attrs = [];
      return;
    }
    untrack(() => void api.call('attributes.list', {}).then((r) => (attrs = r.defs), () => (attrs = [])));
  });

  onMount(() => {
    void run();
    void loadSaved();
    void api.call('flags.list', {}).then((r) => (flagDefs = r.flags), () => (flagDefs = []));
    void api.call('notetypes.list', {}).then((r) => (noteTypes = r.notetypes), () => (noteTypes = []));
    const unsub = api.on(['saved_queries', 'flags_changed', 'undone', 'redone', 'data_changed'], () => {
      void loadSaved();
      void refresh();
    });
    window.addEventListener('fc:shortcut', onShortcut);
    return () => {
      void unsub.then((u) => u());
      window.removeEventListener('fc:shortcut', onShortcut);
    };
  });
</script>

<div class="browse">
  <header class="head">
    <PageHeader title={t('browse.heading')} class="page-head" />
    <form
      class="query"
      role="search"
      onsubmit={(e) => {
        e.preventDefault();
        void run();
      }}
    >
      <Input
        class="q"
        type="search"
        icon={Search}
        size="lg"
        label={t('browse.query_label')}
        hideLabel
        bind:value={query}
        oninput={onInput}
        placeholder={t('browse.query_placeholder')}
        error={queryError ? errorText(queryError) : null}
        spellcheck="false"
        autocomplete="off"
      />
      <IconButton icon={CircleHelp} variant="secondary" size="lg" label={t('browse.syntax.open')} pressed={helpOpen} onclick={() => (helpOpen = !helpOpen)} />
      <Menu items={savedItems} label={t('browse.saved')} triggerClass="saved-trigger">
        {#snippet trigger()}<span class="saved"><Icon icon={Star} size={16} />{t('browse.saved_short')}</span>{/snippet}
      </Menu>
      <Button type="submit" variant="primary" size="lg">{t('browse.run')}</Button>
    </form>
    {#if helpOpen}
      <QueryHelp onpick={(q) => ((query = q), void run())} />
    {/if}
  </header>

  <div class="toolbar">
    <div class="left">
      {#if total > 0}
        <Checkbox
          checked={allChecked}
          indeterminate={selected.size > 0 && !allChecked}
          ariaLabel={t('browse.select_all', { count: total })}
          onchange={(on) => (on ? void selectAll() : clearSelection())}
        />
      {/if}
      <span class="summary" aria-live="polite">
        {notesOnly ? t('browse.results_notes', { count: total }) : t('browse.results', { count: total })}
      </span>
    </div>
    <div class="right">
      <SegmentedControl
        size="sm"
        label={t('browse.view')}
        options={[
          { value: false, label: t('browse.view_cards') },
          { value: true, label: t('browse.view_notes') },
        ]}
        bind:value={notesOnly}
        onchange={() => void run()}
      />
      <Select size="sm" label={t('browse.sort')} hideLabel options={sortOptions} value={sort} onchange={setSort} class="sort" />
      <IconButton
        size="sm"
        variant="secondary"
        icon={descending ? ArrowDown : ArrowUp}
        label={descending ? t('browse.sort_desc') : t('browse.sort_asc')}
        onclick={() => ((descending = !descending), void run())}
      />
      <IconButton size="sm" variant="secondary" icon={Columns3} label={t('browse.columns')} onclick={() => (columnsOpen = true)} data-testid="open-dialog-columns" />
    </div>
  </div>

  {#if selected.size > 0}
    <BulkBar
      count={selected.size}
      {cards}
      notes={noteSelection}
      decks={paths}
      flags={flagDefs}
      {attrs}
      {noteTypes}
      {bulk}
      run={runOther}
      onclear={clearSelection}
      bind:deleting={confirmDelete}
    />
  {/if}

  {#if loadError}
    <EmptyState title={t('browse.load_failed')} description={loadError} action={{ label: t('create.retry'), onclick: () => void run() }} />
  {:else if !loaded}
    <div class="skeleton" aria-busy="true" aria-label={t('create.loading')}>
      {#each [0, 1, 2, 3, 4, 5] as i (i)}<Skeleton shape="control" height="44px" />{/each}
    </div>
  {:else if total === 0 && !ran.trim()}
    <EmptyState
      title={t('browse.empty_collection')}
      description={t('browse.empty_collection_hint')}
      action={ctx.navigate ? { label: t('browse.create_cards'), onclick: () => ctx.navigate?.('/notes/new') } : undefined}
    />
  {:else}
    <Table
      {rows}
      columns={tableColumns}
      rowKey={(r) => r.card_id}
      label={t('browse.table')}
      height={`min(${(rows.length + 1) * 44 + 2}px, calc(100vh - 280px))`}
      {selectedKeys}
      bind:activeKey
      onrowclick={(r) => void showInfo(r.card_id)}
    >
      {#snippet cell(r, c)}
        {#if c.id === 'select'}
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
          <span class="check" onclick={(e) => e.stopPropagation()}>
            <Checkbox checked={selected.has(r.card_id)} ariaLabel={t('browse.select_row', { name: r.sort_field })} onchange={() => toggle(r)} />
          </span>
        {:else if c.id === 'sort_field'}
          <span class="name">{r.sort_field || t('common.none')}</span>
        {:else if c.id === 'deck'}
          <span class="muted ellipsis">{r.deck_path}</span>
        {:else if c.id === 'state'}
          <Chip size="sm" tone={stateTone(r)} label={stateLabel(r)} />
        {:else if c.id === 'due'}
          <span class="num">{fmtDate(r.due)}</span>
        {:else if c.id === 'lapses'}
          <span class="num">{r.lapses}</span>
        {:else if c.id === 'reps'}
          <span class="num">{r.reps}</span>
        {:else if c.id === 'tags'}
          <span class="muted ellipsis">{r.tags.map((x) => `#${x}`).join(' ')}</span>
        {:else if c.id === 'created'}
          <span class="num">{fmtDate(r.created_at)}</span>
        {/if}
      {/snippet}
      {#snippet empty()}
        <div class="no-match">
          <EmptyState size="sm" title={t('browse.empty')} description={t('browse.empty_hint')} action={{ label: t('browse.clear_query'), onclick: () => ((query = ''), void run()) }} />
        </div>
      {/snippet}
    </Table>
    {#if rows.length < total}
      <div class="more">
        <Button loading={loading} onclick={() => void run(true)}>{t('browse.load_more')}</Button>
        <span class="muted">{t('browse.showing', { shown: rows.length, total })}</span>
      </div>
    {/if}
  {/if}
</div>

<CardInfoSheet {info} onclose={() => (info = null)} onedit={(i) => ((editing = i), (info = null))} />

{#if editing}
  <NoteEditDialog
    noteId={editing.note.id}
    deckId={editing.card.deck_id}
    deckName={editing.card.deck_path}
    onclose={() => ((editing = null), void refresh())}
  />
{/if}

<Dialog bind:open={columnsOpen} title={t('browse.columns')} size="sm">
  <div class="cols">
    {#each COLUMNS as c (c.id)}
      <Checkbox label={td(c.label)} checked={shown.includes(c.id)} disabled={c.fixed} onchange={(on) => toggleColumn(c.id, on)} />
    {/each}
  </div>
</Dialog>

<Dialog bind:open={saveOpen} title={t('browse.save')} size="sm">
  <form
    id="save-query"
    class="cols"
    onsubmit={(e) => {
      e.preventDefault();
      void saveQuery();
    }}
  >
    <Input label={t('browse.save_name')} bind:value={saveName} />
    <code class="code">{ran || query}</code>
  </form>
  {#snippet footer()}
    <Button variant="ghost" onclick={() => (saveOpen = false)}>{t('common.cancel')}</Button>
    <Button variant="primary" type="submit" form="save-query" disabled={!saveName.trim()}>{t('browse.save')}</Button>
  {/snippet}
</Dialog>

<Dialog bind:open={confirmDelete} role="alertdialog" title={t('browse.delete_title', { count: noteSelection.length })} description={t('note.undo_hint')} size="sm">
  {#snippet footer()}
    <Button variant="ghost" onclick={() => (confirmDelete = false)}>{t('common.cancel')}</Button>
    <Button variant="danger" onclick={() => void deleteNotes()}>{t('common.delete')}</Button>
  {/snippet}
</Dialog>

<style>
  .browse {
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    min-width: 0;
  }
  .head {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  .head :global(.page-head) {
    margin-bottom: 0;
  }
  .query {
    display: flex;
    gap: var(--space-2);
    align-items: flex-start;
  }
  .query :global(.q) {
    flex: 1;
    min-width: 0;
  }
  .query :global(button.saved-trigger) {
    height: var(--control-h-lg);
    padding: 0 var(--space-4);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-control);
    background: var(--surface-1);
    color: var(--text-1);
  }
  .saved {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    font-weight: var(--weight-medium);
  }
  .toolbar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: var(--space-3);
    flex-wrap: wrap;
  }
  .left,
  .right {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .left {
    gap: var(--space-3);
    padding-inline-start: var(--space-4);
  }
  .right :global(.sort) {
    width: 150px;
  }
  .summary {
    font-weight: var(--weight-semibold);
    font-size: var(--text-sm);
  }
  .skeleton {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }
  .check {
    display: inline-flex;
  }
  .name {
    font-weight: var(--weight-medium);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ellipsis {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    display: block;
  }
  .muted {
    color: var(--text-2);
  }
  .num {
    font-variant-numeric: tabular-nums;
    color: var(--text-2);
  }
  .no-match {
    padding: var(--space-8) 0;
  }
  .more {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    justify-content: center;
  }
  .more .muted {
    font-size: var(--text-sm);
  }
  .cols {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  .code {
    font-family: var(--font-mono);
    font-size: var(--text-sm);
    color: var(--text-2);
    background: var(--surface-2);
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-control);
    overflow-wrap: anywhere;
  }
  @media (max-width: 1000px) {
    .query {
      flex-wrap: wrap;
    }
  }
</style>
