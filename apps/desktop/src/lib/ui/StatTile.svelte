<!--
  StatTile: one compact figure (Statistics, Home's stat strip): a small label, a big mono
  value, an optional hint line under it. The value is already formatted (`$lib/format`).
  `tone` colours the value only where it means something (success / warning / danger).
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { IconNode } from 'lucide';
  import Icon from './Icon.svelte';
  import type { Tone } from './types';

  interface Props {
    label: string;
    value: string | number;
    /** Unit or context after the value ("days", "/ 50"). */
    unit?: string;
    hint?: string;
    icon?: IconNode;
    tone?: Tone;
    size?: 'sm' | 'md';
    /** Extra content under the value (a sparkline, a bar). */
    children?: Snippet;
    testid?: string;
    class?: string;
  }

  let { label, value, unit, hint, icon, tone = 'neutral', size = 'md', children, testid, class: klass = '' }: Props = $props();
</script>

<div class="fc-stat {tone} {size} {klass}" data-testid={testid}>
  <span class="label">{#if icon}<Icon {icon} size={14} />{/if}{label}</span>
  <span class="value"><span class="num">{value}</span>{#if unit}<span class="unit">{unit}</span>{/if}</span>
  {#if children}{@render children()}{/if}
  {#if hint}<span class="hint">{hint}</span>{/if}
</div>

<style>
  .fc-stat {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    padding: var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--surface-1);
  }
  .sm {
    padding: var(--space-2) var(--space-3);
  }
  .label {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    overflow: hidden;
    color: var(--text-2);
    font-size: var(--text-xs);
    font-weight: var(--weight-medium);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .label :global(svg) {
    flex: none;
    color: var(--text-3);
  }
  .value {
    display: flex;
    align-items: baseline;
    gap: var(--space-1);
    color: var(--text-1);
    line-height: 1.2;
  }
  .num {
    font-family: var(--font-mono);
    font-size: var(--text-2xl);
    font-weight: var(--weight-semibold);
    font-variant-numeric: tabular-nums;
    letter-spacing: -0.02em;
  }
  .sm .num {
    font-size: var(--text-lg);
  }
  .unit {
    color: var(--text-2);
    font-size: var(--text-xs);
  }
  .success .num {
    color: var(--success);
  }
  .warning .num {
    color: var(--warning);
  }
  .danger .num {
    color: var(--danger);
  }
  .accent .num {
    color: var(--accent-text);
  }
  .hint {
    color: var(--text-2);
    font-size: var(--text-xs);
  }
</style>
