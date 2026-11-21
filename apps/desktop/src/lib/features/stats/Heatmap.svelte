<!--
  Calendar heatmap of the review history the backend computed, GitHub-contribution style:
  12 px square cells with a 2 px gap, weeks as columns (Monday on top), month and weekday
  labels in small mono, a less → more legend. Only lays the days out and steps the colour by
  the busiest day. Hover or keyboard (one tab stop, arrow keys) shows a tooltip with the date
  and count; the focused day is also announced, and the whole map has a summary label.
-->
<script lang="ts">
  import { i18n, t } from '$lib/i18n/index.svelte';
  import { level, shortDate, weekdayMon0 } from './format';

  interface Props {
    days: { date: string; reviews: number }[];
    label: string;
  }

  let { days, label }: Props = $props();

  const CELL = 12;
  const GAP = 2;
  const STEP = CELL + GAP;
  const LEFT = 28;

  const max = $derived(Math.max(0, ...days.map((d) => d.reviews)));
  const offset = $derived(days.length ? weekdayMon0(days[0]!.date) : 0);
  const cells = $derived(
    days.map((d, i) => {
      const slot = i + offset;
      return { ...d, col: Math.floor(slot / 7), row: slot % 7, lv: level(d.reviews, max) };
    }),
  );
  const cols = $derived(Math.max(1, Math.ceil((days.length + offset) / 7)));
  const w = $derived(cols * STEP - GAP);
  const h = 7 * STEP - GAP;
  /** Month labels: the column where a new month starts (skipped when it would collide). */
  const months = $derived.by(() => {
    const out: { col: number; text: string }[] = [];
    cells.forEach((c, i) => {
      if (i !== 0 && c.date.slice(5, 7) === cells[i - 1]!.date.slice(5, 7)) return;
      const col = c.row === 0 || i === 0 ? c.col : c.col + 1;
      if (out.length && col - out[out.length - 1]!.col < 3) return;
      if (col * STEP > w - 20) return;
      out.push({ col, text: shortDate(c.date, i18n.locale).replace(/^\d+\.?\s*/, '') });
    });
    return out;
  });
  const DAY_ROWS = [
    ['mon', 0],
    ['wed', 2],
    ['fri', 4],
  ] as const;

  let active = $state<number | null>(null);
  let focused = $state(false);
  const cur = $derived(active === null ? null : (cells[active] ?? null));
  const dayText = (c: { date: string; reviews: number }) => t('stats.heatmap.day', { date: shortDate(c.date, i18n.locale), n: c.reviews });

  function onkeydown(e: KeyboardEvent) {
    if (!cells.length) return;
    const step: Record<string, number> = { ArrowRight: 7, ArrowLeft: -7, ArrowDown: 1, ArrowUp: -1 };
    if (e.key in step) {
      e.preventDefault();
      const from = active ?? cells.length - 1;
      active = Math.min(cells.length - 1, Math.max(0, from + step[e.key]!));
    } else if (e.key === 'Home' || e.key === 'End') {
      e.preventDefault();
      active = e.key === 'Home' ? 0 : cells.length - 1;
    }
  }
</script>

<div class="heatmap" style:--left="{LEFT}px">
  <div class="scroll">
    <div class="months" aria-hidden="true" style:width="{w}px">
      {#each months as m, i (i)}<span style:left="{m.col * STEP}px">{m.text}</span>{/each}
    </div>
    <div class="body">
      <div class="days" aria-hidden="true">
        {#each DAY_ROWS as [d, r] (d)}<span style:top="{r * STEP}px">{t(`stats.weekday.${d}`)}</span>{/each}
      </div>
      <div class="plot">
        <!-- One tab stop; arrow keys walk the days (announced through the live region below). -->
        <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
        <svg
          width={w}
          height={h}
          viewBox={`0 0 ${w} ${h}`}
          role="img"
          tabindex="0"
          aria-label={t('stats.heatmap.summary', { label, days: days.length, max })}
          {onkeydown}
          onfocus={() => {
            focused = true;
            active ??= cells.length - 1;
          }}
          onblur={() => {
            focused = false;
            active = null;
          }}
          onmouseover={(e) => {
            const i = (e.target as Element).getAttribute('data-i');
            if (i !== null) active = Number(i);
          }}
          onmouseleave={() => {
            if (!focused) active = null;
          }}
        >
          {#each cells as c, i (c.date)}
            <rect class="l{c.lv}" class:on={i === active} x={c.col * STEP} y={c.row * STEP} width={CELL} height={CELL} rx="2" data-i={i}>
              <title>{dayText(c)}</title>
            </rect>
          {/each}
        </svg>
        {#if cur}
          <span
            class="tip"
            aria-hidden="true"
            class:flip={cur.col * STEP > w - 120}
            class:below={cur.row < 2}
            style:left="{cur.col * STEP + CELL / 2}px"
            style:top="{cur.row < 2 ? cur.row * STEP + CELL + 6 : cur.row * STEP - 6}px">{dayText(cur)}</span
          >
        {/if}
      </div>
    </div>
  </div>
  <div class="legend">
    <span>{t('stats.heatmap.less')}</span>
    {#each [0, 1, 2, 3, 4] as l (l)}<span class="sw l{l}" aria-hidden="true"></span>{/each}
    <span>{t('stats.heatmap.more')}</span>
  </div>
  <p class="sr-only" aria-live="polite">{focused && cur ? dayText(cur) : ''}</p>
</div>

<style>
  .heatmap {
    --hm-0: var(--surface-2);
    --hm-4: light-dark(color-mix(in srgb, var(--accent) 40%, var(--accent-text)), var(--accent));
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    min-width: 0;
    font-family: var(--font-mono);
    font-size: var(--text-2xs);
    color: var(--text-2);
  }
  .scroll {
    overflow-x: auto;
    padding: 2px 0 var(--space-1);
  }
  .months {
    position: relative;
    height: 14px;
    margin-left: var(--left);
    margin-bottom: 4px;
  }
  .months span,
  .days span {
    position: absolute;
    white-space: nowrap;
    line-height: 12px;
  }
  .months span {
    top: 0;
  }
  .body {
    display: flex;
  }
  .days {
    position: relative;
    width: var(--left);
    flex: none;
  }
  .days span {
    left: 0;
  }
  .plot {
    position: relative;
    flex: none;
  }
  svg {
    display: block;
    border-radius: 2px;
  }
  svg:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 3px;
  }
  rect.on {
    stroke: var(--text-1);
    stroke-width: 1.5;
  }
  .l0 {
    fill: var(--hm-0);
    background: var(--hm-0);
  }
  .l1 {
    fill: color-mix(in srgb, var(--hm-4) 28%, var(--hm-0));
    background: color-mix(in srgb, var(--hm-4) 28%, var(--hm-0));
  }
  .l2 {
    fill: color-mix(in srgb, var(--hm-4) 50%, var(--hm-0));
    background: color-mix(in srgb, var(--hm-4) 50%, var(--hm-0));
  }
  .l3 {
    fill: color-mix(in srgb, var(--hm-4) 74%, var(--hm-0));
    background: color-mix(in srgb, var(--hm-4) 74%, var(--hm-0));
  }
  .l4 {
    fill: var(--hm-4);
    background: var(--hm-4);
  }
  .tip {
    position: absolute;
    z-index: 2;
    transform: translate(-50%, -100%);
    padding: 3px 7px;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-control);
    background: var(--surface-2);
    box-shadow: var(--shadow-float);
    color: var(--text-1);
    white-space: nowrap;
    pointer-events: none;
  }
  .tip.flip {
    transform: translate(calc(-100% + 8px), -100%);
  }
  .tip.below {
    transform: translate(-50%, 0);
  }
  .tip.below.flip {
    transform: translate(calc(-100% + 8px), 0);
  }
  .legend {
    display: flex;
    align-items: center;
    gap: 2px;
    padding-left: var(--left);
  }
  .sw {
    width: 10px;
    height: 10px;
    border-radius: 2px;
  }
  .legend span:first-child {
    margin-right: var(--space-1);
  }
  .legend span:last-child {
    margin-left: var(--space-1);
  }
</style>
