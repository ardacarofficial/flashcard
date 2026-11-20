<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { DatabaseReport, DuplicateGroup, TrashList } from '$lib/api/types';
  import { i18n, t } from '$lib/i18n/index.svelte';
  import type { ScreenCtx } from '../types';

  let { ctx }: { ctx: ScreenCtx } = $props();

  let trash = $state<TrashList | null>(null);
  let dups = $state<DuplicateGroup[] | null>(null);
  let report = $state<DatabaseReport | null>(null);
  let repaired = $state<string | null>(null);
  let purged = $state<string | null>(null);
  let retention = $state<number | null>(null);

  const PROBLEMS = [
    'orphan_cards',
    'cards_in_trashed_decks',
    'unscheduled_cards',
    'missing_search_index',
    'cardless_notes',
    'broken_fields',
  ] as const;

  const fmt = (ms: number) => new Date(ms).toLocaleString(i18n.locale);

  async function loadTrash() {
    await ctx.guarded(async () => (trash = await api.call('browse.trash', { limit: null })));
  }

  async function findDups() {
    await ctx.guarded(async () => (dups = (await api.call('browse.duplicates', {})).groups));
  }

  async function check() {
    repaired = null;
    await ctx.guarded(async () => (report = await api.call('browse.check_database', {})));
  }

  async function repair() {
    const ok = await ctx.guarded(async () => {
      const r = await api.call('browse.repair_database', {});
      repaired = t('maintenance.repaired', {
        trashed: r.trashed_cards,
        rescheduled: r.rescheduled_cards,
        reindexed: r.reindexed_notes,
        fixed: r.fixed_notes,
        removed: r.removed_values,
      });
    });
    if (ok) report = await api.call('browse.check_database', {});
  }

  async function emptyTrash() {
    await ctx.guarded(async () => {
      const r = await api.call('browse.purge_trash', { older_than_days: 0 });
      purged = t('maintenance.purged', { count: r.notes + r.cards + r.decks });
    });
    await loadTrash();
  }

  // Only a hint: without the setting (e.g. the dev mock) the line is simply not shown.
  async function loadRetention() {
    try {
      const s = await api.call('settings.get', { scope: { kind: 'app' }, keys: ['browse.trash_retention_days'] });
      const v = s.values[0]?.value;
      retention = typeof v === 'number' ? v : null;
    } catch {
      retention = null;
    }
  }

  onMount(() => {
    void loadTrash();
    void loadRetention();
    const unsub = api.on(['notes_', 'deck_', 'undone', 'redone'], () => void loadTrash());
    return () => void unsub.then((u) => u());
  });
</script>

<h2>{t('maintenance.heading')}</h2>

<section>
  <h3>{t('maintenance.trash')}</h3>
  {#if retention !== null}<p class="muted">{t('maintenance.purge_hint', { days: retention })}</p>{/if}
  {#if purged}<p role="status">{purged}</p>{/if}
  {#if trash && trash.decks.length === 0 && trash.notes.length === 0}
    <p class="muted">{t('maintenance.trash_empty')}</p>
  {:else if trash}
    {#if trash.decks.length > 0}
      <h4>{t('maintenance.trashed_decks')}</h4>
      <ul>
        {#each trash.decks as d (d.id)}
          <li>{d.path} <span class="muted">{fmt(d.deleted_at)}</span>
            <button onclick={() => ctx.guarded(() => api.call('decks.restore', { id: d.id }))}>{t('maintenance.restore')}</button></li>
        {/each}
      </ul>
    {/if}
    {#if trash.notes.length > 0}
      <h4>{t('maintenance.trashed_notes')}</h4>
      <ul>
        {#each trash.notes as n (n.id)}
          <li>{n.sort_field || t('common.none')} <span class="muted">{fmt(n.deleted_at)}</span>
            <button onclick={() => ctx.guarded(() => api.call('notes.restore', { ids: [n.id] }))}>{t('maintenance.restore')}</button></li>
        {/each}
      </ul>
    {/if}
    <button onclick={() => void emptyTrash()}>{t('maintenance.purge')}</button>
  {/if}
</section>

<section>
  <h3>{t('maintenance.duplicates')}</h3>
  <button onclick={() => void findDups()}>{t('maintenance.find_duplicates')}</button>
  {#if dups && dups.length === 0}
    <p class="muted">{t('maintenance.no_duplicates')}</p>
  {:else if dups}
    <ul>
      {#each dups as g (g.note_ids[0])}
        <li>{g.sort_field} × {g.note_ids.length}</li>
      {/each}
    </ul>
  {/if}
</section>

<section>
  <h3>{t('maintenance.check')}</h3>
  <button onclick={() => void check()}>{t('maintenance.run_check')}</button>
  {#if repaired}<p role="status">{repaired}</p>{/if}
  {#if report}
    {#if report.healthy && report.cardless_notes.length === 0}
      <p class="muted">{t('maintenance.healthy')}</p>
    {:else}
      <ul>
        {#each PROBLEMS as p (p)}
          {#if report[p].length > 0}<li>{t(`maintenance.problem.${p}`, { count: report[p].length })}</li>{/if}
        {/each}
        {#if report.dangling_values > 0}
          <li>{t('maintenance.problem.dangling_values', { count: report.dangling_values })}</li>
        {/if}
      </ul>
      {#if !report.healthy}
        <button onclick={() => void repair()}>{t('maintenance.repair')}</button>
      {/if}
    {/if}
  {/if}
</section>

<style>
  section {
    margin-bottom: var(--fc-space-6);
  }
  ul {
    padding-left: var(--fc-space-4);
  }
  li {
    margin: var(--fc-space-1) 0;
  }
</style>
