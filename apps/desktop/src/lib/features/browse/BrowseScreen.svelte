<!--
  Search (v1.2, docs/11 §2–3, design brief flow 3 "find and fix a card"): the query field is
  the page (focused on arrival and with `/`), then one slim toolbar, a dense virtual table
  (34 px rows, mono numbers, column chooser) and the card info as a side panel that leaves the
  table usable. Keyboard: ↑/↓ move, Space selects, Shift+↑/↓ or Shift+click extend the
  selection, Enter edits the note, Esc closes the panel / clears the selection. The query
  language, sorting and every change run in Rust (`search.*`, `bulk.*`).
-->
<script lang="ts">
  import { formatDate } from '$lib/format';
  import { onMount, tick, untrack } from 'svelte';
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
  import { t, td } from '$lib/i18n/index.svelte';
  import { deckPaths } from '$lib/components/deck-paths';
  import { openQuickAdd } from '$lib/shell/commands.svelte';
  import {
    ArrowDown,
    ArrowUp,
    Button,
    Checkbox,
    CircleHelp,
    Columns3,
    Dialog,
    EmptyState,
    Icon,
    IconButton,
    Input,
    Kbd,
    Menu,
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
  import FilterChips from './FilterChips.svelte';
  import { COLUMNS, loadColumns, saveColumns, stateLabel, stateTone, type ColumnId } from './columns';
  import { typingTarget } from './palette';
  import QueryHelp from './QueryHelp.svelte';

  let { ctx }: { ctx: ScreenCtx } = $props();

  const PAGE = 100;
  const ROW = 34;

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
  let editing = $state<{ noteId: string; deckId: string; deckName: string } | null>(null);
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
  let queryInput = $state<HTMLInputElement | null>(null);
  let results = $state<HTMLDivElement | null>(null);
  /** Row the last plain selection started from (Shift extends from here). */
  let anchor = $state<string | null>(null);
  let shiftClick = false;

  const paths = $derived(deckPaths(ctx.decks));
  const cards = $derived([...selected.keys()]);
  const notes = $derived([...new Set(selected.values())]);
  /** Notes of a "select all" (cards outside the loaded page have no row to map from). */
  let allNotes = $state<string[] | null>(null);
  const noteSelection = $derived(allNotes ?? notes.filter(Boolean));
  const selectedKeys = $derived(new Set(selected.keys()));
  const allChecked = $derived(total > 0 && selected.size >= total);
  const indexOf = (key: string | null) => (key === null ? -1 : rows.findIndex((r) => r.card_id === key));

  const fmtDate = (ms: number | null) => (ms === null ? t('common.none') : formatDate(ms));
  const NUMERIC = new Set<ColumnId>(['due', 'lapses', 'reps', 'created']);

  const tableColumns = $derived<TableColumn<CardRow>[]>([
    { id: 'select', header: '', width: '20px' },
    ...COLUMNS.filter((c) => shown.includes(c.id)).map((c) => ({
      id: c.id,
      header: td(c.label) + (c.sort && c.sort === sort ? (descending ? ' ↓' : ' ↑') : ''),
      width: c.width,
      numeric: NUMERIC.has(c.id),
      align: c.id === 'lapses' || c.id === 'reps' ? ('end' as const) : ('start' as const),
    })),
  ]);

  const sortOptions = $derived(
    COLUMNS.filter((c) => c.sort).map((c) => ({ value: c.sort as CardSortKey, label: td(c.label) })),
  );

  /** The table fills the window below the toolbar (fewer rows: just their height). */
  const tableHeight = $derived(`min(${(rows.length + 1) * ROW + 2}px, calc(100vh - ${selected.size > 0 ? 268 : 224}px))`);

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
        anchor = null;
        if (activeKey !== null && indexOf(activeKey) < 0) activeKey = null;
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

  /** Toggles one row; with Shift (click or keys) the range from the anchor follows its state. */
  function toggle(row: CardRow, range = false) {
    allNotes = null;
    const next = new Map(selected);
    const from = indexOf(anchor);
    const to = indexOf(row.card_id);
    if (range && from >= 0 && to >= 0) {
      const on = !next.has(row.card_id) || from === to;
      const [a, b] = from < to ? [from, to] : [to, from];
      for (const r of rows.slice(a, b + 1)) {
        if (on) next.set(r.card_id, r.note_id);
        else next.delete(r.card_id);
      }
    } else {
      if (next.has(row.card_id)) next.delete(row.card_id);
      else next.set(row.card_id, row.note_id);
      anchor = row.card_id;
    }
    selected = next;
  }

  function clearSelection() {
    selected = new Map();
    allNotes = null;
    anchor = null;
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

  let infoSeq = 0;
  async function showInfo(cardId: string) {
    const mine = ++infoSeq;
    await ctx.guarded(async () => {
      const r = await api.call('browse.card_info', { card_id: cardId });
      if (mine === infoSeq) info = r;
    });
  }

  function closeInfo() {
    infoSeq++;
    info = null;
  }

  function edit(row: CardRow | null | undefined) {
    if (row) editing = { noteId: row.note_id, deckId: row.deck_id, deckName: row.deck_path };
  }

  // The open panel follows the active row while moving with the arrow keys.
  $effect(() => {
    const key = activeKey;
    if (!key || untrack(() => info === null || info.card.card_id === key)) return;
    const h = setTimeout(() => void showInfo(key), 120);
    return () => clearTimeout(h);
  });

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

  /** A filter chip rewrote the query: show it in the field and run it. */
  function setFromChip(q: string) {
    query = q;
    void check();
    void run();
  }

  /** A text chip wrote `key:` — the caret goes after it so the value is typed right there. */
  async function typeFromChip(q: string, caret: number) {
    query = q;
    await tick();
    queryInput?.focus();
    queryInput?.setSelectionRange(caret, caret);
  }

  function clearQuery() {
    query = '';
    void run();
    queryInput?.focus();
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

  function focusGrid() {
    const grid = results?.querySelector<HTMLElement>('[role=grid]');
    if (!grid || rows.length === 0) return;
    if (indexOf(activeKey) < 0) activeKey = rows[0]!.card_id;
    grid.focus();
  }

  function onQueryKey(e: KeyboardEvent) {
    if (e.key === 'ArrowDown' && rows.length > 0) {
      e.preventDefault();
      focusGrid();
    }
  }

  /** Keys on the grid before the table handles them: Enter edits, Space/Shift select. */
  function onGridKey(e: KeyboardEvent) {
    const target = e.target as HTMLElement | null;
    if (target?.getAttribute('role') !== 'grid' || e.ctrlKey || e.metaKey || e.altKey) return;
    const at = indexOf(activeKey);
    const row = at >= 0 ? rows[at] : undefined;
    if ((e.key === 'Enter' || e.key === ' ') && row) {
      e.preventDefault();
      e.stopPropagation();
      // A held key is one edit / one toggle (v1.3); only the arrows repeat.
      if (e.repeat) return;
      if (e.key === 'Enter') edit(row);
      else toggle(row, e.shiftKey);
    } else if (e.shiftKey && (e.key === 'ArrowDown' || e.key === 'ArrowUp') && row) {
      // Extend: the current and the next row join the selection; the table then moves.
      const next = rows[at + (e.key === 'ArrowDown' ? 1 : -1)];
      if (!next) return;
      const m = new Map(selected);
      m.set(row.card_id, row.note_id);
      m.set(next.card_id, next.note_id);
      if (anchor === null) anchor = row.card_id;
      allNotes = null;
      selected = m;
    }
  }

  function onWindowKey(e: KeyboardEvent) {
    if (e.defaultPrevented || e.repeat || e.ctrlKey || e.metaKey || e.altKey) return;
    if (document.querySelector('[aria-modal="true"]')) return;
    if (e.key === '/' && !typingTarget(e.target)) {
      e.preventDefault();
      queryInput?.focus();
      queryInput?.select();
    } else if (e.key === 'Escape') {
      if (info) {
        e.preventDefault();
        closeInfo();
      } else if (selected.size > 0 && !typingTarget(e.target)) {
        e.preventDefault();
        clearSelection();
      }
    }
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
    queryInput?.focus();
    return () => {
      void unsub.then((u) => u());
      window.removeEventListener('fc:shortcut', onShortcut);
    };
  });
</script>

<svelte:window onkeydown={onWindowKey} />

<div class="browse">
  <h1 class="sr-only">{t('browse.heading')}</h1>
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
      bind:element={queryInput}
      oninput={onInput}
      onkeydown={onQueryKey}
      placeholder={t('browse.query_placeholder')}
      error={queryError ? errorText(queryError) : null}
      spellcheck="false"
      autocomplete="off"
      aria-keyshortcuts="/"
    />
    <IconButton icon={CircleHelp} variant="secondary" size="lg" label={t('browse.syntax.open')} pressed={helpOpen} onclick={() => (helpOpen = !helpOpen)} />
    <Menu items={savedItems} label={t('browse.saved')} triggerClass="saved-trigger">
      {#snippet trigger()}<span class="saved"><Icon icon={Star} size={16} /><span class="saved-label">{t('browse.saved_short')}</span></span>{/snippet}
    </Menu>
    <Button type="submit" variant="primary" size="lg" kbd="Enter">{t('browse.run')}</Button>
  </form>
  <FilterChips {query} onset={setFromChip} ontext={(q, caret) => void typeFromChip(q, caret)} />
  {#if helpOpen}
    <QueryHelp onpick={(q) => ((query = q), void run())} onclose={() => ((helpOpen = false), queryInput?.focus())} />
  {/if}

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
      <span class="summary" aria-live="polite" data-testid="browse-summary">
        {notesOnly ? t('browse.results_notes', { count: total }) : t('browse.results', { count: total })}
      </span>
      {#if loading && loaded}<span class="busy" aria-hidden="true"></span>{/if}
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
      <span class="sort-group">
        <Select size="sm" label={t('browse.sort')} hideLabel options={sortOptions} value={sort} onchange={setSort} class="sort" />
        <IconButton
          size="sm"
          variant="secondary"
          icon={descending ? ArrowDown : ArrowUp}
          label={descending ? t('browse.sort_desc') : t('browse.sort_asc')}
          onclick={() => ((descending = !descending), void run())}
        />
      </span>
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
    <EmptyState size="sm" title={t('browse.load_failed')} description={loadError} action={{ label: t('create.retry'), onclick: () => void run() }} />
  {:else if !loaded}
    <div class="skeleton" aria-busy="true" aria-label={t('create.loading')}>
      {#each [0, 1, 2, 3, 4, 5, 6, 7] as i (i)}<Skeleton shape="text" height="{ROW - 6}px" />{/each}
    </div>
  {:else if total === 0 && !ran.trim()}
    <div class="empty-line">
      <span>{t('browse.empty_collection')}</span>
      <Button size="sm" variant="primary" onclick={() => openQuickAdd() || ctx.navigate?.('/notes/new')}>{t('browse.create_cards')}</Button>
    </div>
  {:else if rows.length === 0}
    <div class="empty-line">
      <span>{t('browse.empty')}</span>
      <Button size="sm" onclick={clearQuery}>{t('browse.clear_query')}</Button>
    </div>
  {:else}
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="results" class:with-info={info !== null} bind:this={results} onkeydowncapture={onGridKey}>
      <Table
        {rows}
        columns={tableColumns}
        rowKey={(r) => r.card_id}
        label={t('browse.table')}
        rowHeight={ROW}
        height={tableHeight}
        class="grid"
        {selectedKeys}
        bind:activeKey
        onrowclick={(r) => void showInfo(r.card_id)}
      >
        {#snippet cell(r, c)}
          {#if c.id === 'select'}
            <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
            <span
              class="check"
              data-testid="browse-select"
              onclickcapture={(e) => (shiftClick = e.shiftKey)}
              onclick={(e) => e.stopPropagation()}
            >
              <Checkbox
                checked={selected.has(r.card_id)}
                ariaLabel={t('browse.select_row', { name: r.sort_field })}
                onchange={() => toggle(r, shiftClick)}
              />
            </span>
          {:else if c.id === 'sort_field'}
            <span class="name" data-testid="browse-row">{r.sort_field || t('common.none')}</span>
          {:else if c.id === 'deck'}
            <span class="muted ellipsis">{r.deck_path}</span>
          {:else if c.id === 'state'}
            <span class="state tone-{stateTone(r)}"><span class="dot" aria-hidden="true"></span>{stateLabel(r)}</span>
          {:else if c.id === 'due'}
            <span class="dim">{fmtDate(r.due)}</span>
          {:else if c.id === 'lapses'}
            <span class:dim={r.lapses === 0}>{r.lapses}</span>
          {:else if c.id === 'reps'}
            <span class:dim={r.reps === 0}>{r.reps}</span>
          {:else if c.id === 'tags'}
            <span class="muted ellipsis">{r.tags.map((x) => `#${x}`).join(' ')}</span>
          {:else if c.id === 'created'}
            <span class="dim">{fmtDate(r.created_at)}</span>
          {/if}
        {/snippet}
      </Table>
      {#if info}
        <CardInfoSheet {info} onclose={closeInfo} onedit={(i) => edit(i.card)} />
      {/if}
    </div>
    <div class="foot">
      {#if rows.length < total}
        <span class="muted">{t('browse.showing', { shown: rows.length, total })}</span>
        <Button size="sm" variant="ghost" loading={loading} onclick={() => void run(true)}>{t('browse.load_more')}</Button>
      {/if}
      {#if rows.length > 0}
        <span class="keys" aria-hidden="true">
          <span><Kbd keys={['ArrowUp']} size="sm" /><Kbd keys={['ArrowDown']} size="sm" /> {t('browse.keys.move')}</span>
          <span><Kbd keys={['Space']} size="sm" /> {t('browse.keys.select')}</span>
          <span><Kbd keys={['Enter']} size="sm" /> {t('browse.keys.edit')}</span>
          <span><Kbd keys={['/']} size="sm" /> {t('browse.keys.search')}</span>
        </span>
      {/if}
    </div>
  {/if}
</div>

{#if editing}
  <NoteEditDialog noteId={editing.noteId} deckId={editing.deckId} deckName={editing.deckName} onclose={() => ((editing = null), void refresh())} />
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
    gap: var(--space-3);
    min-width: 0;
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
    padding: 0 var(--space-3);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-control);
    background: var(--surface-2);
    color: var(--text-1);
  }
  .query :global(button.saved-trigger:hover) {
    background: var(--surface-3);
  }
  .saved {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
  }
  .toolbar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: var(--space-2) var(--space-3);
    flex-wrap: wrap;
    min-height: var(--control-h-sm);
  }
  .left,
  .right,
  .sort-group {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .left {
    padding-inline-start: var(--space-3);
  }
  .sort-group {
    gap: var(--space-1);
  }
  .right :global(.sort) {
    width: 140px;
  }
  .summary {
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
    color: var(--text-2);
    font-variant-numeric: tabular-nums;
  }
  .busy {
    width: 12px;
    height: 12px;
    border: 2px solid var(--border-strong);
    border-top-color: var(--accent-text);
    border-radius: 50%;
    animation: fc-spin 0.8s linear infinite;
  }
  @keyframes fc-spin {
    to {
      transform: rotate(360deg);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .busy {
      animation: none;
    }
  }
  .skeleton {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
  }
  .results {
    position: relative;
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: var(--space-3);
    align-items: start;
  }
  .results.with-info {
    grid-template-columns: minmax(0, 1fr) 320px;
  }
  /* Narrow: the panel floats over the table's right part instead of squeezing it. */
  @media (max-width: 1180px) {
    .results.with-info {
      grid-template-columns: minmax(0, 1fr);
    }
    .results.with-info :global(.info-panel) {
      position: absolute;
      top: 0;
      right: 0;
      bottom: 0;
      width: min(320px, 100%);
      z-index: 5;
      box-shadow: var(--shadow-float);
    }
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
  .dim {
    color: var(--text-2);
  }
  .empty-line {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-height: 48px;
    padding: 0 var(--space-4);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--surface-1);
    color: var(--text-2);
    font-size: var(--text-sm);
  }
  .state {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    color: var(--text-2);
  }
  .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--text-3);
    flex: none;
  }
  .tone-accent .dot {
    background: var(--accent);
  }
  .tone-success .dot {
    background: var(--success);
  }
  .tone-warning .dot {
    background: var(--warning);
  }
  .tone-danger .dot {
    background: var(--danger);
  }
  .foot {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-height: var(--control-h-sm);
    font-size: var(--text-xs);
  }
  .keys {
    display: inline-flex;
    gap: var(--space-4);
    margin-inline-start: auto;
    color: var(--text-2);
  }
  .keys span {
    display: inline-flex;
    align-items: center;
    gap: 4px;
  }
  .cols {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  .code {
    font-family: var(--font-mono);
    font-variant-ligatures: none;
    font-size: var(--text-sm);
    color: var(--text-2);
    background: var(--surface-2);
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-control);
    overflow-wrap: anywhere;
  }
  @media (max-width: 1000px) {
    .saved-label {
      display: none;
    }
    .keys {
      display: none;
    }
  }
</style>
