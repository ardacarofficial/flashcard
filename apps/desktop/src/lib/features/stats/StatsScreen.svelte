<!--
  İlerleme (v1.4, route `stats`; prototype `olustur-ara-istatistik.html`, İlerleme part). The
  first view answers three questions in plain words, one sentence and one small chart each:
  "Bugün ne yaptım?" (`stats.today`, last 7 days of `stats.overview`, today's hard words from
  `stats.struggles` with "Bunları çalış"), "İlerliyor muyum?" (`words.stats`: the one "which
  words do I know" truth, linking to Kelimelerim) and "Yarın ne var?" (the forecast of
  `stats.overview`). Everything else — summary tiles, calendar, recall, stages, trend,
  30 days, hours, answer times, recent struggles, leeches with the guided repair, deck report
  cards — sits under "Daha fazla", each with a one-line meaning. The filter row, the
  Planlama/Hedefler tabs, stability/difficulty and the classification breakdown are behind the
  one "Gelişmiş" switch (`advanced.svelte.ts`). Every number comes from the backend; this
  screen only words and draws them.
-->
<script lang="ts">
  import { formatCount, formatPercent100, formatRelative } from '$lib/format';
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type {
    AttributeDefDto,
    CardRow,
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
    WordStats,
  } from '$lib/api/types';
  import { i18n, t, td } from '$lib/i18n/index.svelte';
  import {
    Button,
    Chip,
    ChevronDown,
    ChevronRight,
    Clock,
    EmptyState,
    Flame,
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
    Wrench,
    type Tone,
  } from '$lib/ui';
  import type { ScreenCtx } from '../types';
  import { launcher, openLauncher } from '../study/launcher.svelte';
  import { LoadState } from '../core/kit';
  import { moduleKnownOn, trackModules } from '../cards/modules.svelte';
  import BarChart from './BarChart.svelte';
  import Heatmap from './Heatmap.svelte';
  import LeechRepair from './LeechRepair.svelte';
  import Panel from './Panel.svelte';
  import { advanced, readMore, writeMore } from './advanced.svelte';
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
  let today = $state<TodayStats | null>(null);
  let goal = $state<GoalItem | null>(null);
  let window_ = $state<'1' | '7' | '30'>('7');
  let struggles = $state<StruggleList | null>(null);
  let hardToday = $state<StruggleList | null>(null);
  let words = $state<WordStats | null>(null);
  let notice = $state<string | null>(null);
  let showMore = $state(readMore());
  let repairing = $state<CardRow | null>(null);

  const adv = $derived(advanced.on);
  const flatten = (nodes: DeckTreeNode[]): { id: string; path: string }[] =>
    nodes.flatMap((n) => [{ id: n.deck.id, path: n.deck.path }, ...flatten(n.children)]);
  const deckOptions = $derived([
    { value: '', label: t('stats.scope.all_decks') },
    ...flatten(ctx.decks).map((d) => ({ value: d.id, label: d.path })),
  ]);
  const rangeOptions = $derived(RANGES.map((r) => ({ value: r, label: t(`stats.range_short.${r}`) })));

  // The filter is a Gelişmiş tool: switched off, the page always shows the whole collection.
  const scope = (): StatsScope =>
    adv ? { deck_id: deckId || null, tag: null, query: query.trim() || null } : { deck_id: null, tag: null, query: null };
  const days = () => (!adv || range === 'all' ? (adv ? null : 30) : Number(range));

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
    await Promise.all([loadStruggles(), loadHardToday(), loadGoal(), loadWords(), adv ? loadBreakdown() : null]);
  }

  async function loadStruggles() {
    await ctx.guarded(async () => {
      struggles = await api.call('stats.struggles', { scope: scope(), days: Number(window_), limit: 20, tz_offset_mins: tzOffset() });
    });
  }

  async function loadHardToday() {
    await ctx.guarded(async () => {
      hardToday = await api.call('stats.struggles', { scope: scope(), days: 1, limit: 5, tz_offset_mins: tzOffset() });
    });
  }

  // "İlerliyor muyum?": the word index (best-effort; the module may be off).
  async function loadWords() {
    if (!moduleKnownOn('words')) {
      words = null;
      return;
    }
    try {
      words = await api.call('words.stats', {});
    } catch {
      words = null;
    }
  }

  async function loadGoal() {
    if (!moduleKnownOn('goals')) {
      goal = null;
      return;
    }
    try {
      const g = await api.call('goals.list', { tz_offset_mins: tzOffset() });
      goal = g.items.find((i) => !i.progress.done) ?? g.items[0] ?? null;
    } catch {
      goal = null;
    }
  }

  /**
   * "Study these": the study launcher opens on exactly the listed cards (the query comes from
   * Rust with the list) as practice — the schedule is untouched; the learner presses Start.
   */
  function practise(list: StruggleList | null) {
    const query = list?.study_query;
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

  /** The pace sentence (the verdict is Rust's). */
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

  /** The last 7 study days of the heatmap Rust sent (days without answers drawn as 0). */
  const last7 = $derived.by(() => {
    if (!overview) return [] as Bar[];
    const byDay = new Map(overview.reviews.heatmap.map((h) => [h.day, h.reviews]));
    const base = new Date(`${overview.today_date}T00:00:00Z`).getTime();
    return Array.from({ length: 7 }, (_, i) => {
      const day = overview!.today - 6 + i;
      const date = new Date(base - (6 - i) * 86_400_000).toISOString().slice(0, 10);
      const n = byDay.get(day) ?? 0;
      const label = i === 6 ? t('stats.q.today.today') : shortWeekday(date, i18n.locale);
      return { label, value: n, title: t('stats.q.today.day', { date: d(date), n }) };
    });
  });

  // Forecast[0] is today (overdue included); "tomorrow" is the next day Rust sent.
  const next7 = $derived(overview ? overview.cards.forecast.slice(1, 8) : []);
  const forecast30 = $derived(overview ? overview.cards.forecast.slice(0, 30) : []);
  const peakIndex = $derived.by(() => {
    let best = -1;
    next7.forEach((f, i) => {
      if (f.due > 0 && (best < 0 || f.due > next7[best]!.due)) best = i;
    });
    return best;
  });
  const studyable = $derived(struggles ? struggles.items.filter((i) => !i.suspended).length : 0);
  const hardStudyable = $derived(hardToday ? hardToday.items.filter((i) => !i.suspended).length : 0);

  async function loadBreakdown() {
    if (!attrId) {
      breakdown = null;
      return;
    }
    await ctx.guarded(async () => {
      breakdown = await api.call('stats.breakdown', { scope: scope(), attr_id: attrId, tz_offset_mins: tzOffset(), days: days() });
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

  function toggleMore() {
    showMore = !showMore;
    writeMore(showMore);
  }

  const languageName = (code: string) => {
    try {
      return new Intl.DisplayNames([i18n.locale], { type: 'language' }).of(code) ?? code;
    } catch {
      return code;
    }
  };

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

  // Switching Gelişmiş changes the scope (filters apply only there): reload.
  let lastAdv = advanced.on;
  $effect(() => {
    if (adv !== lastAdv) {
      lastAdv = adv;
      void load();
    }
  });

  onMount(() => {
    trackModules();
    void load();
    const unsub = api.on(['study_', 'cards_', 'goals_', 'undone', 'redone', 'collection_replaced', 'data_changed', 'word'], () => void load());
    return () => void unsub.then((u) => u());
  });
</script>

<div class="stats">
  <PageHeader title={t('stats.heading')} subtitle={t('stats.subtitle')}>
    {#snippet actions()}
      {#if adv && ctx.navigate}
        <nav class="subnav" aria-label={t('stats.tabs.label')}>
          {#if moduleKnownOn('planning')}<Button size="sm" variant="ghost" onclick={() => ctx.navigate?.('/planning')}>{t('nav.planning')}</Button>{/if}
          {#if moduleKnownOn('goals')}<Button size="sm" variant="ghost" onclick={() => ctx.navigate?.('/goals')}>{t('nav.goals')}</Button>{/if}
        </nav>
      {/if}
    {/snippet}
  </PageHeader>

  {#if adv}
    <form
      class="filters"
      aria-label={t('stats.scope.label')}
      data-testid="stats-filters"
      onsubmit={(e) => {
        e.preventDefault();
        void load();
      }}
    >
      <Select class="f-deck" size="sm" hideLabel label={t('stats.scope.deck')} options={deckOptions} bind:value={deckId} onchange={() => void load()} />
      <SegmentedControl size="sm" label={t('stats.scope.range')} options={rangeOptions} bind:value={range} onchange={() => void load()} />
      <Input class="f-query" size="sm" hideLabel label={t('stats.scope.query')} icon={Search} bind:value={query} placeholder={t('stats.scope.query_hint')} />
      <Button type="submit" size="sm">{t('stats.scope.apply')}</Button>
    </form>
  {/if}

  {#if status === 'loading' && !overview}
    <div class="answers">
      {#each { length: 3 } as _, i (i)}<Skeleton shape="card" height="220px" />{/each}
    </div>
  {:else if status === 'error' && !overview}
    <div class="error-box"><LoadState state="error" retry={() => void load()} /></div>
  {:else if overview}
    {@const r = overview.reviews}
    {@const c = overview.cards}
    {@const target = overview.target_retention}
    <div class="answers">
      <!-- 1 · Bugün ne yaptım? -->
      <section class="ans" aria-labelledby="q-today" data-testid="stats-today">
        <h2 class="q" id="q-today">{t('stats.q.today.heading')}</h2>
        {#if !today}
          <LoadState state="loading" lines={2} />
        {:else if today.today.answered === 0}
          <p class="say" data-testid="today-say">{t('stats.q.today.none')}</p>
          <p class="sub" data-testid="today-verdict">{paceLine(today)}</p>
          <div class="foot">
            {#if launcher.available}
              <Button size="sm" variant="primary" icon={Play} onclick={() => openLauncher({ deckId: null })}>{t('stats.q.today.start')}</Button>
            {/if}
          </div>
        {:else}
          <p class="say" data-testid="today-say">{t('stats.q.today.say', { n: today.today.answered, correct: today.today.correct })}</p>
          <p class="sub">
            {t('stats.q.today.minutes', { n: minutes(today.today.time_ms) })} · {t('stats.q.today.started', { n: today.today.new_cards })} ·
            <span class="streak"><Icon icon={Flame} size={12} />{t('stats.q.today.streak', { n: today.streak.current })}</span>
          </p>
          <p class="sub" data-testid="today-verdict">{paceLine(today)}</p>
          {#if goal}
            <div class="goal">
              <span class="sub">{t('stats.today.goal')} · {td(`goals.kind.${goal.goal.kind}`)}: <span class="num">{formatCount(goal.progress.current)} / {formatCount(goal.progress.target)}</span></span>
              <ProgressBar
                size="sm"
                label={td(`goals.kind.${goal.goal.kind}`)}
                value={Math.min(goal.progress.current, goal.progress.target)}
                max={Math.max(1, goal.progress.target)}
                tone={goal.progress.done ? 'success' : 'accent'}
              />
            </div>
          {/if}
          <BarChart label={t('stats.q.today.chart')} highlight={6} height={92} labels="all" values bars={last7} />
          <div class="foot" data-testid="today-hard">
            {#if hardToday && hardToday.items.length > 0}
              <span class="hard">{t('stats.q.today.hard', { list: hardToday.items.map((i) => i.term).join(', ') })}</span>
              <Button size="sm" icon={Play} disabled={hardStudyable === 0 || !launcher.available} onclick={() => practise(hardToday)} data-testid="today-hard-study"
                >{t('stats.struggles.study_n', { n: hardStudyable })}</Button
              >
            {:else}
              <span class="sub">{t('stats.q.today.no_hard')}</span>
            {/if}
          </div>
        {/if}
      </section>

      <!-- 2 · İlerliyor muyum? -->
      <section class="ans" aria-labelledby="q-growing" data-testid="stats-growing">
        <h2 class="q" id="q-growing">{t('stats.q.growing.heading')}</h2>
        {#if words && words.languages.length > 0}
          {@const main = words.languages[0]!}
          {@const k = main.counts}
          <p class="say" data-testid="growing-say">
            {k.known > 0 ? t('stats.q.growing.say', { n: k.known }) : t('stats.q.growing.none')}
          </p>
          <p class="sub">{t('stats.q.growing.learning', { n: k.learning, fresh: k.new })}</p>
          <ProgressBar
            size="md"
            label={t('stats.q.growing.chart', { lang: languageName(main.language) })}
            segments={[
              { value: k.known, label: t('stats.q.growing.known'), tone: 'success' },
              { value: k.learning, label: t('stats.q.growing.learning_label'), tone: 'warning' },
              { value: k.new, label: t('stats.q.growing.new'), tone: 'neutral' },
            ]}
          />
          <ul class="legend-inline">
            <li><span class="dot success" aria-hidden="true"></span>{t('stats.q.growing.known')} <span class="num">{formatCount(k.known)}</span></li>
            <li><span class="dot warning" aria-hidden="true"></span>{t('stats.q.growing.learning_label')} <span class="num">{formatCount(k.learning)}</span></li>
            <li><span class="dot neutral" aria-hidden="true"></span>{t('stats.q.growing.new')} <span class="num">{formatCount(k.new)}</span></li>
          </ul>
          {#if words.languages.length > 1}
            <p class="sub">
              {words.languages
                .slice(1)
                .map((l) => t('stats.q.growing.other', { lang: languageName(l.language), n: l.counts.known }))
                .join(' · ')}
            </p>
          {/if}
          {#if words.unassigned_notes > 0}
            <p class="sub warn" data-testid="growing-unassigned">{t('stats.q.growing.unassigned', { n: words.unassigned_notes })}</p>
          {/if}
          <div class="foot">
            <span class="sub">{t('stats.q.growing.meaning', { days: words.threshold_days })}</span>
            {#if ctx.navigate}
              <Button size="sm" variant="ghost" iconEnd={ChevronRight} onclick={() => ctx.navigate?.('/words')} data-testid="growing-words">{t('stats.q.growing.open')}</Button>
            {/if}
          </div>
        {:else if words}
          <p class="say" data-testid="growing-say">{t('stats.q.growing.none')}</p>
          <p class="sub">{words.unassigned_notes > 0 ? t('stats.q.growing.unassigned', { n: words.unassigned_notes }) : t('stats.q.growing.empty')}</p>
        {:else}
          <!-- Without the word index: the cards' own report card (settled = mature). -->
          <p class="say" data-testid="growing-say">{t('stats.q.growing.cards', { n: c.report.mature, pct: pct100(c.report.settled_pct) })}</p>
          <p class="sub">{t('stats.q.growing.cards_sub')}</p>
          <ProgressBar
            size="md"
            label={t('stats.states.heading')}
            segments={STATES.map(([k, tone]) => ({ value: c.states[k], label: t(`stats.states.${k}`), tone }))}
          />
        {/if}
      </section>

      <!-- 3 · Yarın ne var? -->
      <section class="ans" aria-labelledby="q-tomorrow" data-testid="stats-ahead">
        <h2 class="q" id="q-tomorrow">
          {t('stats.q.tomorrow.heading')}
          {#if c.overdue > 0}<Chip size="sm" tone="warning" icon={TriangleAlert} label={t('stats.forecast.overdue', { n: c.overdue })} />{/if}
        </h2>
        {#if next7.length === 0 || next7.every((f) => f.due === 0)}
          <p class="say" data-testid="tomorrow-say">{t('stats.q.tomorrow.none')}</p>
          <p class="sub">{t('stats.q.tomorrow.none_sub')}</p>
        {:else}
          <p class="say" data-testid="tomorrow-say">{t('stats.q.tomorrow.say', { n: next7[0]!.due })}</p>
          {#if peakIndex >= 0}
            <p class="sub" data-testid="ahead-peak">
              {peakIndex === 0
                ? t('stats.q.tomorrow.peak_tomorrow')
                : t('stats.q.tomorrow.peak', { day: shortWeekday(next7[peakIndex]!.date, i18n.locale), date: d(next7[peakIndex]!.date), n: next7[peakIndex]!.due })}
            </p>
          {/if}
          <BarChart
            label={t('stats.ahead.chart', { n: next7.length })}
            highlight={0}
            height={92}
            labels="all"
            values
            bars={bars(
              next7,
              (f) => (f === next7[0] ? t('stats.q.tomorrow.tomorrow') : shortWeekday(f.date, i18n.locale)),
              (f) => f.due,
              (f) => t('stats.forecast.day', { date: d(f.date), n: f.due }),
            )}
          />
        {/if}
        <div class="foot">
          <span class="sub">{t('stats.q.tomorrow.meaning')}</span>
          {#if ctx.navigate && moduleKnownOn('planning')}
            <Button size="sm" variant="ghost" iconEnd={ChevronRight} onclick={() => ctx.navigate?.('/planning')}>{t('stats.ahead.plan')}</Button>
          {/if}
        </div>
      </section>
    </div>

    <button type="button" class="more-toggle" aria-expanded={showMore} aria-controls="stats-more" onclick={toggleMore} data-testid="stats-more-toggle">
      <Icon icon={showMore ? ChevronDown : ChevronRight} size={16} />
      <strong>{t('stats.more_toggle.label')}</strong>
      <span class="muted">{t('stats.more_toggle.hint')}</span>
    </button>

    {#if showMore}
      <div id="stats-more" class="more">
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
              height={160}
              bars={bars(r[trend], (p) => d(p.start_date), (p) => p.reviews, (p) => `${d(p.start_date)}: ${p.reviews} · ${pct(p.rate)}`)}
            />
          </Panel>

          <Panel title={t('stats.forecast30.heading')} description={t('stats.forecast30.meaning')}>
            <BarChart
              label={t('stats.ahead.chart', { n: forecast30.length })}
              highlight={0}
              height={160}
              bars={bars(forecast30, (f) => (f === forecast30[0] ? t('stats.ahead.today') : d(f.date)), (f) => f.due, (f) => t('stats.forecast.day', { date: d(f.date), n: f.due }))}
            />
          </Panel>
        </div>

        <Panel title={t('stats.more.heading')} description={t('stats.more.meaning')}>
          <Tabs
            label={t('stats.more.heading')}
            tabs={[
              { id: 'hours', label: t('stats.hours.heading') },
              { id: 'times', label: t('stats.times.heading') },
              ...(adv ? [{ id: 'memory', label: t('stats.memory.heading') }] : []),
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
                  <BarChart label={t('stats.times.heading')} height={112} bars={bars(r.answer_times.buckets, timeLabel, (b) => b.count, (b) => `${timeLabel(b)}: ${b.count}`)} />
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

        <!-- Recent struggles (v1.3's third question; today's are in "Bugün ne yaptım?"). -->
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
                onclick={() => practise(struggles)}
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

        {#if leeches}
          <Panel title={t('stats.leeches.heading')} description={t('stats.leeches.intro')} body="rows" testid="stats-leeches">
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
                  <li class="row">
                    <span class="leech-text">{l.sort_field}</span>
                    <span class="num lapses">{t('stats.leeches.lapses_n', { n: l.lapses })}</span>
                    <Button size="sm" variant="secondary" icon={Wrench} onclick={() => (repairing = l)} data-testid="leech-fix">{t('stats.repair.open')}</Button>
                  </li>
                {/each}
              </ul>
              {#if leeches.items.length > 8}<p class="muted pad">{t('stats.leeches.more', { n: leeches.total - 8 })}</p>{/if}
            {/if}
            {#if applied}
              <p class="muted pad status" role="status">{t('stats.leeches.applied', { changed: applied.changed, skipped: applied.skipped })}</p>
            {/if}
          </Panel>
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

        {#if adv && attrs.length > 0}
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
        {#if !adv}
          <p class="muted adv-note">{t('stats.adv_note')}</p>
        {/if}
      </div>
    {/if}
  {/if}
</div>

{#if repairing}
  <LeechRepair
    cardId={repairing.card_id}
    term={repairing.sort_field}
    guarded={ctx.guarded}
    onclose={() => (repairing = null)}
    onapplied={() => void load()}
  />
{/if}

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
  @media (max-width: 899.98px) {
    .grid2 {
      grid-template-columns: minmax(0, 1fr);
    }
    .tiles {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }
  /* --- v1.4: three answers --- */
  .subnav {
    display: flex;
    gap: var(--space-1);
  }
  .answers {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: var(--space-3);
  }
  .ans {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    min-width: 0;
    padding: var(--space-4);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--surface-1);
  }
  .ans .q {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin: 0;
    font-size: var(--text-xs);
    font-weight: var(--weight-semibold);
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--text-2);
  }
  .ans .say {
    margin: 0;
    font-size: var(--text-md);
    font-weight: var(--weight-medium);
    line-height: 1.4;
    color: var(--text-1);
  }
  .ans .sub {
    margin: 0;
    font-size: var(--text-sm);
    color: var(--text-2);
  }
  .ans .sub.warn {
    color: var(--warning);
  }
  .ans .streak {
    display: inline-flex;
    align-items: center;
    gap: 2px;
  }
  .ans .streak :global(svg) {
    color: var(--warning);
  }
  .ans .goal {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .ans .foot {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-wrap: wrap;
    margin-top: auto;
    padding-top: var(--space-2);
    border-top: 1px solid var(--border);
  }
  .ans .foot .sub {
    flex: 1 1 10rem;
  }
  .ans .hard {
    flex: 1 1 10rem;
    font-size: var(--text-sm);
  }
  .legend-inline {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1) var(--space-3);
    margin: 0;
    padding: 0;
    list-style: none;
    font-size: var(--text-sm);
    color: var(--text-2);
  }
  .legend-inline li {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .num {
    font-family: var(--font-mono);
  }
  .more-toggle {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    width: 100%;
    margin-top: var(--space-2);
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--surface-1);
    color: var(--text-1);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .more-toggle:hover {
    background: var(--surface-2);
  }
  .more-toggle:focus-visible {
    outline: 2px solid var(--focus-ring, var(--accent));
    outline-offset: 2px;
  }
  .more-toggle .muted {
    flex: 1;
    font-size: var(--text-sm);
  }
  .more {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
  }
  .leeches .row {
    gap: var(--space-3);
  }
  .adv-note {
    font-size: var(--text-xs);
  }
  @media (max-width: 1100px) {
    .answers {
      grid-template-columns: minmax(0, 1fr);
    }
  }
</style>
