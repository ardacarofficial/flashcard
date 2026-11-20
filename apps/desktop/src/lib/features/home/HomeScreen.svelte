<!--
  Home (v1.1, docs/11 §3): today's due / learning / new counts with one big "Start studying",
  the inbox notice (when something waits), streak and goal, and the recently studied decks.
  Plain wording, no greetings. Every number is the backend's: `decks.summaries` (today and
  recent decks), `stats.overview` (streak), `goals.list`, `inbox.count`; parts whose module is
  off are left out (rule 7).
-->
<script lang="ts">
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type { DeckSummaries, GoalItem, ModuleDto, Streak } from '$lib/api/types';
  import type { ScreenCtx } from '$lib/features';
  import { t, td } from '$lib/i18n/index.svelte';
  import {
    Button,
    Card,
    CircleCheck,
    EmptyState,
    Flame,
    GraduationCap,
    Inbox,
    Library,
    PageHeader,
    Plus,
    ProgressBar,
    Skeleton,
    Target,
    Icon,
  } from '$lib/ui';
  import DeckCard from '../decks/DeckCard.svelte';
  import { moduleOn, nodesById, summariesById } from '../decks/library';

  let { ctx }: { ctx: ScreenCtx } = $props();

  /** Decks shown under "Recent" (and the fallback when nothing was studied yet). */
  const RECENT = 6;

  let summaries = $state<DeckSummaries | null>(null);
  let failed = $state(false);
  let modules = $state<ModuleDto[]>([]);
  let streak = $state<Streak | null>(null);
  let goal = $state<GoalItem | null>(null);
  let goalsLoaded = $state(false);
  let inbox = $state(0);

  const nodes = $derived(nodesById(ctx.decks));
  const byId = $derived(summariesById(summaries));
  const today = $derived(summaries?.today ?? null);
  const todayCount = $derived(today ? today.due + today.learning + today.new : 0);
  const recentIds = $derived(summaries?.recent.length ? summaries.recent : ctx.decks.map((n) => n.deck.id));
  const recent = $derived(recentIds.flatMap((id) => nodes.get(id) ?? []).slice(0, RECENT));
  const studied = $derived((summaries?.recent.length ?? 0) > 0);

  const statsOn = $derived(moduleOn(modules, 'stats'));
  const goalsOn = $derived(moduleOn(modules, 'goals'));

  const open = (route: string) => ctx.navigate?.(route);

  function study() {
    ctx.selectDeck(null);
    open('/study');
  }

  function openDeck(id: string) {
    ctx.selectDeck(id);
    open('/decks');
  }

  async function loadSummary() {
    try {
      summaries = await api.call('decks.summaries', {});
      failed = false;
    } catch {
      failed = true;
    }
  }

  async function loadExtras(list: ModuleDto[]) {
    const tz = -new Date().getTimezoneOffset();
    if (moduleOn(list, 'stats')) {
      api.call('stats.overview', { scope: { deck_id: null, tag: null, query: null }, tz_offset_mins: tz, days: 1, forecast_days: 1, fresh: null }).then(
        (o) => (streak = o.reviews.streak),
        () => (streak = null),
      );
    }
    if (moduleOn(list, 'goals')) {
      api.call('goals.list', { tz_offset_mins: tz }).then(
        (g) => {
          goal = g.items.find((i) => !i.progress.done) ?? g.items[0] ?? null;
          goalsLoaded = true;
        },
        () => (goalsLoaded = true),
      );
    }
    if (moduleOn(list, 'inbox')) {
      api.call('inbox.count', {}).then(
        (c) => (inbox = c.pending),
        () => (inbox = 0),
      );
    }
  }

  async function loadAll() {
    void loadSummary();
    try {
      modules = (await api.modules.list()).modules;
      await loadExtras(modules);
    } catch {
      /* the rest of Home still shows */
    }
  }

  onMount(() => {
    void loadAll();
    const unsub = api.on(['deck_', 'note', 'card', 'study_', 'undone', 'redone', 'data_changed', 'imported', 'collection_', 'goals_', 'inbox_'], () => void loadAll());
    return () => void unsub.then((u) => u());
  });
</script>

<PageHeader title={t('home.title')} />

{#if failed}
  <Card class="home-failed">
    <p role="alert">{t('home.load_failed')}</p>
    <Button size="sm" onclick={() => void loadAll()}>{t('home.retry')}</Button>
  </Card>
{:else if !summaries}
  <div class="stack" aria-busy="true" aria-label={t('common.loading')}>
    <Skeleton shape="card" height="168px" />
    <div class="stats"><Skeleton shape="card" height="112px" /><Skeleton shape="card" height="112px" /></div>
  </div>
{:else if ctx.decks.length === 0}
  <Card padding="lg">
    <EmptyState
      icon={Library}
      title={t('home.empty_title')}
      description={t('home.empty_body')}
      action={{ label: t('home.empty_action'), icon: Plus, onclick: () => open('/decks') }}
    />
  </Card>
{:else}
  <div class="stack">
    <Card padding="lg" elevated class="today">
      <div class="today-text">
        <span class="eyebrow">{t('home.today')}</span>
        {#if todayCount > 0}
          <p class="big"><span class="num">{todayCount}</span> <span class="unit">{t('home.today_label')}</span></p>
        {:else}
          <p class="big caught"><Icon icon={CircleCheck} size={24} /> {t('home.caught_up')}</p>
          <p class="sub">{t('home.caught_up_body')}</p>
        {/if}
        {#if today}
          <ul class="counts">
            <li><span class="dot due" aria-hidden="true"></span>{t('home.count_due')} <strong>{today.due}</strong></li>
            <li><span class="dot learning" aria-hidden="true"></span>{t('home.count_learning')} <strong>{today.learning}</strong></li>
            <li><span class="dot new" aria-hidden="true"></span>{t('home.count_new')} <strong>{today.new}</strong></li>
          </ul>
        {/if}
      </div>
      <Button variant="primary" size="lg" icon={GraduationCap} disabled={todayCount === 0} onclick={study}>{t('home.start')}</Button>
    </Card>

    {#if inbox > 0}
      <Card padding="md" class="inbox">
        <span class="inbox-text"><Icon icon={Inbox} size={20} /> {t('home.inbox_notice', { count: inbox })}</span>
        <Button size="sm" variant="secondary" onclick={() => open('/inbox')}>{t('home.inbox_open')}</Button>
      </Card>
    {/if}

    {#if statsOn || goalsOn}
      <div class="stats">
        {#if statsOn}
          <Card padding="md" class="stat">
            <span class="stat-label"><Icon icon={Flame} size={16} /> {t('home.streak')}</span>
            {#if streak}
              <span class="stat-value">{t('home.streak_days', { count: streak.current })}</span>
              <span class="stat-sub">
                {streak.studied_today ? t('home.streak_today') : t('home.streak_not_today')} · {t('home.streak_best', { count: streak.longest })}
              </span>
            {:else}
              <Skeleton shape="text" lines={2} />
            {/if}
          </Card>
        {/if}
        {#if goalsOn}
          <Card padding="md" class="stat">
            <span class="stat-label"><Icon icon={Target} size={16} /> {t('home.goal')}</span>
            {#if goal}
              <span class="stat-value small">{td(`goals.kind.${goal.goal.kind}`)}</span>
              <ProgressBar
                size="sm"
                label={td(`goals.kind.${goal.goal.kind}`)}
                value={Math.min(goal.progress.current, goal.progress.target)}
                max={Math.max(1, goal.progress.target)}
                tone={goal.progress.done ? 'success' : 'accent'}
              />
              <span class="stat-sub">
                {goal.progress.done ? t('home.goal_done') : t('home.goal_progress', { current: goal.progress.current, target: goal.progress.target })}
              </span>
            {:else if goalsLoaded}
              <span class="stat-sub">{t('home.goal_none')}</span>
              <Button size="sm" variant="ghost" icon={Plus} onclick={() => open('/goals')}>{t('home.goal_set')}</Button>
            {:else}
              <Skeleton shape="text" lines={2} />
            {/if}
          </Card>
        {/if}
      </div>
    {/if}

    <section aria-labelledby="home-decks">
      <div class="section-head">
        <h2 id="home-decks" class="section-title">{studied ? t('home.recent') : t('home.decks')}</h2>
        <Button size="sm" variant="ghost" onclick={() => open('/decks')}>{t('home.all_decks')}</Button>
      </div>
      <ul class="grid">
        {#each recent as node (node.deck.id)}
          <li><DeckCard {node} summary={byId.get(node.deck.id) ?? null} chips={false} onopen={() => openDeck(node.deck.id)} /></li>
        {/each}
      </ul>
    </section>
  </div>
{/if}

<style>
  .stack {
    display: flex;
    flex-direction: column;
    gap: var(--space-6);
  }
  :global(.today.fc-card) {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-6);
    flex-wrap: wrap;
  }
  .today-text {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    min-width: 0;
  }
  .eyebrow {
    font-size: var(--text-sm);
    font-weight: var(--weight-semibold);
    color: var(--accent);
  }
  .big {
    display: flex;
    align-items: baseline;
    flex-wrap: wrap;
    gap: var(--space-2);
    margin: 0;
  }
  .num {
    font-size: var(--text-3xl);
    font-weight: var(--weight-bold);
    line-height: 1;
    font-variant-numeric: tabular-nums;
  }
  .unit {
    font-size: var(--text-lg);
    color: var(--text-2);
  }
  .big.caught {
    align-items: center;
    font-size: var(--text-xl);
    font-weight: var(--weight-semibold);
    color: var(--success);
  }
  .sub {
    margin: 0;
    color: var(--text-2);
  }
  .counts {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-6);
    margin: var(--space-2) 0 0;
    padding: 0;
    list-style: none;
    font-size: var(--text-sm);
    color: var(--text-2);
  }
  .counts li {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .counts strong {
    color: var(--text-1);
    font-variant-numeric: tabular-nums;
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: var(--radius-pill);
  }
  .dot.due {
    background: var(--accent);
  }
  .dot.learning {
    background: var(--warning-solid);
  }
  .dot.new {
    background: var(--border-strong);
  }
  :global(.inbox.fc-card) {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    background: var(--accent-soft);
    border-color: transparent;
  }
  .inbox-text {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font-weight: var(--weight-medium);
  }
  .stats {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(240px, 1fr));
    gap: var(--space-4);
  }
  :global(.stat.fc-card) {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--space-2);
  }
  :global(.stat.fc-card) :global(.fc-progress) {
    width: 100%;
  }
  .stat-label {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font-size: var(--text-sm);
    font-weight: var(--weight-semibold);
    color: var(--text-2);
  }
  .stat-value {
    font-size: var(--text-xl);
    font-weight: var(--weight-bold);
    font-variant-numeric: tabular-nums;
  }
  .stat-value.small {
    font-size: var(--text-md);
    font-weight: var(--weight-semibold);
  }
  .stat-sub {
    font-size: var(--text-sm);
    color: var(--text-2);
  }
  .section-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: var(--space-3);
  }
  .section-title {
    margin: 0;
    font-size: var(--text-lg);
    font-weight: var(--weight-semibold);
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
    gap: var(--space-4);
    margin: 0;
    padding: 0;
    list-style: none;
  }
  :global(.home-failed.fc-card) {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
  }
  :global(.home-failed.fc-card) p {
    margin: 0;
    color: var(--danger);
  }
</style>
