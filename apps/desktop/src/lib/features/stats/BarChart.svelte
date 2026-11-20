<!--
  Calm bar chart (HTML/CSS, no chart library). Draws numbers the backend computed; only
  scales them to the tallest bar. The plot is an accessible image (`label`), every bar has
  its `title` as a tooltip; the axis shows the max value and first / middle / last labels.
  Bars keep a sane width whether there are 2 or 365 of them.
-->
<script lang="ts">
  import type { Bar } from './format';

  interface Props {
    bars: Bar[];
    label: string;
    height?: number;
    /** Index of a bar to emphasise (e.g. today). */
    highlight?: number | null;
    /** Max-value label of the y axis (default: the number). */
    formatMax?: (n: number) => string;
  }

  let { bars, label, height = 140, highlight = null, formatMax = (n) => String(Math.round(n)) }: Props = $props();

  const max = $derived(Math.max(1, ...bars.map((b) => b.value)));
  const ticks = $derived(
    bars.length === 0 ? [] : [...new Set([0, Math.floor((bars.length - 1) / 2), bars.length - 1])].map((i) => ({ i, label: bars[i]!.label })),
  );
  const dense = $derived(bars.length > 60);
</script>

<figure class="chart">
  <span class="ymax" aria-hidden="true">{formatMax(max)}</span>
  <div class="plot" class:dense role="img" aria-label={label} style:height="{height}px">
    <span class="grid top" aria-hidden="true"></span>
    <span class="grid mid" aria-hidden="true"></span>
    {#each bars as b, i (i)}
      <span class="slot" title={b.title}>
        <span class="bar" class:hi={i === highlight} class:zero={b.value <= 0} style:height="{b.value > 0 ? Math.max(1.5, (b.value / max) * 100) : 0}%"></span>
      </span>
    {/each}
  </div>
  {#if ticks.length > 0}
    <figcaption aria-hidden="true" class:single={ticks.length === 1}>
      {#each ticks as tk (tk.i)}<span>{tk.label}</span>{/each}
    </figcaption>
  {/if}
</figure>

<style>
  .chart {
    position: relative;
    margin: 0;
    padding-top: var(--space-4);
    min-width: 0;
  }
  .ymax {
    position: absolute;
    top: 0;
    right: 0;
    font-size: var(--text-xs);
    color: var(--text-2);
    font-variant-numeric: tabular-nums;
  }
  .plot {
    position: relative;
    display: flex;
    align-items: flex-end;
    gap: 4px;
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
    max-width: 32px;
    border-radius: 3px 3px 0 0;
    background: color-mix(in srgb, var(--accent) 70%, var(--surface-1));
    transition: background var(--motion-fast) var(--ease);
  }
  .slot:hover .bar,
  .bar.hi {
    background: var(--accent);
  }
  figcaption {
    display: flex;
    justify-content: space-between;
    margin-top: var(--space-2);
    font-size: var(--text-xs);
    color: var(--text-2);
    font-variant-numeric: tabular-nums;
  }
  figcaption.single {
    justify-content: center;
  }
</style>
