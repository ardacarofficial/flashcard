<!-- One overview number: icon, label, big value and a quiet detail line. -->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { IconNode } from 'lucide';
  import { Card, Icon } from '$lib/ui';

  interface Props {
    icon: IconNode;
    label: string;
    value: string;
    detail?: string;
    /** Extra visual on the right (e.g. a ProgressRing). */
    aside?: Snippet;
  }

  let { icon, label, value, detail, aside }: Props = $props();
</script>

<Card as="div" class="fc-stat-tile">
  <div class="tile">
    <div class="text">
      <p class="label"><Icon {icon} size={16} /><span>{label}</span></p>
      <p class="value">{value}</p>
      {#if detail}<p class="detail">{detail}</p>{/if}
    </div>
    {#if aside}<div class="aside">{@render aside()}</div>{/if}
  </div>
</Card>

<style>
  .tile {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    min-height: 92px;
  }
  .text {
    min-width: 0;
  }
  p {
    margin: 0;
  }
  .label {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    font-size: var(--text-sm);
    color: var(--text-2);
  }
  .value {
    margin-top: var(--space-2);
    font-size: var(--text-xl);
    font-weight: var(--weight-bold);
    line-height: var(--leading-tight);
    font-variant-numeric: tabular-nums;
    letter-spacing: -0.01em;
  }
  .detail {
    margin-top: var(--space-1);
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .aside {
    flex: none;
  }
</style>
