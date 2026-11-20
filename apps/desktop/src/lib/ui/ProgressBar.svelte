<!--
  Progress bar. Single value (`value` of `max`), or stacked `segments` (e.g. New / Learning /
  Mastered) — each segment's label is listed in the accessible name, so colour is never the
  only cue. `label` is required (what is progressing).
-->
<script lang="ts">
  import { t } from '$lib/i18n/index.svelte';
  import type { Tone } from './types';

  interface Segment {
    value: number;
    label: string;
    tone: Tone;
  }

  interface Props {
    label: string;
    value?: number;
    max?: number;
    segments?: Segment[];
    tone?: Tone;
    size?: 'sm' | 'md' | 'lg';
    /** Show "12 / 40" text next to the bar. */
    showValue?: boolean;
    class?: string;
  }

  let { label, value = 0, max = 100, segments, tone = 'accent', size = 'md', showValue = false, class: klass = '' }: Props = $props();

  const total = $derived(segments ? Math.max(max, segments.reduce((n, s) => n + s.value, 0)) : max);
  const pct = (v: number) => (total > 0 ? Math.min(100, Math.max(0, (v / total) * 100)) : 0);
  const summary = $derived(segments ? `${label}: ${segments.map((s) => `${s.label} ${s.value}`).join(', ')}` : label);
</script>

<div class="fc-progress {size} {klass}">
  {#if segments}
    <div class="track" role="img" aria-label={summary}>
      {#each segments as s, i (i)}
        <span class="fill {s.tone}" style:width="{pct(s.value)}%"></span>
      {/each}
    </div>
  {:else}
    <div
      class="track"
      role="progressbar"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={max}
      aria-valuenow={value}
      aria-valuetext={t('ui.progress.value', { value, max })}
    >
      <span class="fill {tone}" style:width="{pct(value)}%"></span>
    </div>
  {/if}
  {#if showValue && !segments}<span class="value">{t('ui.progress.value', { value, max })}</span>{/if}
</div>

<style>
  .fc-progress {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    width: 100%;
  }
  .track {
    flex: 1;
    display: flex;
    height: 8px;
    overflow: hidden;
    border-radius: var(--radius-pill);
    background: var(--surface-2);
    gap: 2px;
  }
  .sm .track {
    height: 4px;
  }
  .lg .track {
    height: 12px;
  }
  .fill {
    height: 100%;
    border-radius: var(--radius-pill);
    transition: width var(--motion-slow) var(--ease);
  }
  .accent {
    background: var(--accent);
  }
  .success {
    background: var(--success-solid);
  }
  .warning {
    background: var(--warning-solid);
  }
  .danger {
    background: var(--danger-solid);
  }
  .neutral {
    background: var(--gray-400);
  }
  .value {
    font-size: var(--text-xs);
    color: var(--text-2);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
</style>
