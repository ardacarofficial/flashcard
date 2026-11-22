<!--
  Statistics → Overview (v1.3): one slim filter row, then three questions in order —
  "how am I doing today?" (`stats.today` + the first unfinished goal), "what's ahead?" (the
  next 7 / 30 days of `stats.overview`'s forecast, today and the busiest day called out) and
  "what am I forgetting?" (`stats.struggles`, whose primary action practises exactly those
  cards) — then the details of the selected range under one heading (summary figures,
  activity heatmap, memory states + success rates, progress + habits, deck report cards,
  classification breakdown, leeches), each with a one-line meaning. Every number comes from
  the backend; this screen only words and draws them.
-->
<script lang="ts">
  import { formatCount, formatPercent100, formatRelative } from '$lib/format';
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type {
    AttributeDefDto,
    DeckReports,
    DeckTreeNode,
    GoalItem,
    LeechAction,
    LeechActionResult,
    LeechList,
    StatsBreakdown,
    StatsOverview,
    StatsScope,
    StruggleList,
    TodayStats,
  } from '$lib/api/types';
  import { i18n, t, td } from '$lib/i18n/index.svelte';
  import {
    Button,
    Chip,
    Clock,
    EmptyState,
    Flame,
    GraduationCap,
    Icon,
    Input,
    Layers,
    Menu,
    PageHeader,
    Play,
    ProgressBar,
    Search,
    SegmentedControl,
    Select,
    Skeleton,
    StatTile,
    Tabs,
    Target,
    TriangleAlert,
    type Tone,
  } from '$lib/ui';
  import type { ScreenCtx } from '../types';
  import { launcher, openLauncher } from '../study/launcher.svelte';
  import { LoadState } from '../core/kit';
  import { moduleKnownOn, trackModules } from '../cards/modules.svelte';
  import BarChart from './BarChart.svelte';
  import Heatmap from './Heatmap.svelte';
  import Panel from './Panel.svelte';
  import { minutes, num, pct, pct100, seconds, shortDate, shortWeekday, tzOffset, type Bar } from './format';

  let { ctx }: { ctx: ScreenCtx } = $props();

  const RANGES = ['30', '90', '365', 'all'] as const;
  type Range = (typeof RANGES)[number];
  const STATES = [
    ['new', 'neutral'],
    ['learning', 'warning'],
    ['relearning', 'danger'],
    ['young', 'accent'],
    ['mature', 'success'],
  ] as const satisfies readonly (readonly [string, Tone])[];

  let deckId = $state('');
  let query = $state('');
  let range = $state<Range>('30');
  let status = $state<'loading' | 'ready' | 'error'>('loading');
  let overview = $state<StatsOverview | null>(null);
  let leeches = $state<LeechList | null>(null);
  let applied = $state<LeechActionResult | null>(null);
  let reports = $state<DeckReports | null>(null);
  let attrs = $state<AttributeDefDto[]>([]);
  let attrId = $state('');
  let breakdown = $state<StatsBreakdown | null>(null);
  let trend = $state<'weekly' | 'monthly'>('weekly');
  let more = $state('hours');
  // v1.3: the three questions.
  let today = $state<TodayStats | null>(null);
  let goal = $state<GoalItem | null>(null);
  let goalsLoaded = $state(false);
  let ahead = $state<'7' | '30'>('7');
  let window_ = $state<'1' | '7' | '30'>('7');
  let struggles = $state<StruggleList | null>(null);
  let notice = $state<string | null>(null);

  const flatten = (nodes: DeckTreeNode[]): { id: string; path: string }[] =>
    nodes.flatMap((n) => [{ id: n.deck.id, path: n.deck.path }, ...flatten(n.children)]);
  const deckOptions = $derived([
    { value: '', label: t('stats.scope.all_decks') },
    ...flatten(ctx.decks).map((d) => ({ value: d.id, label: d.path })),
  ]);
  const rangeOptions = $derived(RANGES.map((r) => ({ value: r, label: t(`stats.range_short.${r}`) })));

  const scope = (): StatsScope => ({ deck_id: deckId || null, tag: null, query: query.trim() || null });
  const days = () => (range === 'all' ? null : Number(range));

  async function load() {
    const tz = tzOffset();
    if (!overview) status = 'loading';
    const ok = await ctx.guarded(async () => {
      // Derived data only: brings the per-day cache up to date (no undo step).
      await api.call('stats.refresh_cache', { tz_offset_mins: tz });
      overview = await api.call('stats.overview', {
        scope: scope(),
        tz_offset_mins: tz,
        days: days(),
        forecast_days: 30,
        fresh: null,
      });
      today = await api.call('stats.today', { scope: scope(), tz_offset_mins: tz });
      leeches = await api.call('stats.leeches', { scope: scope(), threshold: null, limit: 50 });
      reports = await api.call('stats.deck_reports', { tz_offset_mins: tz });
      attrs = (await api.call('attributes.list', { scope_deck_id: null, include_deleted: null })).defs.filter((a) =>
        ['enum', 'ordered_enum', 'multi_enum'].includes(a.kind),
      );
    });
    status = ok ? 'ready' : overview ? 'ready' : 'error';
    if (!attrId && attrs.length > 0) attrId = attrs[0]!.id;
    await Promise.all([loadStruggles(), loadGoal(), loadBreakdown()]);
  }

  async function loadStruggles() {
    await ctx.guarded(async () => {
      struggles = await api.call('stats.struggles', {
        scope: scope(),
        days: Number(window_),
        limit: 20,
        tz_offset_mins: tzOffset(),
      });
    });
  }

  // The goal Home shows too: the first unfinished one (best-effort; the module may be off).
  async function loadGoal() {
    if (!moduleKnownOn('goals')) {
      goal = null;
      goalsLoaded = false;
      return;
    }
    try {
      const g = await api.call('goals.list', { tz_offset_mins: tzOffset() });
      goal = g.items.find((i) => !i.progress.done) ?? g.items[0] ?? null;
    } catch {
      goal = null;
    }
    goalsLoaded = true;
  }

  /**
   * "Study these": the study launcher opens on exactly the listed cards (the query comes from
   * Rust with the list) as practice — the schedule is untouched; the learner presses Start.
   */
  function practise() {
    const query = struggles?.study_query;
    if (!query) return;
    notice = null;
    if (!openLauncher({ deckId: null, kind: 'normal', query, practice: true })) notice = t('stats.struggles.nothing');
  }

  async function suspend(cardId: string) {
    await ctx.guarded(() => api.call('bulk.set_suspended', { card_ids: [cardId], suspended: true }));
    await loadStruggles();
  }

  async function tagHard(noteId: string) {
    await ctx.guarded(() => api.call('bulk.add_tags', { note_ids: [noteId], tags: [t('stats.struggles.tag_name')] }));
  }

  /** The pace sentence of "how am I doing today?" (the verdict is Rust's). */
  function paceLine(s: TodayStats): string {
    const ref = num(s.reference_answered, 0);
    const n = s.today.answered;
    const by = s.reference_is_yesterday ? 'yesterday' : 'week';
    switch (s.pace) {
      case 'not_started':
        return s.yesterday.answered > 0 ? t('stats.today.pace.not_started_ref', { ref: s.yesterday.answered }) : t('stats.today.pace.not_started');
      case 'first_day':
        return t('stats.today.pace.first_day');
      default:
        return td(`stats.today.pace.${s.pace}_${by}`, { ref, n });
    }
  }

  const forecastDays = $derived(overview ? overview.cards.forecast.slice(0, ahead === '7' ? 7 : 30) : []);
  const peakIndex = $derived.by(() => {
    let best = -1;
    forecastDays.forEach((f, i) => {
      if (f.due > 0 && (best < 0 || f.due > forecastDays[best]!.due)) best = i;
    });
    return best;
  });
  const aheadTotal = $derived(forecastDays.reduce((n, f) => n + f.due, 0));
  const studyable = $derived(struggles ? struggles.items.filter((i) => !i.suspended).length : 0);

  async function loadBreakdown() {
    if (!attrId) {
      breakdown = null;
      return;
    }
    await ctx.guarded(async () => {
      breakdown = await api.call('stats.breakdown', {
        scope: scope(),
        attr_id: attrId,
        tz_offset_mins: tzOffset(),
        days: days(),
      });
    });
  }

  // Every leech of the scope, selected in Rust (not just the listed page).
  async function applyToLeeches(action: LeechAction) {
    if (!leeches) return;
    const threshold = leeches.threshold;
    await ctx.guarded(async () => {
      applied = await api.call('stats.apply_to_leeches', { scope: scope(), threshold, action, tag: null });
    });
    await load();
  }

  const bars = <T,>(items: T[], label: (x: T) => string, value: (x: T) => number, title: (x: T) => string): Bar[] =>
    items.map((x) => ({ label: label(x), value: value(x), title: title(x) }));
  const d = (date: string) => shortDate(date, i18n.locale);
  const bucket = (b: { from: number | null; to: number | null }) => (b.to === null ? `> ${b.from}` : `≤ ${b.to}`);
  const timeLabel = (b: { up_to_ms: number | null }) =>
    b.up_to_ms === null ? t('stats.times.over', { s: 60 }) : t('stats.times.up_to', { s: b.up_to_ms / 1000 });
  const rateTone = (rate: number | null | undefined, target: number): Tone =>
    rate === null || rate === undefined ? 'neutral' : rate >= target ? 'success' : rate >= target - 0.1 ? 'accent' : 'warning';
  /** CSS length of a 0–1 rate (bar width / target tick position). */
  const w = (rate: number | null | undefined) => `calc(${Math.max(0, Math.min(1, rate ?? 0))} * 100%)`;

  onMount(() => {
    trackModules();
    void load();
    const unsub = api.on(['study_', 'cards_', 'goals_', 'undone', 'redone', 'collection_replaced', 'data_changed'], () => void load());
    return () => void unsub.then((u) => u());
  });
</script>

<div class="stats">
  <PageHeader title={t('stats.heading')} subtitle={t('stats.subtitle')} />

  <form
    class="filters"
    aria-label={t('stats.scope.label')}
    onsubmit={(e) => {
      e.preventDefault();
      void load();
    }}
  >
    <Select class="f-deck" size="sm" hideLabel label={t('stats.scope.deck')} options={deckOptions} bind:value={deckId} onchange={() => void load()} />
    <SegmentedControl size="sm" label={t('stats.scope.range')} options={rangeOptions} bind:value={range} onchange={() => void load()} />
    <Input
      class="f-query"
      size="sm"
      hideLabel
      label={t('stats.scope.query')}
      icon={Search}
      bind:value={query}
      placeholder={t('stats.scope.query_hint')}
    />
    <Button type="submit" size="sm">{t('stats.scope.apply')}</Button>
  </form>

  {#if status === 'loading' && !overview}
    <div class="tiles">
      {#each { length: 4 } as _, i (i)}<Skeleton shape="card" height="84px" />{/each}
    </div>
    <LoadState state="loading" shape="block" />
  {:else if status === 'error' && !overview}
    <div class="error-box"><LoadState state="error" retry={() => void load()} /></div>
  {:else if overview}
    {@const r = overview.reviews}
    {@const c = overview.cards}
    {@const target = overview.target_retention}
    <!-- 1 · How am I doing today? -->
    <Panel class="q-today" testid="stats-today" title={t('stats.today.heading')} description={t('stats.today.intro')}>
      {#if today}
        <p class="verdict" data-testid="today-verdict">{paceLine(today)}</p>
        <dl class="figs">
          <div>
            <dt>{t('stats.today.answered')}</dt>
            <dd class="num big">{formatCount(today.today.answered)}</dd>
          </div>
          <div>
            <dt>{t('stats.today.correct')}</dt>
            <dd>
              <span class="num big">{pct(today.today.rate)}</span>
              {#if today.today.answered > 0}<span class="sub-fig">{t('stats.today.correct_of', { correct: today.today.correct, n: today.today.answered })}</span>{/if}
            </dd>
          </div>
          <div>
            <dt>{t('stats.today.time')}</dt>
            <dd class="num big">{t('stats.today.minutes', { n: minutes(today.today.time_ms) })}</dd>
          </div>
          <div>
            <dt>{t('stats.today.streak')}</dt>
            <dd class="num big streak-fig"><Icon icon={Flame} size={14} />{t('stats.today.days', { n: today.streak.current })}</dd>
          </div>
          {#if moduleKnownOn('goals')}
            <div class="goal-fig">
              <dt>{t('stats.today.goal')}{#if goal} · {td(`goals.kind.${goal.goal.kind}`)}{/if}</dt>
              <dd>
                {#if goal}
                  <span class="num big">{formatCount(goal.progress.current)} / {formatCount(goal.progress.target)}</span>
                  <ProgressBar
                    size="sm"
                    label={td(`goals.kind.${goal.goal.kind}`)}
                    value={Math.min(goal.progress.current, goal.progress.target)}
                    max={Math.max(1, goal.progress.target)}
                    tone={goal.progress.done ? 'success' : 'accent'}
                  />
                {:else if goalsLoaded}
                  <span class="sub-fig">{t('stats.today.no_goal')}</span>
                  {#if ctx.navigate}<Button size="sm" variant="ghost" icon={Target} onclick={() => ctx.navigate?.('/goals')}>{t('stats.today.set_goal')}</Button>{/if}
                {/if}
              </dd>
            </div>
          {/if}
        </dl>
      {:else}
        <LoadState state="loading" lines={2} />
      {/if}
    </Panel>

    <div class="q-row">
      <!-- 2 · What's ahead? -->
      <Panel class="q-ahead" testid="stats-ahead" title={t('stats.ahead.heading')} description={t('stats.ahead.intro')}>
        {#snippet actions()}
          {#if c.overdue > 0}<Chip size="sm" tone="warning" icon={TriangleAlert} label={t('stats.forecast.overdue', { n: c.overdue })} />{/if}
          <SegmentedControl
            size="sm"
            label={t('stats.ahead.range')}
            options={[
              { value: '7', label: t('stats.ahead.week') },
              { value: '30', label: t('stats.ahead.month') },
            ]}
            bind:value={ahead}
          />
        {/snippet}
        {#if aheadTotal === 0}
          <p class="muted">{t('stats.ahead.empty')}</p>
        {:else}
          <p class="lead-line">
            {#if peakIndex >= 0}<strong data-testid="ahead-peak">{t('stats.ahead.peak', { date: d(forecastDays[peakIndex]!.date), n: forecastDays[peakIndex]!.due })}</strong> · {/if}
            <span class="muted-inline">{t('stats.ahead.total', { n: aheadTotal, days: forecastDays.length })}</span>
          </p>
        {/if}
        <BarChart
          label={t('stats.ahead.chart', { n: forecastDays.length })}
          highlight={0}
          peak={peakIndex >= 0 ? peakIndex : null}
          peakLabel={t('stats.ahead.peak_mark')}
          height={ahead === '7' ? 112 : 104}
          labels={ahead === '7' ? 'all' : 'ends'}
          values={ahead === '7'}
          bars={bars(
            forecastDays,
            (f) => (ahead === '7' ? (f === forecastDays[0] ? t('stats.ahead.today') : shortWeekday(f.date, i18n.locale)) : d(f.date)),
            (f) => f.due,
            (f) => t('stats.forecast.day', { date: d(f.date), n: f.due }),
          )}
        />
        {#if ctx.navigate && moduleKnownOn('planning')}
          <div class="q-foot">
            <span class="muted">{t('stats.forecast.meaning')}</span>
            <Button size="sm" variant="ghost" onclick={() => ctx.navigate?.('/planning')}>{t('stats.ahead.plan')}</Button>
          </div>
        {:else}
          <p class="muted q-foot">{t('stats.forecast.meaning')}</p>
        {/if}
      </Panel>

      <!-- 3 · What am I forgetting? -->
      <Panel class="q-struggles" testid="stats-struggles" title={t('stats.struggles.heading')} description={t('stats.struggles.intro')} body="rows">
        {#snippet actions()}
          <SegmentedControl
            size="sm"
            label={t('stats.struggles.window')}
            options={[
              { value: '1', label: t('stats.struggles.w1') },
              { value: '7', label: t('stats.struggles.w7') },
              { value: '30', label: t('stats.struggles.w30') },
            ]}
            bind:value={window_}
            onchange={() => void loadStruggles()}
          />
        {/snippet}
        {#if !struggles}
          <div class="pad"><LoadState state="loading" lines={3} /></div>
        {:else if struggles.items.length === 0}
          <p class="pad muted">{struggles.days === 1 ? t('stats.struggles.none_today') : t('stats.struggles.none', { n: struggles.days })}</p>
        {:else}
          <div class="study-bar">
            <Button
              variant="primary"
              size="sm"
              icon={Play}
              disabled={studyable === 0 || !launcher.available}
              onclick={practise}
              data-testid="struggles-study">{t('stats.struggles.study_n', { n: studyable })}</Button
            >
            <span class="muted">{t('stats.struggles.study_hint')}</span>
          </div>
          {#if notice}<p class="pad muted status" role="status">{notice}</p>{/if}
          <div class="table-wrap">
            <table class="dt struggles">
              <thead>
                <tr>
                  <th scope="col">{t('stats.struggles.term')}</th>
                  <th scope="col">{t('stats.struggles.meaning')}</th>
                  <th scope="col" class="n">{t('stats.struggles.misses')}</th>
                  <th scope="col" class="n">{t('stats.struggles.last')}</th>
                  <th scope="col" class="act"><span class="sr-only">{t('stats.struggles.actions', { term: '' })}</span></th>
                </tr>
              </thead>
              <tbody>
                {#each struggles.items as it (it.card_id)}
                  <tr class:paused={it.suspended}>
                    <th scope="row" class="term">
                      {it.term}
                      {#if it.suspended}<Chip size="sm" label={t('stats.struggles.paused')} />{/if}
                    </th>
                    <td class="meaning">{it.meaning}</td>
                    <td class="n num miss" title={t('stats.struggles.misses_title', { misses: it.misses, answers: it.answers })}>{it.misses}<span class="of">/{it.answers}</span></td>
                    <td class="n when">{formatRelative(it.last_seen_at)}</td>
                    <td class="act">
                      <Menu
                        label={t('stats.struggles.actions', { term: it.term })}
                        items={[
                          { id: 'suspend', label: t('stats.struggles.suspend'), disabled: it.suspended, onselect: () => void suspend(it.card_id) },
                          { id: 'tag', label: t('stats.struggles.tag'), onselect: () => void tagHard(it.note_id) },
                        ]}
                      />
                    </td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
          {#if struggles.total > struggles.items.length}
            <p class="pad muted">{t('stats.struggles.more', { n: struggles.total - struggles.items.length, shown: struggles.items.length })}</p>
          {/if}
        {/if}
      </Panel>
    </div>

    <!-- Details of the selected range -->
    <header class="details-head">
      <h2>{t('stats.details.heading')}</h2>
      <p>{t('stats.details.intro')}</p>
    </header>
    <section class="tiles" aria-label={t('stats.summary.heading')}>
      <StatTile
        testid="stat-streak"
        icon={Flame}
        label={t('stats.streak.label')}
        value={formatCount(r.streak.current)}
        unit={t('stats.streak.unit', { n: r.streak.current })}
        hint={t('stats.streak.longest', { n: r.streak.longest })}
      />
      <StatTile
        icon={Layers}
        label={t('stats.reviews.label')}
        value={formatCount(r.totals.reviews)}
        hint={[t('stats.reviews.detail', { days: r.totals.days_studied, total: r.totals.days_in_range }), r.totals.imported > 0 ? t('stats.imported', { n: r.totals.imported }) : '']
          .filter(Boolean)
          .join(' · ')}
      />
      <StatTile
        icon={Clock}
        label={t('stats.time.label')}
        value={formatCount(minutes(r.totals.time_ms))}
        unit={t('stats.time.unit')}
        hint={t('stats.times.mean', { s: seconds(r.answer_times.mean_ms) })}
      />
      <StatTile
        icon={Target}
        label={t('stats.retention.label')}
        value={r.true_retention === null ? '–' : pct(r.true_retention, 1)}
        hint={r.true_retention === null ? t('stats.retention.none') : t('stats.retention.target', { target: pct(target) })}
      >
        {#if r.true_retention !== null}
          <span class="meter {rateTone(r.true_retention, target)}" aria-hidden="true">
            <span class="fill" style:width={w(r.true_retention)}></span>
            <span class="tick" style:left={w(target)}></span>
          </span>
        {/if}
      </StatTile>
    </section>

    {#if r.totals.reviews === 0}
      <div class="empty-row">
        <span>{t('stats.empty')} <span class="muted">{t('stats.empty_hint')}</span></span>
        {#if ctx.navigate}
          <Button size="sm" variant="primary" icon={GraduationCap} onclick={() => ctx.navigate?.('/study')}>{t('stats.start')}</Button>
        {/if}
      </div>
    {/if}

    <div class="wrap-row">
      <Panel class="p-activity" title={t('stats.heatmap.heading')} description={t('stats.heatmap.intro')}>
        {#if r.heatmap.length === 0}
          <p class="muted">{t('stats.empty')}</p>
        {:else}
          <Heatmap days={r.heatmap} label={t('stats.heatmap.heading')} />
        {/if}
      </Panel>
    </div>

    <div class="grid2">
      <Panel title={t('stats.states.heading')} description={t('stats.states.meaning')}>
        <ProgressBar
          size="sm"
          label={t('stats.states.heading')}
          segments={STATES.map(([k, tone]) => ({ value: c.states[k], label: t(`stats.states.${k}`), tone }))}
        />
        <dl class="rows legend">
          {#each STATES as [k, tone] (k)}
            <div class="row">
              <dt><span class="dot {tone}" aria-hidden="true"></span>{t(`stats.states.${k}`)}</dt>
              <dd class="num">{formatCount(c.states[k])}</dd>
            </div>
          {/each}
          {#each ['suspended', 'buried'] as const as k (k)}
            <div class="row quiet">
              <dt><span class="dot hollow" aria-hidden="true"></span>{t(`stats.states.${k}`)}</dt>
              <dd class="num">{formatCount(c.states[k])}</dd>
            </div>
          {/each}
          <div class="row total">
            <dt>{t('stats.states.total')}</dt>
            <dd class="num">{formatCount(c.states.total)}</dd>
          </div>
        </dl>
      </Panel>

      <Panel title={t('stats.success.heading')} description={`${t('stats.success.meaning')} ${t('stats.retention.target', { target: pct(target) })}.`}>
        <ul class="rows rates">
          {#each ['learn', 'young', 'mature', 'relearn', 'all'] as const as k (k)}
            {@const s = r.success[k]}
            <li class="row" class:total={k === 'all'}>
              <span class="rate-name">{t(`stats.success.${k}`)}</span>
              <span class="meter {rateTone(s.rate, target)}" role="img" aria-label={`${t(`stats.success.${k}`)}: ${pct(s.rate)}`}>
                <span class="fill" style:width={w(s.rate)}></span>
                <span class="tick" style:left={w(target)}></span>
              </span>
              <span class="num rate-val">{pct(s.rate)}</span>
              <span class="rate-count">{t('stats.success.answers_n', { n: s.count })}</span>
            </li>
          {/each}
        </ul>
      </Panel>
    </div>

    <div class="grid2">
      <Panel title={t('stats.trend.heading')} description={t('stats.trend.meaning')}>
        {#snippet actions()}
          <SegmentedControl
            size="sm"
            label={t('stats.trend.heading')}
            options={[
              { value: 'weekly', label: t('stats.trend.weekly') },
              { value: 'monthly', label: t('stats.trend.monthly') },
            ]}
            bind:value={trend}
          />
        {/snippet}
        <BarChart
          label={t('stats.trend.heading')}
          height={196}
          bars={bars(r[trend], (p) => d(p.start_date), (p) => p.reviews, (p) => `${d(p.start_date)}: ${p.reviews} · ${pct(p.rate)}`)}
        />
      </Panel>

      <Panel title={t('stats.more.heading')} description={t('stats.more.meaning')}>
        <Tabs
          label={t('stats.more.heading')}
          tabs={[
            { id: 'hours', label: t('stats.hours.heading') },
            { id: 'times', label: t('stats.times.heading') },
            { id: 'memory', label: t('stats.memory.heading') },
          ]}
          bind:value={more}
        >
          {#snippet children(id)}
            <div class="tab-panel">
              {#if id === 'hours'}
                <p class="muted">{t('stats.hours.intro')}</p>
                <BarChart
                  label={t('stats.hours.heading')}
                  height={112}
                  formatMax={(n) => formatPercent100(Math.round(n))}
                  bars={bars(r.hours, (h) => `${h.hour}:00`, (h) => (h.stat.rate ?? 0) * 100, (h) => `${h.hour}:00 · ${h.stat.count} · ${pct(h.stat.rate)}`)}
                />
              {:else if id === 'times'}
                <p class="muted">{t('stats.times.meaning')} {t('stats.times.mean', { s: seconds(r.answer_times.mean_ms) })}</p>
                <BarChart
                  label={t('stats.times.heading')}
                  height={112}
                  bars={bars(r.answer_times.buckets, timeLabel, (b) => b.count, (b) => `${timeLabel(b)}: ${b.count}`)}
                />
                {#if r.answer_times.slowest.length > 0}
                  <h3 class="sub">{t('stats.times.slowest')}</h3>
                  <ul class="rows">
                    {#each r.answer_times.slowest as s (s.card_id)}
                      <li class="row"><code>{s.card_id.slice(-8)}</code><span class="num">{t('stats.times.seconds', { s: seconds(s.duration_ms) })}</span></li>
                    {/each}
                  </ul>
                {/if}
              {:else}
                <p class="muted">{t('stats.memory.meaning')}</p>
                <div class="pair">
                  <div>
                    <h3 class="sub">{t('stats.memory.stability')}</h3>
                    <BarChart label={t('stats.memory.stability')} height={96} bars={bars(c.stability, bucket, (b) => b.count, (b) => `${bucket(b)}: ${b.count}`)} />
                  </div>
                  <div>
                    <h3 class="sub">{t('stats.memory.difficulty')}</h3>
                    <BarChart label={t('stats.memory.difficulty')} height={96} bars={bars(c.difficulty, bucket, (b) => b.count, (b) => `${bucket(b)}: ${b.count}`)} />
                  </div>
                </div>
              {/if}
            </div>
          {/snippet}
        </Tabs>
      </Panel>
    </div>
  {/if}

  {#if reports && reports.items.length > 0}
    <Panel title={t('stats.report.heading')} description={t('stats.report.intro')} body="rows">
      <div class="table-wrap">
        <table class="dt">
          <thead>
            <tr>
              <th scope="col">{t('stats.report.deck')}</th>
              <th scope="col" class="bar-col">{t('stats.report.settled')}</th>
              <th scope="col" class="n">{t('stats.report.studied')}</th>
              <th scope="col" class="n">{t('stats.report.recall')}</th>
              <th scope="col" class="n">{t('stats.report.due')}</th>
            </tr>
          </thead>
          <tbody>
            {#each reports.items as dr (dr.deck_id)}
              <tr>
                <th scope="row" class="deck">{dr.path}</th>
                <td class="bar-col">
                  <span class="inline-bar">
                    <span class="meter success" role="img" aria-label={`${dr.path}: ${pct100(dr.report.settled_pct)}`}>
                      <span class="fill" style:width="{dr.report.settled_pct}%"></span>
                    </span>
                    <span class="num">{pct100(dr.report.settled_pct)}</span>
                  </span>
                </td>
                <td class="n num">{pct100(dr.report.studied_pct)}</td>
                <td class="n num">{dr.report.mean_recall === null ? '–' : pct(dr.report.mean_recall)}</td>
                <td class="n num">{num(dr.due_today, 0)}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    </Panel>
  {/if}

  {#if overview && attrs.length > 0}
    <Panel title={t('stats.breakdown.heading')} description={t('stats.breakdown.intro')} body="rows">
      {#snippet actions()}
        <Select
          size="sm"
          hideLabel
          label={t('stats.breakdown.pick')}
          options={attrs.map((a) => ({ value: a.id, label: a.name }))}
          bind:value={attrId}
          onchange={() => void loadBreakdown()}
        />
      {/snippet}
      {#if !breakdown}
        <div class="pad"><LoadState state="loading" /></div>
      {:else if breakdown.rows.length === 0}
        <p class="muted pad">{t('stats.breakdown.empty')}</p>
      {:else}
        <div class="table-wrap">
          <table class="dt">
            <thead>
              <tr>
                <th scope="col">{t('stats.breakdown.value')}</th>
                <th scope="col" class="n">{t('stats.breakdown.cards')}</th>
                <th scope="col" class="bar-col">{t('stats.breakdown.settled')}</th>
                <th scope="col" class="n">{t('stats.breakdown.retention')}</th>
                <th scope="col" class="n">{t('stats.breakdown.due_today')}</th>
                <th scope="col" class="n">{t('stats.breakdown.due_week')}</th>
              </tr>
            </thead>
            <tbody>
              {#each breakdown.rows as row (row.option_id ?? 'none')}
                <tr>
                  <th scope="row">
                    {#if row.label}<Chip size="sm" tone="accent" label={row.label} />{:else}<span class="muted">{t('stats.breakdown.none_value')}</span>{/if}
                  </th>
                  <td class="n num">{formatCount(row.states.total)}</td>
                  <td class="bar-col">
                    <span class="inline-bar">
                      <span class="meter success" role="img" aria-label={`${t('stats.breakdown.settled')}: ${pct100(row.report.settled_pct)}`}>
                        <span class="fill" style:width="{row.report.settled_pct}%"></span>
                      </span>
                      <span class="num">{pct100(row.report.settled_pct)}</span>
                    </span>
                  </td>
                  <td class="n num">{pct(row.true_retention)}</td>
                  <td class="n num">{formatCount(row.due_today)}</td>
                  <td class="n num">{formatCount(row.due_week)}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    </Panel>
  {/if}

  {#if leeches}
    <Panel title={t('stats.leeches.heading')} description={t('stats.leeches.intro')} body="rows">
      {#snippet actions()}
        {#if leeches && leeches.total > 0}
          <Button size="sm" onclick={() => void applyToLeeches('suspend')}>{t('stats.leeches.suspend')}</Button>
          <Button size="sm" variant="ghost" onclick={() => void applyToLeeches('tag')}>{t('stats.leeches.tag')}</Button>
        {/if}
      {/snippet}
      {#if leeches.total === 0}
        <div class="pad"><EmptyState size="sm" icon={TriangleAlert} title={t('stats.leeches.none')} /></div>
      {:else}
        <p class="lead">{t('stats.leeches.count', { n: leeches.total, threshold: leeches.threshold })}</p>
        <ul class="rows leeches">
          {#each leeches.items.slice(0, 8) as l (l.card_id)}
            <li class="row"><span class="leech-text">{l.sort_field}</span><span class="num lapses">{t('stats.leeches.lapses_n', { n: l.lapses })}</span></li>
          {/each}
        </ul>
        {#if leeches.items.length > 8}<p class="muted pad">{t('stats.leeches.more', { n: leeches.total - 8 })}</p>{/if}
      {/if}
      {#if applied}
        <p class="muted pad status" role="status">{t('stats.leeches.applied', { changed: applied.changed, skipped: applied.skipped })}</p>
      {/if}
    </Panel>
  {/if}
</div>

<style>
  .stats {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  .stats > :global(.fc-page-header) {
    margin-bottom: var(--space-1);
  }
  /* --- filter row --- */
  .filters {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-wrap: wrap;
  }
  .filters :global(.f-deck) {
    width: 13rem;
    flex: none;
  }
  .filters :global(.f-query) {
    flex: 1 1 12rem;
    min-width: 10rem;
  }
  /* --- tiles --- */
  .tiles {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: var(--space-2);
  }
  .meter {
    position: relative;
    display: block;
    height: 4px;
    margin: 6px 0 2px;
    border-radius: var(--radius-pill);
    background: var(--surface-3);
  }
  .meter .fill {
    display: block;
    height: 100%;
    border-radius: inherit;
    background: var(--accent);
  }
  .meter.success .fill {
    background: var(--success-solid);
  }
  .meter.warning .fill {
    background: var(--warning-solid);
  }
  .meter.neutral .fill {
    background: var(--text-3);
  }
  .meter .tick {
    position: absolute;
    top: -3px;
    width: 2px;
    height: 10px;
    margin-left: -1px;
    border-radius: 1px;
    background: var(--text-1);
  }
  .empty-row,
  .error-box {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    flex-wrap: wrap;
    padding: var(--space-2) var(--space-4);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--surface-1);
    font-size: var(--text-sm);
  }
  /* --- panel layout --- */
  .wrap-row {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-3);
  }
  .wrap-row > :global(.p-activity) {
    flex: 1 1 auto;
    max-width: 100%;
  }
  /* --- the three questions --- */
  .verdict {
    margin: 0 0 var(--space-3);
    font-size: var(--text-md);
    font-weight: var(--weight-medium);
    color: var(--text-1);
  }
  .figs {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-3) var(--space-6);
    margin: 0;
  }
  .figs > div {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 6rem;
  }
  .figs dt {
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .figs dd {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin: 0;
  }
  .figs .big {
    font-size: var(--text-xl);
    font-weight: var(--weight-semibold);
    line-height: 1.2;
  }
  .streak-fig {
    flex-direction: row !important;
    align-items: center;
    gap: var(--space-1) !important;
  }
  .streak-fig :global(svg) {
    color: var(--warning);
  }
  .goal-fig {
    flex: 1 1 12rem;
    max-width: 22rem;
  }
  .goal-fig dd {
    align-items: flex-start;
  }
  .sub-fig {
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .q-row {
    display: grid;
    grid-template-columns: minmax(0, 5fr) minmax(0, 7fr);
    gap: var(--space-3);
    align-items: start;
  }
  .lead-line {
    margin: 0;
    font-size: var(--text-sm);
  }
  .lead-line strong {
    font-weight: var(--weight-semibold);
  }
  .muted-inline {
    color: var(--text-2);
  }
  .q-foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-2);
    margin-top: var(--space-2);
  }
  .study-bar {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: var(--space-2) var(--space-3);
    padding: var(--space-2) var(--space-4);
    border-bottom: 1px solid var(--border);
  }
  .struggles .term {
    white-space: normal;
    overflow-wrap: anywhere;
  }
  .struggles .term :global(.fc-chip) {
    margin-inline-start: var(--space-1);
  }
  .struggles .meaning {
    white-space: normal;
    color: var(--text-2);
    overflow-wrap: anywhere;
  }
  .struggles .miss {
    color: var(--danger);
  }
  .struggles .of {
    color: var(--text-2);
  }
  .struggles .when {
    color: var(--text-2);
    font-size: var(--text-xs);
  }
  .struggles .act {
    width: 1%;
    padding-inline: var(--space-1) var(--space-3) !important;
  }
  .struggles tr.paused > * {
    color: var(--text-3);
  }
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
    white-space: nowrap;
  }
  .details-head {
    margin-top: var(--space-4);
    padding-top: var(--space-3);
    border-top: 1px solid var(--border);
  }
  .details-head h2 {
    margin: 0;
    font-size: var(--text-md);
    font-weight: var(--weight-semibold);
  }
  .details-head p {
    margin: 2px 0 0;
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .grid2 {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--space-3);
  }
  /* --- dense rows --- */
  .rows {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    min-height: 28px;
    font-size: var(--text-sm);
    border-bottom: 1px solid var(--border);
  }
  .row:last-child {
    border-bottom: none;
  }
  .legend {
    margin-top: var(--space-3);
  }
  .legend dt {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .legend dd {
    margin: 0;
  }
  .row.quiet {
    color: var(--text-2);
  }
  .row.total {
    font-weight: var(--weight-semibold);
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: var(--radius-pill);
    flex: none;
  }
  .dot.neutral {
    background: var(--text-3);
  }
  .dot.warning {
    background: var(--warning-solid);
  }
  .dot.danger {
    background: var(--danger-solid);
  }
  .dot.accent {
    background: var(--accent);
  }
  .dot.success {
    background: var(--success-solid);
  }
  .dot.hollow {
    border: 1.5px solid var(--text-3);
  }
  .rates {
    display: grid;
    grid-template-columns: max-content minmax(3rem, 1fr) 3.5rem 5.5rem;
    column-gap: var(--space-3);
  }
  .rates .row {
    display: grid;
    grid-column: 1 / -1;
    grid-template-columns: subgrid;
    min-height: 32px;
  }
  .rates .meter {
    margin: 0;
  }
  .rate-name {
    white-space: nowrap;
  }
  .rate-val {
    text-align: end;
  }
  .rate-count {
    text-align: end;
    color: var(--text-2);
    font-size: var(--text-xs);
    font-weight: var(--weight-regular);
  }
  .tab-panel {
    padding-top: var(--space-3);
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
  }
  .pair {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--space-4);
  }
  .sub {
    margin: var(--space-2) 0 0;
    font-size: var(--text-xs);
    font-weight: var(--weight-semibold);
    color: var(--text-2);
  }
  .muted {
    margin: 0;
    color: var(--text-2);
    font-size: var(--text-xs);
  }
  .pad {
    padding: var(--space-3) var(--space-4);
  }
  .lead {
    margin: 0;
    padding: var(--space-2) var(--space-4);
    font-size: var(--text-sm);
    border-bottom: 1px solid var(--border);
  }
  .leeches .row {
    padding: 0 var(--space-4);
  }
  .leech-text {
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .lapses {
    flex: none;
    color: var(--danger);
    font-size: var(--text-xs);
  }
  code {
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  /* --- dense tables --- */
  .table-wrap {
    overflow-x: auto;
  }
  .dt {
    width: 100%;
    border-collapse: collapse;
    font-size: var(--text-sm);
  }
  .dt th,
  .dt td {
    height: 32px;
    padding: 0 var(--space-3);
    text-align: start;
    border-bottom: 1px solid var(--border);
    white-space: nowrap;
  }
  .dt th:first-child,
  .dt td:first-child {
    padding-inline-start: var(--space-4);
  }
  .dt th:last-child,
  .dt td:last-child {
    padding-inline-end: var(--space-4);
  }
  .dt thead th {
    height: 28px;
    font-size: var(--text-xs);
    font-weight: var(--weight-medium);
    color: var(--text-2);
  }
  .dt tbody tr:last-child > * {
    border-bottom: none;
  }
  .dt tbody tr:hover > * {
    background: var(--surface-2);
  }
  .dt tbody th {
    font-weight: var(--weight-medium);
  }
  .deck {
    white-space: normal;
    overflow-wrap: anywhere;
  }
  .n {
    text-align: end !important;
  }
  .bar-col {
    width: 30%;
    min-width: 9rem;
  }
  .inline-bar {
    display: grid;
    grid-template-columns: minmax(3rem, 1fr) 3rem;
    align-items: center;
    gap: var(--space-2);
  }
  .inline-bar .meter {
    margin: 0;
  }
  .inline-bar .num {
    text-align: end;
    color: var(--text-2);
    font-size: var(--text-xs);
  }
  @media (max-width: 1100px) {
    .rates {
      grid-template-columns: max-content minmax(2rem, 1fr) 3.25rem 4.75rem;
    }
  }
  @media (max-width: 1100px) {
    .q-row {
      grid-template-columns: minmax(0, 1fr);
    }
  }
  @media (max-width: 899.98px) {
    .grid2 {
      grid-template-columns: minmax(0, 1fr);
    }
    .tiles {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }
</style>
