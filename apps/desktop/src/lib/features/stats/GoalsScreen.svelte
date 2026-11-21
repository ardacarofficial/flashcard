<!--
  Statistics → Goals (v1.2): the goals as dense rows (kind, deadline / query, mono progress
  with a thin bar, the backend's pace forecast with an icon), and the "new goal" form as one
  compact row under them.
-->
<script lang="ts">
  import { formatPercent100 } from '$lib/format';
  import { onMount } from 'svelte';
  import type { IconNode } from 'lucide';
  import { api } from '$lib/api';
  import type { GoalItem, GoalKind, GoalList } from '$lib/api/types';
  import { t } from '$lib/i18n/index.svelte';
  import {
    Button,
    Calendar,
    Chip,
    CircleCheck,
    Clock,
    Flame,
    GraduationCap,
    Icon,
    IconButton,
    Input,
    PageHeader,
    Plus,
    ProgressBar,
    Select,
    Target,
    Trash2,
    TriangleAlert,
    TrendingUp,
    type Tone,
  } from '$lib/ui';
  import type { ScreenCtx } from '../types';
  import { LoadState } from '../core/kit';
  import Panel from './Panel.svelte';
  import { num, tzOffset } from './format';

  let { ctx }: { ctx: ScreenCtx } = $props();

  const KINDS: GoalKind[] = ['cards_learned', 'study_minutes', 'streak', 'deck_completion'];
  const KIND_ICON: Record<GoalKind, IconNode> = {
    cards_learned: GraduationCap,
    study_minutes: Clock,
    streak: Flame,
    deck_completion: Target,
  };

  let goals = $state<GoalList | null>(null);
  let failed = $state(false);
  let kind = $state<GoalKind>('cards_learned');
  let target = $state(100);
  let deadline = $state('');
  let query = $state('');

  const kindOptions = $derived(KINDS.map((k) => ({ value: k, label: t(`goals.kind.${k}`) })));

  async function load() {
    const ok = await ctx.guarded(async () => {
      goals = await api.call('goals.list', { tz_offset_mins: tzOffset() });
    });
    failed = !ok && !goals;
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

  /** The pace forecast the backend computed, as text + tone (+ icon: colour is never the only cue). */
  function forecast(g: GoalItem): { text: string; tone: Tone; icon: IconNode } | null {
    const p = g.progress;
    if (p.done) return { text: t('goals.done'), tone: 'success', icon: CircleCheck };
    if (p.days_late !== null) {
      if (p.days_late > 0) return { text: t('goals.late', { n: p.days_late }), tone: 'warning', icon: TriangleAlert };
      if (p.days_late < 0) return { text: t('goals.early', { n: -p.days_late }), tone: 'success', icon: TrendingUp };
      return { text: t('goals.on_track', { date: p.projected_date ?? '' }), tone: 'accent', icon: TrendingUp };
    }
    if (p.projected_date) return { text: t('goals.eta', { date: p.projected_date }), tone: 'neutral', icon: TrendingUp };
    return p.daily ? null : { text: t('goals.no_pace'), tone: 'neutral', icon: Clock };
  }

  onMount(() => {
    void load();
    const unsub = api.on(['goals_changed', 'study_', 'undone', 'redone', 'collection_replaced', 'data_changed'], () => void load());
    return () => void unsub.then((u) => u());
  });
</script>

<div class="goals-page">
  <PageHeader title={t('goals.heading')} subtitle={t('goals.subtitle')} />

  <Panel body="rows">
    {#if failed}
      <div class="pad"><LoadState state="error" retry={() => void load()} /></div>
    {:else if !goals}
      <div class="pad"><LoadState state="loading" /></div>
    {:else if goals.items.length === 0}
      <p class="pad empty"><Icon icon={Target} size={14} /><strong>{t('goals.none')}</strong> <span>{t('goals.none_hint')}</span></p>
    {:else}
      <ul class="goals">
        {#each goals.items as g (g.goal.id)}
          {@const f = forecast(g)}
          <li>
            <span class="ico" aria-hidden="true"><Icon icon={KIND_ICON[g.goal.kind]} size={16} /></span>
            <div class="titles">
              <h3>{t(`goals.kind.${g.goal.kind}`)}</h3>
              <p class="meta">
                {#if g.progress.daily}<Chip size="sm" label={t('goals.daily')} />{/if}
                {#if g.goal.deadline}<span class="dl"><Icon icon={Calendar} size={14} />{t('goals.deadline')}: <span class="num">{g.goal.deadline}</span></span>{/if}
                {#if g.goal.query}<code>{g.goal.query}</code>{/if}
              </p>
            </div>
            <div class="progress">
              <div class="figures">
                <span class="amount num">{t('goals.progress', { current: num(g.progress.current), target: num(g.goal.target) })}</span>
                <span class="pc num">{formatPercent100(Math.round(g.progress.percent))}</span>
              </div>
              <ProgressBar
                size="sm"
                label={t(`goals.kind.${g.goal.kind}`)}
                value={Math.round(g.progress.percent)}
                tone={g.progress.done ? 'success' : 'accent'}
              />
            </div>
            <p class="forecast {f?.tone ?? ''}" role="status">
              {#if f}<Icon icon={f.icon} size={14} /><span>{f.text}</span>{/if}
            </p>
            <IconButton
              size="sm"
              icon={Trash2}
              label={t('goals.delete_named', { name: t(`goals.kind.${g.goal.kind}`) })}
              onclick={() => ctx.guarded(() => api.call('goals.delete', { id: g.goal.id }))}
            />
          </li>
        {/each}
      </ul>
    {/if}

    <form
      class="add"
      aria-label={t('goals.new')}
      onsubmit={(e) => {
        e.preventDefault();
        void add();
      }}
    >
      <Select size="sm" label={t('goals.kind.label')} options={kindOptions} bind:value={kind} />
      <Input size="sm" label={t('goals.target')} type="number" min="1" bind:value={target} />
      <Input size="sm" label={t('goals.deadline')} type="date" bind:value={deadline} />
      <Input size="sm" label={t('goals.query')} bind:value={query} />
      <Button type="submit" size="sm" variant="primary" icon={Plus}>{t('goals.add')}</Button>
    </form>
    <p class="hint">{t('goals.new_hint')}</p>
  </Panel>
</div>

<style>
  .goals-page {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  /* The header (title + Statistics tabs) spans the page like on Overview; the forms stay 880. */
  .goals-page > :global(.fc-page-header) {
    margin-bottom: var(--space-1);
  }
  .goals-page > :global(:not(.fc-page-header)) {
    max-width: 880px;
  }
  .pad {
    padding: var(--space-3) var(--space-4);
  }
  .empty {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-2);
    margin: 0;
    font-size: var(--text-sm);
    color: var(--text-2);
    border-bottom: 1px solid var(--border);
  }
  .empty strong {
    font-weight: var(--weight-medium);
    color: var(--text-1);
  }
  .goals {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .goals li {
    display: grid;
    grid-template-columns: 28px minmax(9rem, 1fr) minmax(10rem, 14rem) minmax(8rem, 12rem) auto;
    grid-template-areas: 'ico title progress forecast del';
    align-items: center;
    gap: var(--space-2) var(--space-4);
    min-height: 52px;
    padding: var(--space-2) var(--space-2) var(--space-2) var(--space-4);
    border-bottom: 1px solid var(--border);
  }
  .ico {
    grid-area: ico;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 28px;
    height: 28px;
    border-radius: var(--radius-control);
    background: var(--accent-soft);
    color: var(--accent-text);
  }
  .titles {
    grid-area: title;
    min-width: 0;
  }
  h3 {
    margin: 0;
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
  }
  .meta {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-2);
    margin: 2px 0 0;
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .meta:empty {
    display: none;
  }
  .dl {
    display: inline-flex;
    align-items: center;
    gap: var(--space-1);
  }
  code {
    font-family: var(--font-mono);
    font-size: var(--text-xs);
  }
  .progress {
    grid-area: progress;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .figures {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    gap: var(--space-2);
  }
  .amount {
    font-size: var(--text-sm);
    font-weight: var(--weight-semibold);
  }
  .pc {
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .forecast {
    grid-area: forecast;
    display: flex;
    align-items: center;
    gap: var(--space-1);
    margin: 0;
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .forecast :global(svg) {
    flex: none;
  }
  .forecast.success {
    color: var(--success);
  }
  .forecast.warning {
    color: var(--warning);
  }
  .forecast.accent {
    color: var(--text-1);
  }
  .goals li > :global(.fc-iconbtn) {
    grid-area: del;
  }
  .add {
    display: grid;
    grid-template-columns: minmax(9rem, 1.3fr) 6rem 9.5rem minmax(8rem, 1fr) auto;
    align-items: end;
    gap: var(--space-2);
    padding: var(--space-3) var(--space-4) var(--space-1);
    background: var(--surface-0);
  }
  .hint {
    margin: 0;
    padding: 0 var(--space-4) var(--space-3);
    background: var(--surface-0);
    border-radius: 0 0 var(--radius-card) var(--radius-card);
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .add > :global(.fc-btn) {
    justify-self: start;
  }
  @media (max-width: 1023.98px) {
    .goals li {
      grid-template-columns: 28px minmax(0, 1fr) minmax(9rem, 13rem) auto;
      grid-template-areas:
        'ico title progress del'
        '. forecast forecast forecast';
    }
    .add {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }
</style>
