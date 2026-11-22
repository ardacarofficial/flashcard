<!--
  Calm single-series bar chart (HTML/CSS, no chart library). Draws numbers the backend
  computed; only scales them to the tallest bar. Recessive grid (baseline, middle and top
  lines with mono value labels), accent bars with rounded tops and a 2px gap, a hover / focus
  tooltip per bar, mono x labels (first / middle / last, or every bar with `labels="all"`),
  optional direct value labels over each bar (`values`), an emphasised bar (`highlight`,
  e.g. today: stronger colour + bold label) and a called-out bar (`peak`: a text marker, so
  colour is never the only cue), and a screen-reader table of every bar.
-->
<script lang="ts">
  import type { Bar } from './format';

  interface Props {
    bars: Bar[];
    label: string;
    height?: number;
    /** Index of a bar to emphasise (e.g. today). */
    highlight?: number | null;
    /** Value labels of the y axis (default: the rounded number). */
    formatMax?: (n: number) => string;
    /** X labels: first / middle / last (`ends`, default) or under every bar (`all`, ≤ 14 bars). */
    labels?: 'ends' | 'all';
    /** Direct value label over every bar (few bars only). */
    values?: boolean;
    /** Index of a bar to call out (e.g. the busiest day), with `peakLabel` as its text marker. */
    peak?: number | null;
    peakLabel?: string;
  }

  let {
    bars,
    label,
    height = 132,
    highlight = null,
    formatMax = (n) => String(Math.round(n)),
    labels = 'ends',
    values = false,
    peak = null,
    peakLabel = '',
  }: Props = $props();

  let hover = $state<number | null>(null);
  const max = $derived(Math.max(1, ...bars.map((b) => b.value)));
  const ticks = $derived(
    bars.length === 0 ? [] : [...new Set([0, Math.floor((bars.length - 1) / 2), bars.length - 1])].map((i) => ({ i, label: bars[i]!.label })),
  );
  const pctOf = (v: number) => (v > 0 ? Math.max(2, (v / max) * 100) : 0);
  const tip = $derived(hover === null ? null : { i: hover, bar: bars[hover]! });
</script>

<figure class="chart" class:roomy={values || peak !== null} class:direct={values} style:--h="{height}px">
  <div class="plot" class:dense={bars.length > 45} role="img" aria-label={label} onmouseleave={() => (hover = null)}>
    <span class="grid top" aria-hidden="true"><span class="yl">{formatMax(max)}</span></span>
    <span class="grid mid" aria-hidden="true"><span class="yl">{formatMax(max / 2)}</span></span>
    {#each bars as b, i (i)}
      <span class="slot" aria-hidden="true" onmouseenter={() => (hover = i)}>
        <span class="bar" class:hi={i === highlight} class:on={i === hover} class:peak={i === peak} style:height="{pctOf(b.value)}%">
          {#if values || i === peak}
            <span class="val" class:peak-val={i === peak}>{#if i === peak && peakLabel}<span class="mark">▲ {peakLabel}</span>{/if}{formatMax(b.value)}</span>
          {/if}
        </span>
      </span>
    {/each}
    {#if tip}
      <span
        class="tip"
        aria-hidden="true"
        class:end={tip.i > bars.length * 0.66}
        class:start={tip.i < bars.length * 0.34}
        style:left="{((tip.i + 0.5) / bars.length) * 100}%"
        style:bottom="calc({pctOf(tip.bar.value)}% + 6px)">{tip.bar.title}</span
      >
    {/if}
  </div>
  {#if labels === 'all' && bars.length > 0}
    <figcaption aria-hidden="true" class="all">
      {#each bars as b, i (i)}<span class:hi={i === highlight}>{b.label}</span>{/each}
    </figcaption>
  {:else if ticks.length > 0}
    <figcaption aria-hidden="true" class:single={ticks.length === 1}>
      {#each ticks as tk (tk.i)}<span>{tk.label}</span>{/each}
    </figcaption>
  {/if}
  <table class="sr-only">
    <caption>{label}</caption>
    <tbody>
      {#each bars as b, i (i)}<tr><td>{b.title}</td></tr>{/each}
    </tbody>
  </table>
</figure>

<style>
  .chart {
    --bar: light-dark(color-mix(in srgb, var(--accent) 40%, var(--accent-text)), color-mix(in srgb, var(--accent) 78%, var(--surface-1)));
    --bar-hi: light-dark(var(--accent-text), var(--accent));
    position: relative;
    margin: 0;
    padding-top: var(--space-3);
    min-width: 0;
  }
  .chart.roomy {
    padding-top: 30px;
  }
  /* Direct labels make the y-axis values redundant. */
  .chart.direct .yl {
    display: none;
  }
  .plot {
    position: relative;
    display: flex;
    align-items: flex-end;
    gap: 2px;
    height: var(--h);
    border-bottom: 1px solid var(--border-strong);
  }
  .plot.dense {
    gap: 1px;
  }
  .grid {
    position: absolute;
    left: 0;
    right: 0;
    border-top: 1px dashed var(--border);
    pointer-events: none;
  }
  .grid.top {
    top: 0;
  }
  .grid.mid {
    top: 50%;
  }
  .yl {
    position: absolute;
    right: 0;
    bottom: 1px;
    padding-left: 4px;
    background: var(--surface-1);
    font-family: var(--font-mono);
    font-size: var(--text-2xs);
    color: var(--text-2);
    font-variant-numeric: tabular-nums;
    line-height: 1.2;
  }
  .slot {
    position: relative;
    flex: 1 1 0;
    min-width: 0;
    height: 100%;
    display: flex;
    align-items: flex-end;
    justify-content: center;
  }
  .bar {
    width: 100%;
    max-width: 28px;
    border-radius: 3px 3px 0 0;
    background: var(--bar);
    transition: background var(--motion-fast) var(--ease);
  }
  .bar {
    position: relative;
  }
  .bar.hi,
  .bar.on {
    background: var(--bar-hi);
  }
  .bar.peak {
    outline: 1px solid var(--text-1);
    outline-offset: 1px;
  }
  .val {
    position: absolute;
    left: 50%;
    bottom: calc(100% + 2px);
    transform: translateX(-50%);
    display: flex;
    flex-direction: column;
    align-items: center;
    font-family: var(--font-mono);
    font-size: var(--text-2xs);
    color: var(--text-2);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
    line-height: 1.2;
    /* Drawn above the bar on the panel: its own panel background keeps it legible. */
    background: var(--surface-1);
    padding: 0 2px;
    border-radius: 2px;
  }
  .val.peak-val {
    color: var(--text-1);
    font-weight: var(--weight-semibold);
  }
  .mark {
    font-family: var(--font-sans, inherit);
    font-weight: var(--weight-medium);
    color: var(--accent-text);
  }
  .tip {
    position: absolute;
    z-index: 2;
    transform: translateX(-50%);
    padding: 3px 7px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-control);
    background: var(--surface-2);
    box-shadow: var(--shadow-float);
    color: var(--text-1);
    font-family: var(--font-mono);
    font-size: var(--text-2xs);
    white-space: nowrap;
    pointer-events: none;
  }
  .tip.start {
    transform: translateX(-12px);
  }
  .tip.end {
    transform: translateX(calc(-100% + 12px));
  }
  figcaption {
    display: flex;
    justify-content: space-between;
    margin-top: 6px;
    font-family: var(--font-mono);
    font-size: var(--text-2xs);
    color: var(--text-2);
    font-variant-numeric: tabular-nums;
  }
  figcaption.single {
    justify-content: center;
  }
  figcaption.all {
    gap: 2px;
  }
  figcaption.all span {
    flex: 1 1 0;
    min-width: 0;
    text-align: center;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  figcaption.all span.hi {
    color: var(--text-1);
    font-weight: var(--weight-semibold);
  }
</style>
