<!--
  Calm single-series line chart (inline SVG, no chart library), the companion of BarChart:
  draws numbers the backend computed and only scales them. Recessive grid (top and middle
  dashed lines with mono value labels, a solid baseline), a 2px accent line over a faint area,
  the last point marked (today), a hover / focus readout per point, mono x labels (first /
  last) and a screen-reader table of every point. The y axis starts at 0 so growth is honest.
-->
<script lang="ts">
  import type { Bar } from './format';

  interface Props {
    points: Bar[];
    label: string;
    height?: number;
    formatMax?: (n: number) => string;
  }

  let { points, label, height = 96, formatMax = (n) => String(Math.round(n)) }: Props = $props();

  const W = 300;
  let hover = $state<number | null>(null);
  const max = $derived(Math.max(1, ...points.map((p) => p.value)));
  const x = (i: number) => (points.length <= 1 ? W : (i / (points.length - 1)) * W);
  const y = (v: number) => height - (v / max) * (height - 4) - 1;
  /** SVG coordinates (not shown numbers), one decimal. */
  const r1 = (n: number) => Math.round(n * 10) / 10;
  const line = $derived(points.map((p, i) => `${r1(x(i))},${r1(y(p.value))}`).join(' '));
  const area = $derived(points.length ? `0,${height} ${line} ${W},${height}` : '');
  const last = $derived(points.length ? points.length - 1 : null);
  const tip = $derived(hover === null ? null : { i: hover, p: points[hover]! });

  function onMove(e: PointerEvent) {
    const r = (e.currentTarget as SVGElement).getBoundingClientRect();
    if (!points.length || r.width === 0) return;
    hover = Math.max(0, Math.min(points.length - 1, Math.round(((e.clientX - r.left) / r.width) * (points.length - 1))));
  }
</script>

<figure class="chart" style:--h="{height}px">
  <div class="plot" role="img" aria-label={label}>
    <span class="grid top" aria-hidden="true"><span class="yl">{formatMax(max)}</span></span>
    <span class="grid mid" aria-hidden="true"><span class="yl">{formatMax(max / 2)}</span></span>
    <svg
      viewBox="0 0 {W} {height}"
      preserveAspectRatio="none"
      aria-hidden="true"
      onpointermove={onMove}
      onpointerleave={() => (hover = null)}
    >
      {#if points.length}
        <polygon class="area" points={area} />
        <polyline class="line" points={line} vector-effect="non-scaling-stroke" />
        {#if tip}
          <line class="cursor" x1={x(tip.i)} x2={x(tip.i)} y1="0" y2={height} vector-effect="non-scaling-stroke" />
        {/if}
      {/if}
    </svg>
    {#if last !== null}
      <span class="dot" aria-hidden="true" style:left="{(x(last) / W) * 100}%" style:top="{y(points[last]!.value)}px"></span>
    {/if}
    {#if tip}
      <span
        class="tip"
        aria-hidden="true"
        class:end={tip.i > points.length * 0.66}
        class:start={tip.i < points.length * 0.34}
        style:left="{(x(tip.i) / W) * 100}%">{tip.p.title}</span
      >
    {/if}
  </div>
  {#if points.length}
    <figcaption aria-hidden="true">
      <span>{points[0]!.label}</span>{#if points.length > 1}<span>{points[points.length - 1]!.label}</span>{/if}
    </figcaption>
  {/if}
  <table class="sr-only">
    <caption>{label}</caption>
    <tbody>
      {#each points as p, i (i)}<tr><td>{p.title}</td></tr>{/each}
    </tbody>
  </table>
</figure>

<style>
  .chart {
    --line: light-dark(var(--accent-text), var(--accent));
    position: relative;
    margin: 0;
    padding-top: var(--space-3);
    min-width: 0;
  }
  .plot {
    position: relative;
    height: var(--h);
    border-bottom: 1px solid var(--border-strong);
  }
  svg {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    overflow: visible;
  }
  .area {
    fill: color-mix(in srgb, var(--accent) 12%, transparent);
  }
  .line {
    fill: none;
    stroke: var(--line);
    stroke-width: 2;
    stroke-linejoin: round;
    stroke-linecap: round;
  }
  .cursor {
    stroke: var(--border-strong);
    stroke-width: 1;
  }
  .dot {
    position: absolute;
    width: 8px;
    height: 8px;
    margin: -4px 0 0 -4px;
    border-radius: 50%;
    background: var(--line);
    box-shadow: 0 0 0 2px var(--surface-1);
    pointer-events: none;
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
    left: 0;
    bottom: 1px;
    padding-right: 4px;
    background: var(--surface-1);
    font-family: var(--font-mono);
    font-size: var(--text-2xs);
    color: var(--text-2);
    font-variant-numeric: tabular-nums;
    line-height: 1.2;
  }
  .tip {
    position: absolute;
    top: -6px;
    transform: translate(-50%, -100%);
    padding: 2px 6px;
    border-radius: var(--radius-sm);
    background: var(--surface-3, var(--surface-2));
    border: 1px solid var(--border);
    font-size: var(--text-xs);
    color: var(--text-1);
    white-space: nowrap;
    pointer-events: none;
    z-index: 1;
  }
  .tip.start {
    transform: translate(0, -100%);
  }
  .tip.end {
    transform: translate(-100%, -100%);
  }
  figcaption {
    display: flex;
    justify-content: space-between;
    margin-top: 4px;
    font-family: var(--font-mono);
    font-size: var(--text-2xs);
    color: var(--text-2);
  }
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
    white-space: nowrap;
  }
</style>
