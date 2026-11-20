<script lang="ts">
  /**
   * Plain SVG bar chart. Draws numbers the backend computed; scales them to the tallest bar
   * and nothing else.
   */
  import type { Bar } from './format';

  let { bars, label, height = 96 }: { bars: Bar[]; label: string; height?: number } = $props();

  const max = $derived(Math.max(1, ...bars.map((b) => b.value)));
  const width = $derived(Math.max(1, bars.length) * 12);
</script>

<figure>
  <svg viewBox={`0 0 ${width} ${height}`} preserveAspectRatio="none" role="img" aria-label={label}>
    {#each bars as b, i (i)}
      {@const h = (b.value / max) * (height - 2)}
      <rect x={i * 12 + 1} y={height - h} width="10" height={h} rx="1"><title>{b.title}</title></rect>
    {/each}
  </svg>
  {#if bars.length > 0}
    <figcaption class="muted">
      <span>{bars.at(0)?.label}</span><span>{bars.at(-1)?.label}</span>
    </figcaption>
  {/if}
</figure>

<style>
  figure {
    margin: 0;
  }
  svg {
    width: 100%;
    height: 6rem;
    display: block;
  }
  rect {
    fill: var(--fc-accent, currentColor);
  }
  figcaption {
    display: flex;
    justify-content: space-between;
    font-size: 0.8em;
  }
</style>
