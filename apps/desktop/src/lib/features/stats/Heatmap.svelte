<!--
  Calendar heatmap of the review history the backend computed (one cell per day, weeks as
  columns, Monday on top). Only lays the days out and steps the colour by the busiest day;
  every cell has its date and count as a tooltip, and the legend says what the steps mean.
-->
<script lang="ts">
  import { i18n, t } from '$lib/i18n/index.svelte';
  import { level, shortDate, weekdayMon0 } from './format';

  interface Props {
    days: { date: string; reviews: number }[];
    label: string;
  }

  let { days, label }: Props = $props();

  const GAP = 3;
  /** Container width: few weeks get bigger cells (14–26 px), a year scrolls at 14 px. */
  let avail = $state(0);
  let box = $state<HTMLDivElement | null>(null);
  $effect(() => {
    if (!box) return;
    avail = box.clientWidth;
    if (typeof ResizeObserver === 'undefined') return;
    const ro = new ResizeObserver(() => (avail = box?.clientWidth ?? 0));
    ro.observe(box);
    return () => ro.disconnect();
  });
  const max = $derived(Math.max(0, ...days.map((d) => d.reviews)));
  const offset = $derived(days.length ? weekdayMon0(days[0]!.date) : 0);
  const cells = $derived(
    days.map((d, i) => {
      const slot = i + offset;
      return { ...d, col: Math.floor(slot / 7), row: slot % 7, lv: level(d.reviews, max) };
    }),
  );
  const cols = $derived(Math.max(1, Math.ceil((days.length + offset) / 7)));
  /** Column pitch fills the card (min 17 px; a year scrolls); row pitch stays squarish (17–40 px). */
  const STEP = $derived(Math.max(17, (avail - 32 + GAP) / cols));
  const STEP_Y = $derived(Math.min(40, STEP));
  const CELL = $derived(STEP - GAP);
  const CELL_Y = $derived(STEP_Y - GAP);
  const w = $derived(cols * STEP - GAP);
  const h = $derived(7 * STEP_Y - GAP);
  /** Month labels: the column where a new month starts. */
  const months = $derived(
    cells
      .filter((c, i) => i === 0 || c.date.slice(5, 7) !== cells[i - 1]!.date.slice(5, 7))
      .map((c) => ({ col: c.col, text: shortDate(c.date, i18n.locale).replace(/^\d+\.?\s*/, '') })),
  );
  const DAY_KEYS = ['mon', 'wed', 'fri'] as const;
</script>

<div class="heatmap">
  <div class="scroll" bind:this={box}>
    <div class="grid" style:--cols={cols}>
      <div class="months" aria-hidden="true" style:width="{w}px">
        {#each months as m, i (i)}<span style:left="{m.col * STEP}px">{m.text}</span>{/each}
      </div>
      <div class="body">
        <div class="days" aria-hidden="true">
          {#each DAY_KEYS as d, i (d)}<span style:top="{i * 2 * STEP_Y + (CELL_Y - 14) / 2}px">{t(`stats.weekday.${d}`)}</span>{/each}
        </div>
        <svg width={w} height={h} viewBox={`0 0 ${w} ${h}`} role="img" aria-label={t('stats.heatmap.summary', { label, days: days.length, max })}>
          {#each cells as c (c.date)}
            <rect class="l{c.lv}" x={c.col * STEP} y={c.row * STEP_Y} width={CELL} height={CELL_Y} rx="4">
              <title>{t('stats.heatmap.day', { date: shortDate(c.date, i18n.locale), n: c.reviews })}</title>
            </rect>
          {/each}
        </svg>
      </div>
    </div>
  </div>
  <div class="legend">
    <span>{t('stats.heatmap.less')}</span>
    {#each [0, 1, 2, 3, 4] as l (l)}<span class="sw l{l}" aria-hidden="true"></span>{/each}
    <span>{t('stats.heatmap.more')}</span>
  </div>
</div>

<style>
  .heatmap {
    display: flex;
    flex-direction: column;
    gap: var(--space-3);
    min-width: 0;
  }
  .scroll {
    overflow-x: auto;
    padding-bottom: var(--space-1);
  }
  .grid {
    display: inline-flex;
    flex-direction: column;
    gap: var(--space-1);
  }
  .months {
    position: relative;
    height: 16px;
    margin-left: 32px;
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .months span {
    position: absolute;
    top: 0;
    white-space: nowrap;
  }
  .body {
    display: flex;
  }
  .days {
    position: relative;
    width: 32px;
    flex: none;
    font-size: 11px;
    color: var(--text-2);
  }
  .days span {
    position: absolute;
    left: 0;
    line-height: 14px;
  }
  svg {
    display: block;
    flex: none;
  }
  .l0 {
    fill: var(--surface-2);
    background: var(--surface-2);
  }
  .l1 {
    fill: color-mix(in srgb, var(--accent) 25%, var(--surface-1));
    background: color-mix(in srgb, var(--accent) 25%, var(--surface-1));
  }
  .l2 {
    fill: color-mix(in srgb, var(--accent) 48%, var(--surface-1));
    background: color-mix(in srgb, var(--accent) 48%, var(--surface-1));
  }
  .l3 {
    fill: color-mix(in srgb, var(--accent) 72%, var(--surface-1));
    background: color-mix(in srgb, var(--accent) 72%, var(--surface-1));
  }
  .l4 {
    fill: var(--accent);
    background: var(--accent);
  }
  rect {
    stroke: color-mix(in srgb, var(--text-1) 6%, transparent);
    stroke-width: 1;
  }
  .legend {
    display: flex;
    align-items: center;
    justify-content: flex-start;
    padding-left: 32px;
    gap: var(--space-1);
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .sw {
    width: 12px;
    height: 12px;
    border-radius: 3px;
  }
  .legend span:first-child {
    margin-right: var(--space-1);
  }
  .legend span:last-child {
    margin-left: var(--space-1);
  }
</style>
