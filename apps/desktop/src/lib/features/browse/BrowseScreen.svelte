<script lang="ts">
  import { onMount } from 'svelte';
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
  import ConfirmDialog from '$lib/components/ConfirmDialog.svelte';
  import { deckPaths } from '$lib/components/deck-paths';
  import type { ScreenCtx } from '../types';

  let { ctx }: { ctx: ScreenCtx } = $props();

  const PAGE = 100;
  const COLUMNS: { key: CardSortKey | null; label: string }[] = [
    { key: 'sort_field', label: 'browse.col.sort_field' },
    { key: null, label: 'browse.col.deck' },
    { key: null, label: 'browse.col.state' },
    { key: 'due', label: 'browse.col.due' },
    { key: 'lapses', label: 'browse.col.lapses' },
    { key: null, label: 'browse.col.tags' },
    { key: 'created', label: 'browse.col.created' },
  ];

  let query = $state('');
  let ran = $state('');
  let notesOnly = $state(false);
  let sort = $state<CardSortKey>('sort_field');
  let descending = $state(false);
  let rows = $state<CardRow[]>([]);
  let total = $state(0);
  let queryError = $state<QueryErrorDto | null>(null);
  let selected = $state<Map<string, string>>(new Map()); // card → note
  let saved = $state<SavedQueryDto[]>([]);
  let saveName = $state('');
  let info = $state<CardInfo | null>(null);
  let notice = $state<string | null>(null);
  let confirmDelete = $state(false);
  let attrs = $state<AttributeDefDto[]>([]);

  // Bulk inputs.
  let moveTo = $state<string>('');
  // Anki's numbering: 1 red, 2 orange, 3 green, 4 blue, 5 pink, 6 turquoise, 7 purple. Names
  // come from v0.3's `flags.list` (`card_flag_defs`; a custom name also works as `flag:<name>`
  // in queries, `browse::flag_names`), else the localized default.
  const FLAGS = [1, 2, 3, 4, 5, 6, 7] as const;
  let flagDefs = $state<FlagDto[]>([]);
  // Change note type (default mapping: same field/template keys, then by position).
  let noteTypes = $state<NoteTypeDto[]>([]);
  let changeTo = $state('');
  async function loadNoteTypes() {
    try {
      noteTypes = (await api.call('notetypes.list', {})).notetypes;
    } catch {
      noteTypes = [];
    }
  }
  async function changeNoteType() {
    const ok = await ctx.guarded(async () => {
      const r = await api.call('bulk.change_notetype', {
        note_ids: noteSelection,
        notetype_id: changeTo,
        field_map: null,
        template_map: null,
      });
      notice = t('browse.bulk.notetype_done', {
        notes: r.notes,
        kept: r.cards_kept,
        trashed: r.cards_trashed,
        created: r.cards_created,
      });
    });
    if (ok) await refresh();
  }
  const flagLabel = (n: number): string =>
    flagDefs.find((f) => f.flag === n)?.name ?? td(`browse.flags.${n}`);
  async function loadFlags() {
    try {
      flagDefs = (await api.call('flags.list', {})).flags;
    } catch {
      flagDefs = []; // flags module off: default names
    }
  }
  let flag = $state(1);
  let tag = $state('');
  let find = $state('');
  let replace = $state('');
  let field = $state('');
  let regex = $state(false);
  let matchCase = $state(false);
  let attrId = $state('');
  let attrValue = $state('');

  const paths = $derived(deckPaths(ctx.decks));
  const enumAttrs = $derived(attrs.filter((a) => a.options.length > 0));
  const attr = $derived(enumAttrs.find((a) => a.id === attrId) ?? null);
  const cards = $derived([...selected.keys()]);
  const notes = $derived([...new Set(selected.values())]);

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
    try {
      const page = await api.call('search.cards', {
        query,
        sort,
        descending,
        notes_only: notesOnly,
        offset,
        limit: PAGE,
      });
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
      } else {
        await ctx.guarded(() => Promise.reject(e));
      }
    }
  }

  async function refresh() {
    query = ran;
    await run();
  }

  async function loadSaved() {
    await ctx.guarded(async () => (saved = (await api.call('saved_queries.list', {})).items));
  }

  function sortBy(key: CardSortKey | null) {
    if (!key) return;
    descending = sort === key ? !descending : false;
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

  /** Notes of a "select all" (cards outside the loaded page have no row to map from). */
  let allNotes = $state<string[] | null>(null);
  const noteSelection = $derived(allNotes ?? notes.filter(Boolean));

  async function selectAll() {
    await ctx.guarded(async () => {
      const ids = await api.call('search.ids', { query: ran });
      const byCard = new Map(rows.map((r) => [r.card_id, r.note_id]));
      selected = new Map(ids.card_ids.map((c) => [c, byCard.get(c) ?? '']));
      allNotes = ids.note_ids;
    });
  }

  async function bulk(fn: () => Promise<BulkResult>) {
    const ok = await ctx.guarded(async () => {
      const r = await fn();
      notice = t('browse.bulk.done', { changed: r.changed, skipped: r.skipped });
    });
    if (ok) await refresh();
  }

  async function showInfo(cardId: string) {
    await ctx.guarded(async () => (info = await api.call('browse.card_info', { card_id: cardId })));
  }

  async function saveQuery() {
    const ok = await ctx.guarded(() =>
      api.call('saved_queries.create', { name: saveName, query: ran || query, icon: null, position: null }),
    );
    if (ok) saveName = '';
  }

  async function deleteNotes() {
    confirmDelete = false;
    const ok = await ctx.guarded(() => api.call('notes.delete', { ids: noteSelection }));
    if (ok) await refresh();
  }

  function attrValuePayload() {
    if (!attr || attrValue === '') return null;
    return attr.kind === 'multi_enum'
      ? { type: 'choices' as const, value: [attrValue] }
      : { type: 'choice' as const, value: attrValue };
  }

  const fmtDate = (ms: number | null) => (ms === null ? t('common.none') : new Date(ms).toLocaleDateString(i18n.locale));

  function onShortcut(e: Event) {
    const action = (e as CustomEvent<string>).detail;
    if (action === 'browse.select_all') void selectAll();
    else if (action === 'browse.suspend' && cards.length) void bulk(() => api.call('bulk.set_suspended', { card_ids: cards, suspended: true }));
    else if (action === 'browse.info' && cards[0]) void showInfo(cards[0]);
    else if (action === 'browse.delete' && cards.length) confirmDelete = true;
  }

  onMount(() => {
    void run();
    void loadSaved();
    void loadFlags();
    void loadNoteTypes();
    void ctx.guarded(async () => (attrs = (await api.call('attributes.list', {})).defs));
    const unsub = api.on(['saved_queries', 'flags_changed', 'undone', 'redone', 'data_changed'], () => {
      void loadSaved();
      void loadFlags();
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
  <aside>
    <h3>{t('browse.saved')}</h3>
    {#if saved.length === 0}<p class="muted">{t('browse.no_saved')}</p>{/if}
    <ul>
      {#each saved as s (s.id)}
        <li>
          <button class="link" onclick={() => ((query = s.query), void run())}>{s.name}</button>
          <button class="icon" aria-label={t('browse.delete_saved', { name: s.name })} onclick={() => ctx.guarded(() => api.call('saved_queries.delete', { id: s.id }))}>×</button>
        </li>
      {/each}
    </ul>
    <form class="save" onsubmit={(e) => (e.preventDefault(), void saveQuery())}>
      <input bind:value={saveName} placeholder={t('browse.save_name')} aria-label={t('browse.save_name')} />
      <button type="submit" disabled={!saveName.trim()}>{t('browse.save')}</button>
    </form>
  </aside>

  <section>
    <form class="query" onsubmit={(e) => (e.preventDefault(), void run())}>
      <label class="grow">
        <span class="sr">{t('browse.query_label')}</span>
        <input
          bind:value={query}
          oninput={onInput}
          placeholder={t('browse.query_placeholder')}
          aria-label={t('browse.query_label')}
          aria-invalid={queryError !== null}
          spellcheck="false"
        />
      </label>
      <button type="submit">{t('browse.query_label')}</button>
      <label class="inline"><input type="checkbox" bind:checked={notesOnly} onchange={() => void run()} /> {t('browse.notes_only')}</label>
    </form>
    {#if queryError}
      <p class="error" role="alert">{errorText(queryError)}</p>
    {:else}
      <p class="muted small">{t('browse.help')}</p>
    {/if}

    <p class="summary">
      {notesOnly ? t('browse.results_notes', { count: total }) : t('browse.results', { count: total })}
      {#if selected.size > 0}
        · {t('browse.selected', { count: selected.size })}
        <button class="link" onclick={() => ((selected = new Map()), (allNotes = null))}>{t('browse.clear_selection')}</button>
      {/if}
      {#if total > 0 && selected.size < total}
        <button class="link" onclick={() => void selectAll()}>{t('browse.select_all', { count: total })}</button>
      {/if}
    </p>
    {#if notice}<p class="muted" role="status">{notice}</p>{/if}

    {#if selected.size > 0}
      <div class="bulk" role="group" aria-label={t('browse.bulk.heading')}>
        <div class="row">
          <select bind:value={moveTo} aria-label={t('browse.bulk.move')}>
            <option value="">{t('browse.bulk.move')}</option>
            {#each paths as p (p.id)}<option value={p.id}>{p.path}</option>{/each}
          </select>
          <button disabled={!moveTo} onclick={() => bulk(() => api.call('bulk.move_cards', { card_ids: cards, deck_id: moveTo }))}>{t('browse.bulk.apply')}</button>
          <button onclick={() => bulk(() => api.call('bulk.set_suspended', { card_ids: cards, suspended: true }))}>{t('browse.bulk.suspend')}</button>
          <button onclick={() => bulk(() => api.call('bulk.set_suspended', { card_ids: cards, suspended: false }))}>{t('browse.bulk.unsuspend')}</button>
          <button onclick={() => bulk(() => api.call('bulk.set_buried', { card_ids: cards, buried: true }))}>{t('browse.bulk.bury')}</button>
          <button onclick={() => bulk(() => api.call('bulk.set_buried', { card_ids: cards, buried: false }))}>{t('browse.bulk.unbury')}</button>
          <button onclick={() => bulk(() => api.call('bulk.reset_progress', { card_ids: cards }))}>{t('browse.bulk.reset')}</button>
          <button class="danger" onclick={() => (confirmDelete = true)}>{t('browse.bulk.delete')}</button>
        </div>
        <div class="row">
          <select bind:value={flag} aria-label={t('browse.bulk.flag')}>
            {#each FLAGS as f (f)}<option value={f}>{flagLabel(f)}</option>{/each}
          </select>
          <button onclick={() => bulk(() => api.call('bulk.set_flag', { card_ids: cards, flag, on: true }))}>{t('browse.bulk.flag')}</button>
          <button onclick={() => bulk(() => api.call('bulk.set_flag', { card_ids: cards, flag: 0, on: false }))}>{t('browse.bulk.flag_clear')}</button>
          <input bind:value={tag} placeholder={t('browse.bulk.tag')} aria-label={t('browse.bulk.tag')} />
          <button disabled={!tag.trim()} onclick={() => bulk(() => api.call('bulk.add_tags', { note_ids: noteSelection, tags: [tag] }))}>{t('browse.bulk.add_tag')}</button>
          <button disabled={!tag.trim()} onclick={() => bulk(() => api.call('bulk.remove_tags', { note_ids: noteSelection, tags: [tag] }))}>{t('browse.bulk.remove_tag')}</button>
        </div>
        {#if enumAttrs.length > 0}
          <div class="row">
            <select bind:value={attrId} aria-label={t('browse.bulk.classification')}>
              <option value="">{t('browse.bulk.classification')}</option>
              {#each enumAttrs as a (a.id)}<option value={a.id}>{a.name}</option>{/each}
            </select>
            {#if attr}
              <select bind:value={attrValue} aria-label={t('browse.bulk.value')}>
                <option value="">{t('browse.bulk.clear_value')}</option>
                {#each attr.options as o (o.id)}<option value={o.id}>{o.label}</option>{/each}
              </select>
              <button onclick={() => bulk(() => api.call('bulk.set_attribute', { card_ids: cards, attr_id: attrId, value: attrValuePayload() }))}>{t('browse.bulk.apply')}</button>
            {/if}
          </div>
        {/if}
        <details>
          <summary>{t('browse.bulk.find_replace')}</summary>
          <div class="row">
            <input bind:value={find} placeholder={t('browse.bulk.find')} aria-label={t('browse.bulk.find')} />
            <input bind:value={replace} placeholder={t('browse.bulk.replace')} aria-label={t('browse.bulk.replace')} />
            <input bind:value={field} placeholder={t('browse.bulk.all_fields')} aria-label={t('browse.bulk.field')} />
            <label class="inline"><input type="checkbox" bind:checked={regex} /> {t('browse.bulk.regex')}</label>
            <label class="inline"><input type="checkbox" bind:checked={matchCase} /> {t('browse.bulk.match_case')}</label>
            <button disabled={!find} onclick={() => bulk(() => api.call('bulk.find_replace', { note_ids: noteSelection, find, replace, field: field.trim() || null, regex, match_case: matchCase }))}>{t('browse.bulk.apply')}</button>
          </div>
        </details>
        <details>
          <summary>{t('browse.bulk.change_notetype')}</summary>
          <div class="row">
            <select bind:value={changeTo} aria-label={t('browse.bulk.notetype')}>
              <option value="">{t('browse.bulk.notetype')}</option>
              {#each noteTypes as nt (nt.id)}<option value={nt.id}>{nt.name}</option>{/each}
            </select>
            <button disabled={!changeTo} onclick={() => void changeNoteType()}>{t('browse.bulk.apply')}</button>
          </div>
          <p class="muted">{t('browse.bulk.notetype_help')}</p>
        </details>
      </div>
    {/if}

    {#if rows.length === 0}
      <p class="muted">{t('browse.empty')}</p>
    {:else}
      <table>
        <thead>
          <tr>
            <th><span class="sr">{t('browse.selected', { count: selected.size })}</span></th>
            {#each COLUMNS as c (c.label)}
              <th aria-sort={c.key === sort ? (descending ? 'descending' : 'ascending') : undefined}>
                {#if c.key}
                  <button class="link" onclick={() => sortBy(c.key)}>{td(c.label)}{c.key === sort ? (descending ? ' ↓' : ' ↑') : ''}</button>
                {:else}
                  {td(c.label)}
                {/if}
              </th>
            {/each}
          </tr>
        </thead>
        <tbody>
          {#each rows as r (r.card_id)}
            <tr class:selected={selected.has(r.card_id)}>
              <td><input type="checkbox" checked={selected.has(r.card_id)} onchange={() => toggle(r)} aria-label={t('browse.select_row', { name: r.sort_field })} /></td>
              <td><button class="link" onclick={() => void showInfo(r.card_id)}>{r.sort_field || t('common.none')}</button></td>
              <td>{r.deck_path}</td>
              <td>{r.suspended ? t('browse.state.suspended') : r.buried ? t('browse.state.buried') : td(`browse.state.${r.state}`)}</td>
              <td>{fmtDate(r.due)}</td>
              <td>{r.lapses}</td>
              <td>{r.tags.join(' ')}</td>
              <td>{fmtDate(r.created_at)}</td>
            </tr>
          {/each}
        </tbody>
      </table>
      {#if rows.length < total}
        <button onclick={() => void run(true)}>{t('browse.load_more')}</button>
      {/if}
    {/if}
  </section>
</div>

{#if info}
  <div class="info" role="dialog" aria-label={t('browse.info.heading')}>
    <h3>{t('browse.info.heading')}: {info.card.sort_field}</h3>
    <dl>
      <dt>{t('browse.col.deck')}</dt><dd>{info.card.deck_path}</dd>
      <dt>{t('browse.info.added')}</dt><dd>{fmtDate(info.card.created_at)}</dd>
      <dt>{t('browse.info.due')}</dt><dd>{fmtDate(info.card.due)}</dd>
      <dt>{t('browse.info.stability')}</dt><dd>{info.card.stability?.toFixed(1) ?? t('common.none')}</dd>
      <dt>{t('browse.info.difficulty')}</dt><dd>{info.card.difficulty?.toFixed(1) ?? t('common.none')}</dd>
      <dt>{t('browse.info.retrievability')}</dt><dd>{info.retrievability === null ? t('common.none') : `${Math.round(info.retrievability * 100)}%`}</dd>
    </dl>
    <h4>{t('browse.info.reviews')}</h4>
    {#if info.reviews.length === 0}
      <p class="muted">{t('browse.info.no_reviews')}</p>
    {:else}
      <ul>
        {#each info.reviews as rv, i (i)}<li>{fmtDate(rv.at)} · {rv.rating} · {rv.scheduled_days}</li>{/each}
      </ul>
    {/if}
    <button onclick={() => (info = null)}>{t('browse.info.close')}</button>
  </div>
{/if}

{#if confirmDelete}
  <ConfirmDialog
    message={t('browse.bulk.delete')}
    hint={t('note.undo_hint')}
    confirmLabel={t('common.delete')}
    onconfirm={() => void deleteNotes()}
    oncancel={() => (confirmDelete = false)}
  />
{/if}

<style>
  .browse {
    display: grid;
    grid-template-columns: 200px 1fr;
    gap: var(--fc-space-4);
  }
  aside ul {
    list-style: none;
    padding: 0;
    margin: 0 0 var(--fc-space-3);
  }
  aside li {
    display: flex;
    justify-content: space-between;
  }
  .save,
  .query,
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: var(--fc-space-2);
    align-items: center;
  }
  .save {
    flex-direction: column;
    align-items: stretch;
  }
  .grow {
    flex: 1;
  }
  .grow input {
    width: 100%;
  }
  .inline {
    display: inline-flex;
    gap: var(--fc-space-1);
    align-items: center;
  }
  .bulk {
    display: flex;
    flex-direction: column;
    gap: var(--fc-space-2);
    padding: var(--fc-space-2);
    border: var(--fc-border-width) solid var(--fc-border);
    border-radius: var(--fc-radius);
    margin-bottom: var(--fc-space-3);
  }
  table {
    width: 100%;
    border-collapse: collapse;
  }
  th,
  td {
    text-align: left;
    padding: var(--fc-space-1) var(--fc-space-2);
    border-bottom: var(--fc-border-width) solid var(--fc-border);
  }
  tr.selected {
    background: var(--fc-selected);
  }
  .link {
    border: none;
    background: transparent;
    padding: 0;
    color: inherit;
    text-align: left;
  }
  .icon {
    border: none;
    background: transparent;
  }
  .small {
    font-size: 0.85em;
  }
  .sr {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
  }
  .info {
    position: fixed;
    right: var(--fc-space-4);
    top: var(--fc-space-4);
    width: 320px;
    background: var(--fc-surface);
    border: var(--fc-border-width) solid var(--fc-border);
    border-radius: var(--fc-radius);
    padding: var(--fc-space-3);
  }
  dl {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: var(--fc-space-1) var(--fc-space-3);
  }
  dd {
    margin: 0;
  }
</style>
