<script lang="ts">
  import { onMount } from 'svelte';
  import type { IconNode } from 'lucide';
  import { api } from '$lib/api';
  import type { GoalItem, GoalKind, GoalList } from '$lib/api/types';
  import { t } from '$lib/i18n/index.svelte';
  import {
    Button,
    Calendar,
    Card,
    Chip,
    CircleCheck,
    Clock,
    EmptyState,
    Flame,
    GraduationCap,
    Icon,
    IconButton,
    Input,
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
  import { LoadState, SettingsPage, SettingsSection } from '../core/kit';
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

<SettingsPage title={t('goals.heading')} subtitle={t('goals.subtitle')}>
  {#if failed}
    <Card padding="lg"><LoadState state="error" retry={() => void load()} /></Card>
  {:else if !goals}
    <Card padding="lg"><LoadState state="loading" /></Card>
  {:else if goals.items.length === 0}
    <Card padding="lg">
      <EmptyState icon={Target} title={t('goals.none')} description={t('goals.none_hint')} />
    </Card>
  {:else}
    <ul class="goals">
      {#each goals.items as g (g.goal.id)}
        {@const f = forecast(g)}
        <li>
          <Card padding="lg">
            <div class="head">
              <span class="ico" aria-hidden="true"><Icon icon={KIND_ICON[g.goal.kind]} size={20} /></span>
              <div class="titles">
                <h2>{t(`goals.kind.${g.goal.kind}`)}</h2>
                <p class="meta">
                  {#if g.progress.daily}<Chip size="sm" label={t('goals.daily')} />{/if}
                  {#if g.goal.deadline}<span class="dl"><Icon icon={Calendar} size={16} />{t('goals.deadline')}: {g.goal.deadline}</span>{/if}
                  {#if g.goal.query}<code>{g.goal.query}</code>{/if}
                </p>
              </div>
              <IconButton icon={Trash2} label={t('goals.delete_named', { name: t(`goals.kind.${g.goal.kind}`) })} onclick={() => ctx.guarded(() => api.call('goals.delete', { id: g.goal.id }))} />
            </div>
            <div class="progress">
              <span class="amount">{t('goals.progress', { current: num(g.progress.current), target: num(g.goal.target) })}</span>
              <span class="pc">{Math.round(g.progress.percent)}%</span>
            </div>
            <ProgressBar
              size="lg"
              label={t(`goals.kind.${g.goal.kind}`)}
              value={Math.round(g.progress.percent)}
              tone={g.progress.done ? 'success' : 'accent'}
            />
            {#if f}
              <p class="forecast {f.tone}" role="status"><Icon icon={f.icon} size={16} /><span>{f.text}</span></p>
            {/if}
          </Card>
        </li>
      {/each}
    </ul>
  {/if}

  <SettingsSection title={t('goals.new')} icon={Plus} description={t('goals.new_hint')}>
    <form
      onsubmit={(e) => {
        e.preventDefault();
        void add();
      }}
    >
      <div class="grid">
        <Select label={t('goals.kind.label')} options={kindOptions} bind:value={kind} />
        <Input label={t('goals.target')} type="number" min="1" bind:value={target} />
        <Input label={t('goals.deadline')} type="date" bind:value={deadline} />
        <Input label={t('goals.query')} bind:value={query} />
      </div>
      <div class="end">
        <Button type="submit" variant="primary" icon={Plus}>{t('goals.add')}</Button>
      </div>
    </form>
  </SettingsSection>
</SettingsPage>

<style>
  .goals {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  .head {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }
  .ico {
    flex: none;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 40px;
    height: 40px;
    border-radius: var(--radius-control);
    background: var(--accent-soft);
    color: var(--accent);
  }
  .titles {
    flex: 1;
    min-width: 0;
  }
  h2 {
    margin: 0;
    font-size: var(--text-md);
    font-weight: var(--weight-semibold);
  }
  .meta {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-2);
    margin: var(--space-1) 0 0;
    font-size: var(--text-xs);
    color: var(--text-2);
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
    display: flex;
    justify-content: space-between;
    align-items: baseline;
    margin: var(--space-4) 0 var(--space-2);
  }
  .amount {
    font-size: var(--text-lg);
    font-weight: var(--weight-bold);
    font-variant-numeric: tabular-nums;
  }
  .pc {
    font-size: var(--text-sm);
    color: var(--text-2);
    font-variant-numeric: tabular-nums;
  }
  .forecast {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin: var(--space-3) 0 0;
    font-size: var(--text-sm);
    color: var(--text-2);
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
  .grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--space-3) var(--space-4);
  }
  .end {
    display: flex;
    justify-content: flex-end;
    margin-top: var(--space-4);
  }
  @media (max-width: 639.98px) {
    .grid {
      grid-template-columns: minmax(0, 1fr);
    }
  }
</style>
