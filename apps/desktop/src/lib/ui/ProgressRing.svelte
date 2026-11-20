<!-- Circular progress (test score, goal). Shows the percentage in the middle unless `children` is given. -->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import { t } from '$lib/i18n/index.svelte';
  import type { Tone } from './types';

  interface Props {
    label: string;
    value: number;
    max?: number;
    size?: number;
    stroke?: number;
    tone?: Tone;
    children?: Snippet;
    class?: string;
  }

  let { label, value, max = 100, size = 64, stroke = 6, tone = 'accent', children, class: klass = '' }: Props = $props();

  const r = $derived((size - stroke) / 2);
  const c = $derived(2 * Math.PI * r);
  const ratio = $derived(max > 0 ? Math.min(1, Math.max(0, value / max)) : 0);
  const percent = $derived(Math.round(ratio * 100));
</script>

<div
  class="fc-ring {tone} {klass}"
  style:width="{size}px"
  style:height="{size}px"
  role="progressbar"
  aria-label={label}
  aria-valuemin={0}
  aria-valuemax={max}
  aria-valuenow={value}
>
  <svg width={size} height={size} viewBox="0 0 {size} {size}" aria-hidden="true">
    <circle class="bg" cx={size / 2} cy={size / 2} {r} stroke-width={stroke} fill="none" />
    <circle
      class="fg"
      cx={size / 2}
      cy={size / 2}
      {r}
      stroke-width={stroke}
      fill="none"
      stroke-linecap="round"
      stroke-dasharray={c}
      stroke-dashoffset={c * (1 - ratio)}
      transform="rotate(-90 {size / 2} {size / 2})"
    />
  </svg>
  <span class="center" style:font-size="{Math.max(11, size / 4.5)}px">
    {#if children}{@render children()}{:else}{t('ui.progress.percent', { value: percent })}{/if}
  </span>
</div>

<style>
  .fc-ring {
    position: relative;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: none;
  }
  svg {
    position: absolute;
    inset: 0;
  }
  .bg {
    stroke: var(--surface-2);
  }
  .fg {
    transition: stroke-dashoffset var(--motion-slow) var(--ease);
  }
  .accent .fg {
    stroke: var(--accent);
  }
  .success .fg {
    stroke: var(--success-solid);
  }
  .warning .fg {
    stroke: var(--warning-solid);
  }
  .danger .fg {
    stroke: var(--danger-solid);
  }
  .neutral .fg {
    stroke: var(--text-2);
  }
  .center {
    position: relative;
    font-weight: var(--weight-semibold);
    color: var(--text-1);
    font-variant-numeric: tabular-nums;
  }
</style>
