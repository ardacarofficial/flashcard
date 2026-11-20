<script lang="ts">
  import { formatDateTime } from '$lib/format';
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { DatabaseReport, DuplicateGroup, TrashList } from '$lib/api/types';
  import { i18n, t } from '$lib/i18n/index.svelte';
  import type { ScreenCtx } from '../types';
  import { Button, CircleCheck, Copy, Database, Icon, RotateCcw, Search, Trash2, TriangleAlert, Wrench } from '$lib/ui';
  import { FormRow, LoadState, SettingsPage, SettingsSection } from '../core/kit';

  let { ctx }: { ctx: ScreenCtx } = $props();

  let trash = $state<TrashList | null>(null);
  let dups = $state<DuplicateGroup[] | null>(null);
  let report = $state<DatabaseReport | null>(null);
  let repaired = $state<string | null>(null);
  let purged = $state<string | null>(null);
  let retention = $state<number | null>(null);
  let trashFailed = $state(false);
  let finding = $state(false);
  let checking = $state(false);
  /** Trashed decks and notes as two lists of restorable rows. */
  const trashGroups = $derived([
    {
      key: 'trashed_decks' as const,
      items: (trash?.decks ?? []).map((d) => ({ id: d.id, label: d.path, at: d.deleted_at, restore: () => ctx.guarded(() => api.call('decks.restore', { id: d.id })) })),
    },
    {
      key: 'trashed_notes' as const,
      items: (trash?.notes ?? []).map((n) => ({ id: n.id, label: n.sort_field || t('common.none'), at: n.deleted_at, restore: () => ctx.guarded(() => api.call('notes.restore', { ids: [n.id] })) })),
    },
  ]);
  const trashEmpty = $derived(!!trash && trash.decks.length === 0 && trash.notes.length === 0);

  const PROBLEMS = [
    'orphan_cards',
    'cards_in_trashed_decks',
    'unscheduled_cards',
    'missing_search_index',
    'cardless_notes',
    'broken_fields',
  ] as const;

  const fmt = (ms: number) => formatDateTime(ms);

  async function loadTrash() {
    trashFailed = !(await ctx.guarded(async () => (trash = await api.call('browse.trash', { limit: null }))));
  }

  async function findDups() {
    finding = true;
    await ctx.guarded(async () => (dups = (await api.call('browse.duplicates', {})).groups));
    finding = false;
  }

  async function check() {
    repaired = null;
    checking = true;
    await ctx.guarded(async () => (report = await api.call('browse.check_database', {})));
    checking = false;
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
    const unsub = api.on(['notes_', 'deck_', 'undone', 'redone', 'data_changed'], () => void loadTrash());
    return () => void unsub.then((u) => u());
  });
</script>

<SettingsPage title={t('maintenance.heading')} subtitle={t('maintenance.subtitle')}>
  <SettingsSection title={t('maintenance.trash')} icon={Trash2}
    description={retention !== null ? t('maintenance.purge_hint', { days: retention }) : t('maintenance.trash_desc')}>
    {#snippet actions()}
      {#if trash && !trashEmpty}
        <Button size="sm" variant="danger" icon={Trash2} onclick={() => void emptyTrash()}>{t('maintenance.purge')}</Button>
      {/if}
    {/snippet}
    {#if purged}<p class="ok" role="status"><Icon icon={CircleCheck} size={16} />{purged}</p>{/if}
    {#if !trash}
      <LoadState state={trashFailed ? 'error' : 'loading'} retry={() => void loadTrash()} />
    {:else if trashEmpty}
      <p class="muted">{t('maintenance.trash_empty')}</p>
    {:else}
      {#each trashGroups as group (group.key)}
        {#if group.items.length > 0}
          <h3 class="sub">{t(`maintenance.${group.key}`)}</h3>
          <ul class="list">
            {#each group.items as item (item.id)}
              <li>
                <span class="label">{item.label}</span>
                <span class="when">{fmt(item.at)}</span>
                <Button size="sm" variant="ghost" icon={RotateCcw} onclick={() => void item.restore()}>{t('maintenance.restore')}</Button>
              </li>
            {/each}
          </ul>
        {/if}
      {/each}
    {/if}
  </SettingsSection>

  <SettingsSection title={t('maintenance.duplicates')} icon={Copy} description={t('maintenance.duplicates_intro')} body="rows">
    <FormRow label={t('maintenance.duplicates_desc')}>
      <Button icon={Search} loading={finding} onclick={() => void findDups()}>{t('maintenance.find_duplicates')}</Button>
    </FormRow>
    {#if dups}
      <FormRow label={t('maintenance.results')} stack>
        {#if dups.length === 0}
          <p class="muted">{t('maintenance.no_duplicates')}</p>
        {:else}
          <ul class="list">
            {#each dups as g (g.note_ids[0])}
              <li><span class="label">{g.sort_field}</span><span class="when">× {g.note_ids.length}</span></li>
            {/each}
          </ul>
        {/if}
      </FormRow>
    {/if}
  </SettingsSection>

  <SettingsSection title={t('maintenance.check')} icon={Database} description={t('maintenance.check_desc')} body="rows">
    <FormRow label={t('maintenance.check_row')}>
      <Button icon={Wrench} loading={checking} onclick={() => void check()}>{t('maintenance.run_check')}</Button>
    </FormRow>
    {#if repaired || report}
      <FormRow label={t('maintenance.results')} stack>
        {#if repaired}<p class="ok" role="status"><Icon icon={CircleCheck} size={16} />{repaired}</p>{/if}
        {#if report}
          {#if report.healthy && report.cardless_notes.length === 0}
            <p class="ok"><Icon icon={CircleCheck} size={16} />{t('maintenance.healthy')}</p>
          {:else}
            <ul class="problems">
              {#each PROBLEMS as p (p)}
                {#if report[p].length > 0}<li><Icon icon={TriangleAlert} size={16} />{t(`maintenance.problem.${p}`, { count: report[p].length })}</li>{/if}
              {/each}
              {#if report.dangling_values > 0}
                <li><Icon icon={TriangleAlert} size={16} />{t('maintenance.problem.dangling_values', { count: report.dangling_values })}</li>
              {/if}
            </ul>
            {#if !report.healthy}
              <div><Button variant="primary" icon={Wrench} onclick={() => void repair()}>{t('maintenance.repair')}</Button></div>
            {/if}
          {/if}
        {/if}
      </FormRow>
    {/if}
  </SettingsSection>
</SettingsPage>

<style>
  .muted {
    margin: 0;
    color: var(--text-2);
    font-size: var(--text-sm);
  }
  .sub {
    margin: var(--space-3) 0 var(--space-1);
    font-size: var(--text-xs);
    font-weight: var(--weight-semibold);
    color: var(--text-2);
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .sub:first-of-type {
    margin-top: 0;
  }
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    width: 100%;
  }
  .list li {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    min-height: 40px;
    padding: var(--space-1) 0;
    border-bottom: 1px solid var(--border);
    font-size: var(--text-sm);
  }
  .list li:last-child {
    border-bottom: none;
  }
  .label {
    flex: 1 1 auto;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .when {
    color: var(--text-2);
    white-space: nowrap;
  }
  .ok,
  .problems li {
    display: flex;
    align-items: flex-start;
    gap: var(--space-2);
    margin: 0;
    font-size: var(--text-sm);
  }
  .ok {
    color: var(--success);
    margin-bottom: var(--space-2);
  }
  .ok :global(svg),
  .problems :global(svg) {
    flex: none;
    margin-top: 2px;
  }
  .problems {
    list-style: none;
    margin: 0 0 var(--space-3);
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .problems li {
    color: var(--warning);
  }
</style>
