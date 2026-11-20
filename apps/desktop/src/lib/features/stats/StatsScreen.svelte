<script lang="ts">
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
  import { t } from '$lib/i18n/index.svelte';
  import type { ScreenCtx } from '../types';
  import BarChart from './BarChart.svelte';
  import { minutes, num, pct, pct100, seconds, tzOffset, type Bar } from './format';

  let { ctx }: { ctx: ScreenCtx } = $props();

  const RANGES = ['30', '90', '365', 'all'] as const;

  let deckId = $state('');
  let query = $state('');
  let range = $state<(typeof RANGES)[number]>('30');
  let overview = $state<StatsOverview | null>(null);
  let leeches = $state<LeechList | null>(null);
  let applied = $state<LeechActionResult | null>(null);
  let reports = $state<DeckReports | null>(null);
  let attrs = $state<AttributeDefDto[]>([]);
  let attrId = $state('');
  let breakdown = $state<StatsBreakdown | null>(null);
  let trend = $state<'weekly' | 'monthly'>('weekly');

  const flatten = (nodes: DeckTreeNode[]): { id: string; path: string }[] =>
    nodes.flatMap((n) => [{ id: n.deck.id, path: n.deck.path }, ...flatten(n.children)]);
  const decks = $derived(flatten(ctx.decks));

  const scope = (): StatsScope => ({ deck_id: deckId || null, tag: null, query: query.trim() || null });
  const days = () => (range === 'all' ? null : Number(range));

  async function load() {
    const tz = tzOffset();
    await ctx.guarded(async () => {
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

  onMount(() => {
    void load();
    const unsub = api.on(['study_', 'cards_', 'undone', 'redone', 'collection_replaced'], () => void load());
    return () => void unsub.then((u) => u());
  });
</script>

<h2>{t('stats.heading')}</h2>

<form class="row" onsubmit={(e) => { e.preventDefault(); void load(); }}>
  <label>{t('stats.scope.deck')}
    <select bind:value={deckId}>
      <option value="">{t('stats.scope.all_decks')}</option>
      {#each decks as d (d.id)}<option value={d.id}>{d.path}</option>{/each}
    </select></label>
  <label>{t('stats.scope.query')}
    <input bind:value={query} placeholder={t('stats.scope.query_hint')} /></label>
  <label>{t('stats.scope.range')}
    <select bind:value={range}>
      {#each RANGES as r (r)}<option value={r}>{t(`stats.range.${r}`)}</option>{/each}
    </select></label>
  <button type="submit">{t('stats.scope.apply')}</button>
</form>

{#if overview}
  {@const r = overview.reviews}
  {@const c = overview.cards}
  <section class="tiles" aria-label={t('stats.heading')}>
    <div><h3>{t('stats.streak.label')}</h3>
      <p class="big">{t('stats.streak.days', { n: r.streak.current })}</p>
      <p class="muted">{t('stats.streak.longest', { n: r.streak.longest })}</p></div>
    <div><h3>{t('stats.reviews.label')}</h3>
      <p class="big">{r.totals.reviews}</p>
      <p class="muted">{t('stats.reviews.detail', { days: r.totals.days_studied, total: r.totals.days_in_range })}</p>
      {#if r.totals.imported > 0}<p class="muted">{t('stats.imported', { n: r.totals.imported })}</p>{/if}</div>
    <div><h3>{t('stats.time.label')}</h3>
      <p class="big">{t('stats.time.minutes', { n: minutes(r.totals.time_ms) })}</p></div>
    <div><h3>{t('stats.retention.label')}</h3>
      <p class="big">{r.true_retention === null ? t('stats.retention.none') : pct(r.true_retention, 1)}</p>
      <p class="muted">{t('stats.retention.target', { target: pct(overview.target_retention) })}</p></div>
  </section>

  {#if r.totals.reviews === 0}<p class="muted">{t('stats.empty')}</p>{/if}

  <section>
    <h3>{t('stats.heatmap.heading')}</h3>
    <BarChart label={t('stats.heatmap.heading')}
      bars={bars(r.heatmap, (h) => h.date, (h) => h.reviews, (h) => t('stats.heatmap.day', { date: h.date, n: h.reviews }))} />
  </section>

  <section>
    <h3>{t('stats.forecast.heading')}</h3>
    <BarChart label={t('stats.forecast.heading')}
      bars={bars(c.forecast, (f) => f.date, (f) => f.due, (f) => `${f.date}: ${f.due}`)} />
    {#if c.overdue > 0}<p class="muted">{t('stats.forecast.overdue', { n: c.overdue })}</p>{/if}
  </section>

  <div class="cols">
    <section>
      <h3>{t('stats.states.heading')}</h3>
      <table>
        <tbody>
          {#each ['new', 'learning', 'relearning', 'young', 'mature', 'suspended', 'buried', 'total'] as const as k (k)}
            <tr><th scope="row">{t(`stats.states.${k}`)}</th><td>{c.states[k]}</td></tr>
          {/each}
        </tbody>
      </table>
    </section>
    <section>
      <h3>{t('stats.success.heading')}</h3>
      <table>
        <thead><tr><th>{t('stats.success.kind')}</th><th>{t('stats.success.answers')}</th><th>{t('stats.success.rate')}</th></tr></thead>
        <tbody>
          {#each ['learn', 'young', 'mature', 'relearn', 'all'] as const as k (k)}
            <tr><th scope="row">{t(`stats.success.${k}`)}</th><td>{r.success[k].count}</td><td>{pct(r.success[k].rate)}</td></tr>
          {/each}
        </tbody>
      </table>
    </section>
  </div>

  <section>
    <h3>{t('stats.trend.heading')}
      <select bind:value={trend} aria-label={t('stats.trend.heading')}>
        <option value="weekly">{t('stats.trend.weekly')}</option>
        <option value="monthly">{t('stats.trend.monthly')}</option>
      </select></h3>
    <BarChart label={t('stats.trend.heading')}
      bars={bars(r[trend], (p) => p.start_date, (p) => p.reviews, (p) => `${p.start_date}: ${p.reviews} · ${pct(p.rate)}`)} />
  </section>

  <details>
    <summary>{t('stats.hours.heading')}</summary>
    <BarChart label={t('stats.hours.heading')}
      bars={bars(r.hours, (h) => `${h.hour}:00`, (h) => (h.stat.rate ?? 0) * 100, (h) => `${h.hour}:00 · ${h.stat.count} · ${pct(h.stat.rate)}`)} />
  </details>

  <details>
    <summary>{t('stats.times.heading')}</summary>
    <p>{t('stats.times.mean', { s: seconds(r.answer_times.mean_ms) })}</p>
    <BarChart label={t('stats.times.heading')}
      bars={bars(r.answer_times.buckets, (b) => b.up_to_ms === null ? t('stats.times.over', { s: 60 }) : t('stats.times.up_to', { s: b.up_to_ms / 1000 }), (b) => b.count,
        (b) => `${b.up_to_ms === null ? t('stats.times.over', { s: 60 }) : t('stats.times.up_to', { s: b.up_to_ms / 1000 })}: ${b.count}`)} />
    {#if r.answer_times.slowest.length > 0}
      <h4>{t('stats.times.slowest')}</h4>
      <ul>{#each r.answer_times.slowest as s (s.card_id)}<li><code>{s.card_id.slice(-8)}</code> {t('stats.times.seconds', { s: seconds(s.duration_ms) })}</li>{/each}</ul>
    {/if}
  </details>

  <details>
    <summary>{t('stats.memory.heading')}</summary>
    <h4>{t('stats.memory.stability')}</h4>
    <BarChart label={t('stats.memory.stability')}
      bars={bars(c.stability, (b) => b.to === null ? `> ${b.from}` : `≤ ${b.to}`, (b) => b.count, (b) => `${b.to === null ? `> ${b.from}` : `≤ ${b.to}`}: ${b.count}`)} />
    <h4>{t('stats.memory.difficulty')}</h4>
    <BarChart label={t('stats.memory.difficulty')}
      bars={bars(c.difficulty, (b) => b.to === null ? `> ${b.from}` : `≤ ${b.to}`, (b) => b.count, (b) => `${b.to === null ? `> ${b.from}` : `≤ ${b.to}`}: ${b.count}`)} />
  </details>
{/if}

{#if attrs.length > 0}
  <section>
    <h3>{t('stats.breakdown.heading')}</h3>
    <select bind:value={attrId} aria-label={t('stats.breakdown.pick')} onchange={() => void loadBreakdown()}>
      <option value="">{t('stats.breakdown.pick')}</option>
      {#each attrs as a (a.id)}<option value={a.id}>{a.name}</option>{/each}
    </select>
    {#if breakdown}
      <table>
        <thead><tr>
          <th>{t('stats.breakdown.value')}</th><th>{t('stats.breakdown.cards')}</th><th>{t('stats.breakdown.settled')}</th>
          <th>{t('stats.breakdown.retention')}</th><th>{t('stats.breakdown.due_today')}</th><th>{t('stats.breakdown.due_week')}</th>
        </tr></thead>
        <tbody>
          {#each breakdown.rows as row (row.option_id ?? 'none')}
            <tr>
              <th scope="row">{row.label ?? t('stats.breakdown.none_value')}</th>
              <td>{row.states.total}</td><td>{pct100(row.report.settled_pct)}</td>
              <td>{pct(row.true_retention)}</td><td>{row.due_today}</td><td>{row.due_week}</td>
            </tr>
          {/each}
        </tbody>
      </table>
    {/if}
  </section>
{/if}

{#if leeches}
  <section>
    <h3>{t('stats.leeches.heading')}</h3>
    {#if leeches.total === 0}
      <p class="muted">{t('stats.leeches.none')}</p>
    {:else}
      <p>{t('stats.leeches.count', { n: leeches.total, threshold: leeches.threshold })}</p>
      <table>
        <thead><tr><th>{t('stats.leeches.card')}</th><th>{t('stats.leeches.lapses')}</th></tr></thead>
        <tbody>{#each leeches.items as l (l.card_id)}<tr><td>{l.sort_field}</td><td>{l.lapses}</td></tr>{/each}</tbody>
      </table>
      <div class="row">
        <button onclick={() => void applyToLeeches('suspend')}>{t('stats.leeches.suspend')}</button>
        <button onclick={() => void applyToLeeches('tag')}>{t('stats.leeches.tag')}</button>
      </div>
      {#if applied}
        <p class="muted" role="status">{t('stats.leeches.applied', { changed: applied.changed, skipped: applied.skipped })}</p>
      {/if}
    {/if}
  </section>
{/if}

{#if reports && reports.items.length > 0}
  <details>
    <summary>{t('stats.report.heading')}</summary>
    <table>
      <thead><tr>
        <th>{t('stats.report.deck')}</th><th>{t('stats.report.settled')}</th><th>{t('stats.report.studied')}</th>
        <th>{t('stats.report.recall')}</th><th>{t('stats.report.due')}</th>
      </tr></thead>
      <tbody>
        {#each reports.items as d (d.deck_id)}
          <tr>
            <th scope="row">{d.path}</th>
            <td><meter min="0" max="100" value={d.report.settled_pct}></meter> {pct100(d.report.settled_pct)}</td>
            <td>{pct100(d.report.studied_pct)}</td>
            <td>{d.report.mean_recall === null ? '–' : pct(d.report.mean_recall)}</td>
            <td>{num(d.due_today, 0)}</td>
          </tr>
        {/each}
      </tbody>
    </table>
  </details>
{/if}

<style>
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: var(--fc-space-3);
    align-items: end;
  }
  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(10rem, 1fr));
    gap: var(--fc-space-4);
    margin: var(--fc-space-4) 0;
  }
  .tiles h3 {
    margin: 0;
    font-size: 0.9em;
  }
  .big {
    font-size: 1.6em;
    margin: var(--fc-space-1) 0;
  }
  .cols {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(14rem, 1fr));
    gap: var(--fc-space-6);
  }
  section,
  details {
    margin: var(--fc-space-4) 0;
  }
  table {
    border-collapse: collapse;
  }
  th,
  td {
    text-align: left;
    padding: var(--fc-space-1) var(--fc-space-3) var(--fc-space-1) 0;
  }
</style>
