<script lang="ts">
  import { formatPercent100 } from '$lib/format';
  import { onMount } from 'svelte';
  import { api } from '$lib/api';
  import type {
    AttributeDefDto,
    DeckReports,
    DeckTreeNode,
    LeechAction,
    LeechActionResult,
    LeechList,
    StatsBreakdown,
    StatsOverview,
    StatsScope,
  } from '$lib/api/types';
  import { i18n, t } from '$lib/i18n/index.svelte';
  import {
    Activity,
    BarChart3,
    Button,
    Card,
    Chip,
    Clock,
    EmptyState,
    Flag,
    Flame,
    GraduationCap,
    Input,
    Layers,
    ProgressBar,
    ProgressRing,
    Search,
    SegmentedControl,
    Select,
    Skeleton,
    Tabs,
    Target,
    TrendingUp,
    TriangleAlert,
    type Tone,
  } from '$lib/ui';
  import type { ScreenCtx } from '../types';
  import { LoadState, SettingsPage, SettingsSection } from '../core/kit';
  import BarChart from './BarChart.svelte';
  import Heatmap from './Heatmap.svelte';
  import StatTile from './StatTile.svelte';
  import { minutes, num, pct, pct100, seconds, shortDate, tzOffset, type Bar } from './format';

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
      leeches = await api.call('stats.leeches', { scope: scope(), threshold: null, limit: 50 });
      reports = await api.call('stats.deck_reports', { tz_offset_mins: tz });
      attrs = (await api.call('attributes.list', { scope_deck_id: null, include_deleted: null })).defs.filter((a) =>
        ['enum', 'ordered_enum', 'multi_enum'].includes(a.kind),
      );
    });
    status = ok ? 'ready' : overview ? 'ready' : 'error';
    if (!attrId && attrs.length > 0) attrId = attrs[0]!.id;
    await loadBreakdown();
  }

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

  onMount(() => {
    void load();
    const unsub = api.on(['study_', 'cards_', 'undone', 'redone', 'collection_replaced', 'data_changed'], () => void load());
    return () => void unsub.then((u) => u());
  });
</script>

<SettingsPage title={t('stats.heading')} subtitle={t('stats.subtitle')} width="wide">
  <Card padding="md" class="filters">
    <form
      class="filter-row"
      aria-label={t('stats.scope.label')}
      onsubmit={(e) => {
        e.preventDefault();
        void load();
      }}
    >
      <Select
        class="f-deck"
        label={t('stats.scope.deck')}
        options={deckOptions}
        bind:value={deckId}
        onchange={() => void load()}
      />
      <div class="f-range">
        <span class="f-label" aria-hidden="true">{t('stats.scope.range')}</span>
        <SegmentedControl label={t('stats.scope.range')} options={rangeOptions} bind:value={range} onchange={() => void load()} />
      </div>
      <Input class="f-query" label={t('stats.scope.query')} icon={Search} bind:value={query} placeholder={t('stats.scope.query_hint')} />
      <Button type="submit" class="f-apply">{t('stats.scope.apply')}</Button>
    </form>
  </Card>

  {#if status === 'loading' && !overview}
    <div class="tiles">
      {#each { length: 4 } as _, i (i)}<Skeleton shape="card" height="118px" />{/each}
    </div>
    <LoadState state="loading" shape="block" />
  {:else if status === 'error' && !overview}
    <Card padding="lg"><LoadState state="error" retry={() => void load()} /></Card>
  {:else if overview}
    {@const r = overview.reviews}
    {@const c = overview.cards}
    <section class="tiles" aria-label={t('stats.overview')}>
      <StatTile icon={Flame} label={t('stats.streak.label')} value={t('stats.streak.days', { n: r.streak.current })} detail={t('stats.streak.longest', { n: r.streak.longest })} />
      <StatTile
        icon={Layers}
        label={t('stats.reviews.label')}
        value={String(r.totals.reviews)}
        detail={[t('stats.reviews.detail', { days: r.totals.days_studied, total: r.totals.days_in_range }), r.totals.imported > 0 ? t('stats.imported', { n: r.totals.imported }) : '']
          .filter(Boolean)
          .join(' · ')}
      />
      <StatTile icon={Clock} label={t('stats.time.label')} value={t('stats.time.minutes', { n: minutes(r.totals.time_ms) })} detail={t('stats.times.mean', { s: seconds(r.answer_times.mean_ms) })} />
      <StatTile
        icon={Target}
        label={t('stats.retention.label')}
        value={r.true_retention === null ? t('stats.retention.none') : pct(r.true_retention, 1)}
        detail={t('stats.retention.target', { target: pct(overview.target_retention) })}
      >
        {#snippet aside()}
          {#if r.true_retention !== null}
            <ProgressRing
              label={t('stats.retention.label')}
              value={Math.round(r.true_retention * 100)}
              size={52}
              stroke={5}
              tone={rateTone(r.true_retention, overview!.target_retention)}
            />
          {/if}
        {/snippet}
      </StatTile>
    </section>

    {#if r.totals.reviews === 0}
      <Card padding="lg">
        <EmptyState
          icon={BarChart3}
          title={t('stats.empty')}
          description={t('stats.empty_hint')}
          action={ctx.navigate ? { label: t('stats.start'), icon: GraduationCap, onclick: () => ctx.navigate?.('/study') } : undefined}
        />
      </Card>
    {/if}

    <SettingsSection title={t('stats.heatmap.heading')} icon={Activity} description={t('stats.heatmap.intro')}>
      {#if r.heatmap.length === 0}
        <EmptyState size="sm" title={t('stats.empty')} />
      {:else}
        <Heatmap days={r.heatmap} label={t('stats.heatmap.heading')} />
      {/if}
    </SettingsSection>

    <SettingsSection title={t('stats.forecast.heading')} icon={TrendingUp} description={t('stats.forecast.intro')}>
      {#snippet actions()}
        {#if c.overdue > 0}<Chip tone="warning" icon={TriangleAlert} label={t('stats.forecast.overdue', { n: c.overdue })} />{/if}
      {/snippet}
      <BarChart
        label={t('stats.forecast.heading')}
        highlight={0}
        bars={bars(c.forecast, (f) => d(f.date), (f) => f.due, (f) => t('stats.forecast.day', { date: d(f.date), n: f.due }))}
      />
    </SettingsSection>

    <div class="cols">
      <SettingsSection title={t('stats.states.heading')} icon={Layers}>
        <ProgressBar
          size="lg"
          label={t('stats.states.heading')}
          segments={STATES.map(([k, tone]) => ({ value: c.states[k], label: t(`stats.states.${k}`), tone }))}
        />
        <dl class="legend">
          {#each STATES as [k, tone] (k)}
            <div class="lg-row">
              <dt><span class="dot {tone}" aria-hidden="true"></span>{t(`stats.states.${k}`)}</dt>
              <dd>{c.states[k]}</dd>
            </div>
          {/each}
          {#each ['suspended', 'buried'] as const as k (k)}
            <div class="lg-row quiet">
              <dt><span class="dot hollow" aria-hidden="true"></span>{t(`stats.states.${k}`)}</dt>
              <dd>{c.states[k]}</dd>
            </div>
          {/each}
          <div class="lg-row total">
            <dt>{t('stats.states.total')}</dt>
            <dd>{c.states.total}</dd>
          </div>
        </dl>
      </SettingsSection>

      <SettingsSection title={t('stats.success.heading')} icon={Target} description={t('stats.retention.target', { target: pct(overview.target_retention) })}>
        <ul class="rates">
          {#each ['learn', 'young', 'mature', 'relearn', 'all'] as const as k (k)}
            {@const s = r.success[k]}
            <li class:all={k === 'all'}>
              <div class="rate-head">
                <span class="rate-name">{t(`stats.success.${k}`)}</span>
                <span class="rate-val">{pct(s.rate)}</span>
              </div>
              <ProgressBar
                size="sm"
                label={`${t(`stats.success.${k}`)}: ${pct(s.rate)}`}
                value={Math.round((s.rate ?? 0) * 100)}
                tone={rateTone(s.rate, overview.target_retention)}
              />
              <span class="rate-count">{t('stats.success.answers_n', { n: s.count })}</span>
            </li>
          {/each}
        </ul>
      </SettingsSection>
    </div>

    <SettingsSection title={t('stats.trend.heading')} icon={BarChart3}>
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
        bars={bars(r[trend], (p) => d(p.start_date), (p) => p.reviews, (p) => `${d(p.start_date)}: ${p.reviews} · ${pct(p.rate)}`)}
      />
    </SettingsSection>

    <SettingsSection title={t('stats.more.heading')} icon={Clock}>
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
                formatMax={(n) => formatPercent100(Math.round(n))}
                bars={bars(r.hours, (h) => `${h.hour}:00`, (h) => (h.stat.rate ?? 0) * 100, (h) => `${h.hour}:00 · ${h.stat.count} · ${pct(h.stat.rate)}`)}
              />
            {:else if id === 'times'}
              <p class="muted">{t('stats.times.mean', { s: seconds(r.answer_times.mean_ms) })}</p>
              <BarChart
                label={t('stats.times.heading')}
                bars={bars(r.answer_times.buckets, timeLabel, (b) => b.count, (b) => `${timeLabel(b)}: ${b.count}`)}
              />
              {#if r.answer_times.slowest.length > 0}
                <h3 class="sub">{t('stats.times.slowest')}</h3>
                <ul class="slowest">
                  {#each r.answer_times.slowest as s (s.card_id)}
                    <li><code>{s.card_id.slice(-8)}</code><span>{t('stats.times.seconds', { s: seconds(s.duration_ms) })}</span></li>
                  {/each}
                </ul>
              {/if}
            {:else}
              <div class="cols tight">
                <div>
                  <h3 class="sub">{t('stats.memory.stability')}</h3>
                  <BarChart label={t('stats.memory.stability')} height={110} bars={bars(c.stability, bucket, (b) => b.count, (b) => `${bucket(b)}: ${b.count}`)} />
                </div>
                <div>
                  <h3 class="sub">{t('stats.memory.difficulty')}</h3>
                  <BarChart label={t('stats.memory.difficulty')} height={110} bars={bars(c.difficulty, bucket, (b) => b.count, (b) => `${bucket(b)}: ${b.count}`)} />
                </div>
              </div>
            {/if}
          </div>
        {/snippet}
      </Tabs>
    </SettingsSection>
  {/if}

  {#if overview && attrs.length > 0}
    <SettingsSection title={t('stats.breakdown.heading')} icon={Flag} description={t('stats.breakdown.intro')}>
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
        <LoadState state="loading" />
      {:else if breakdown.rows.length === 0}
        <EmptyState size="sm" title={t('stats.breakdown.empty')} />
      {:else}
        <div class="table-wrap">
          <table>
            <thead>
              <tr>
                <th scope="col">{t('stats.breakdown.value')}</th>
                <th scope="col" class="num">{t('stats.breakdown.cards')}</th>
                <th scope="col" class="bar-col">{t('stats.breakdown.settled')}</th>
                <th scope="col" class="num">{t('stats.breakdown.retention')}</th>
                <th scope="col" class="num">{t('stats.breakdown.due_today')}</th>
                <th scope="col" class="num">{t('stats.breakdown.due_week')}</th>
              </tr>
            </thead>
            <tbody>
              {#each breakdown.rows as row (row.option_id ?? 'none')}
                <tr>
                  <th scope="row">
                    {#if row.label}<Chip size="sm" tone="accent" label={row.label} />{:else}<span class="muted">{t('stats.breakdown.none_value')}</span>{/if}
                  </th>
                  <td class="num">{row.states.total}</td>
                  <td class="bar-col">
                    <div class="inline-bar">
                      <ProgressBar size="sm" tone="success" label={`${t('stats.breakdown.settled')}: ${pct100(row.report.settled_pct)}`} value={row.report.settled_pct} />
                      <span>{pct100(row.report.settled_pct)}</span>
                    </div>
                  </td>
                  <td class="num">{pct(row.true_retention)}</td>
                  <td class="num">{row.due_today}</td>
                  <td class="num">{row.due_week}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    </SettingsSection>
  {/if}

  {#if leeches}
    <SettingsSection title={t('stats.leeches.heading')} icon={TriangleAlert} description={t('stats.leeches.intro')}>
      {#snippet actions()}
        {#if leeches && leeches.total > 0}
          <Button size="sm" onclick={() => void applyToLeeches('suspend')}>{t('stats.leeches.suspend')}</Button>
          <Button size="sm" variant="ghost" onclick={() => void applyToLeeches('tag')}>{t('stats.leeches.tag')}</Button>
        {/if}
      {/snippet}
      {#if leeches.total === 0}
        <EmptyState size="sm" icon={TriangleAlert} title={t('stats.leeches.none')} />
      {:else}
        <p class="lead">{t('stats.leeches.count', { n: leeches.total, threshold: leeches.threshold })}</p>
        <ul class="leeches">
          {#each leeches.items.slice(0, 8) as l (l.card_id)}
            <li><span class="leech-text">{l.sort_field}</span><Chip size="sm" tone="danger" label={t('stats.leeches.lapses_n', { n: l.lapses })} /></li>
          {/each}
        </ul>
        {#if leeches.items.length > 8}<p class="muted">{t('stats.leeches.more', { n: leeches.total - 8 })}</p>{/if}
      {/if}
      {#if applied}
        <p class="muted status" role="status">{t('stats.leeches.applied', { changed: applied.changed, skipped: applied.skipped })}</p>
      {/if}
    </SettingsSection>
  {/if}

  {#if reports && reports.items.length > 0}
    <SettingsSection title={t('stats.report.heading')} icon={GraduationCap} description={t('stats.report.intro')}>
      <div class="table-wrap">
        <table>
          <thead>
            <tr>
              <th scope="col">{t('stats.report.deck')}</th>
              <th scope="col" class="bar-col">{t('stats.report.settled')}</th>
              <th scope="col" class="num">{t('stats.report.studied')}</th>
              <th scope="col" class="num">{t('stats.report.recall')}</th>
              <th scope="col" class="num">{t('stats.report.due')}</th>
            </tr>
          </thead>
          <tbody>
            {#each reports.items as dr (dr.deck_id)}
              <tr>
                <th scope="row" class="deck">{dr.path}</th>
                <td class="bar-col">
                  <div class="inline-bar">
                    <ProgressBar size="sm" tone="success" label={`${dr.path}: ${pct100(dr.report.settled_pct)}`} value={dr.report.settled_pct} />
                    <span>{pct100(dr.report.settled_pct)}</span>
                  </div>
                </td>
                <td class="num">{pct100(dr.report.studied_pct)}</td>
                <td class="num">{dr.report.mean_recall === null ? '–' : pct(dr.report.mean_recall)}</td>
                <td class="num">{num(dr.due_today, 0)}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </div>
    </SettingsSection>
  {/if}
</SettingsPage>

<style>
  :global(.filters) {
    position: relative;
  }
  .filter-row {
    display: grid;
    grid-template-columns: minmax(10rem, 1fr) auto minmax(12rem, 1.3fr) auto;
    gap: var(--space-4);
    align-items: end;
  }
  .f-range {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }
  .f-label {
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
    color: var(--text-1);
  }
  .tiles {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: var(--space-4);
  }
  .cols {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: var(--space-6);
  }
  .cols > :global(.fc-settings-section > .fc-card) {
    height: 100%;
  }
  .cols.tight {
    gap: var(--space-6);
  }
  .legend {
    margin: var(--space-4) 0 0;
    display: flex;
    flex-direction: column;
  }
  .lg-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: var(--space-2) 0;
    font-size: var(--text-sm);
    border-bottom: 1px solid var(--border);
  }
  .lg-row dt {
    display: flex;
    align-items: center;
    gap: var(--space-2);
  }
  .lg-row dd {
    margin: 0;
    font-variant-numeric: tabular-nums;
    font-weight: var(--weight-medium);
  }
  .lg-row.quiet {
    color: var(--text-2);
  }
  .lg-row.total {
    border-bottom: none;
    font-weight: var(--weight-semibold);
  }
  .dot {
    width: 10px;
    height: 10px;
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
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  .rates li {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: var(--space-1) var(--space-3);
  }
  .rates li :global(.fc-progress) {
    grid-column: 1 / -1;
  }
  .rate-head {
    display: contents;
  }
  .rate-name {
    font-size: var(--text-sm);
  }
  .rate-val {
    font-size: var(--text-sm);
    font-weight: var(--weight-semibold);
    font-variant-numeric: tabular-nums;
    text-align: end;
  }
  .rate-count {
    grid-column: 1 / -1;
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .rates li.all {
    padding-top: var(--space-3);
    border-top: 1px solid var(--border);
  }
  .rates li.all .rate-name {
    font-weight: var(--weight-semibold);
  }
  .tab-panel {
    padding-top: var(--space-4);
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
  }
  .sub {
    margin: 0 0 var(--space-3);
    font-size: var(--text-sm);
    font-weight: var(--weight-semibold);
  }
  .muted {
    margin: 0;
    color: var(--text-2);
    font-size: var(--text-sm);
  }
  .lead {
    margin: 0 0 var(--space-3);
    font-size: var(--text-sm);
  }
  .status {
    margin-top: var(--space-3);
  }
  .slowest,
  .leeches {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .slowest li,
  .leeches li {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-2) 0;
    border-bottom: 1px solid var(--border);
    font-size: var(--text-sm);
  }
  .leeches li:last-child,
  .slowest li:last-child {
    border-bottom: none;
  }
  .leech-text {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  code {
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .table-wrap {
    overflow-x: auto;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: var(--text-sm);
  }
  th,
  td {
    text-align: start;
    padding: var(--space-3) var(--space-3);
    border-bottom: 1px solid var(--border);
    white-space: nowrap;
  }
  th:first-child,
  td:first-child {
    padding-inline-start: 0;
  }
  th:last-child,
  td:last-child {
    padding-inline-end: 0;
  }
  thead th {
    font-size: var(--text-xs);
    font-weight: var(--weight-semibold);
    color: var(--text-2);
    padding-top: 0;
  }
  tbody tr:last-child th,
  tbody tr:last-child td {
    border-bottom: none;
  }
  tbody th {
    font-weight: var(--weight-medium);
  }
  .deck {
    white-space: normal;
    overflow-wrap: anywhere;
  }
  .num {
    text-align: end;
    font-variant-numeric: tabular-nums;
  }
  .bar-col {
    width: 32%;
    min-width: 9rem;
  }
  .inline-bar {
    display: grid;
    grid-template-columns: minmax(4rem, 1fr) 3rem;
    align-items: center;
    gap: var(--space-2);
    font-variant-numeric: tabular-nums;
  }
  .inline-bar span {
    text-align: end;
    color: var(--text-2);
  }
  @media (max-width: 1100px) {
    .tiles {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
    .filter-row {
      grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
    }
  }
  @media (max-width: 899.98px) {
    .cols {
      grid-template-columns: minmax(0, 1fr);
    }
  }
</style>
