<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { GoalItem, GoalKind, GoalList } from '$lib/api/types';
  import { t } from '$lib/i18n/index.svelte';
  import type { ScreenCtx } from '../types';
  import { num, tzOffset } from './format';

  let { ctx }: { ctx: ScreenCtx } = $props();

  const KINDS: GoalKind[] = ['cards_learned', 'study_minutes', 'streak', 'deck_completion'];

  let goals = $state<GoalList | null>(null);
  let kind = $state<GoalKind>('cards_learned');
  let target = $state(100);
  let deadline = $state('');
  let query = $state('');

  async function load() {
    await ctx.guarded(async () => {
      goals = await api.call('goals.list', { tz_offset_mins: tzOffset() });
    });
  }

  async function add() {
    const ok = await ctx.guarded(() =>
      api.call('goals.create', { kind, target: Number(target), deadline: deadline || null, query: query.trim() || null }),
    );
    if (ok) {
      deadline = '';
      query = '';
    }
    await load();
  }

  function forecast(g: GoalItem): string {
    const p = g.progress;
    if (p.done) return t('goals.done');
    if (p.days_late !== null) {
      if (p.days_late > 0) return t('goals.late', { n: p.days_late });
      if (p.days_late < 0) return t('goals.early', { n: -p.days_late });
      return t('goals.on_track', { date: p.projected_date ?? '' });
    }
    if (p.projected_date) return t('goals.eta', { date: p.projected_date });
    return p.daily ? '' : t('goals.no_pace');
  }

  onMount(() => {
    void load();
    const unsub = api.on(['goals_changed', 'study_', 'undone', 'redone', 'collection_replaced', 'data_changed'], () => void load());
    return () => void unsub.then((u) => u());
  });
</script>

<h2>{t('goals.heading')}</h2>

{#if goals && goals.items.length === 0}<p class="muted">{t('goals.none')}</p>{/if}
<ul>
  {#each goals?.items ?? [] as g (g.goal.id)}
    <li>
      <div class="head">
        <strong>{t(`goals.kind.${g.goal.kind}`)}</strong>
        <span>{t('goals.progress', { current: num(g.progress.current), target: num(g.goal.target) })}
          {#if g.progress.daily}({t('goals.daily')}){/if}</span>
        {#if g.goal.deadline}<span class="muted">{t('goals.deadline')}: {g.goal.deadline}</span>{/if}
        {#if g.goal.query}<code>{g.goal.query}</code>{/if}
        <button onclick={() => ctx.guarded(() => api.call('goals.delete', { id: g.goal.id }))}>{t('goals.delete')}</button>
      </div>
      <progress max="100" value={g.progress.percent} aria-label={t(`goals.kind.${g.goal.kind}`)}></progress>
      <p class="muted" role="status">{forecast(g)}</p>
    </li>
  {/each}
</ul>

<form class="row" onsubmit={(e) => { e.preventDefault(); void add(); }}>
  <label>{t('goals.kind.label')}
    <select bind:value={kind}>{#each KINDS as k (k)}<option value={k}>{t(`goals.kind.${k}`)}</option>{/each}</select></label>
  <label>{t('goals.target')} <input type="number" min="1" bind:value={target} /></label>
  <label>{t('goals.deadline')} <input type="date" bind:value={deadline} /></label>
  <label>{t('goals.query')} <input bind:value={query} /></label>
  <button type="submit">{t('goals.add')}</button>
</form>

<style>
  ul {
    list-style: none;
    padding: 0;
  }
  li {
    margin: var(--fc-space-4) 0;
  }
  .head,
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: var(--fc-space-3);
    align-items: center;
  }
  .row {
    align-items: end;
  }
  progress {
    width: 100%;
  }
</style>
