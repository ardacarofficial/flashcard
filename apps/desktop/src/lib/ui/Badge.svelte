<!--
  Badge: a count or a one-word status. `count` renders nothing when 0/null (hide empty badges);
  counts above `max` show as "99+". Give `label` when the number alone is unclear to screen readers.
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { Tone } from './types';

  interface Props {
    count?: number | null;
    max?: number;
    tone?: Tone;
    variant?: 'solid' | 'soft';
    /** Accessible text (e.g. "3 waiting"); the visible number is then hidden from readers. */
    label?: string;
    children?: Snippet;
    class?: string;
  }

  let { count, max = 99, tone = 'accent', variant = 'solid', label, children, class: klass = '' }: Props = $props();

  const shown = $derived(count == null ? null : count > max ? `${max}+` : String(count));
</script>

{#if children || (shown !== null && count !== 0)}
  <span class="fc-badge {tone} {variant} {klass}" role={label ? 'status' : undefined} aria-label={label}>
    {#if children}{@render children()}{:else}<span aria-hidden={label ? 'true' : undefined}>{shown}</span>{/if}
  </span>
{/if}

<style>
  .fc-badge {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 20px;
    height: 20px;
    padding: 0 6px;
    border-radius: var(--radius-pill);
    font-size: var(--text-xs);
    font-weight: var(--weight-semibold);
    line-height: 1;
    font-variant-numeric: tabular-nums;
  }
  .solid.accent {
    background: var(--accent);
    color: var(--accent-fg);
  }
  .solid.neutral {
    background: var(--text-2);
    color: var(--surface-1);
  }
  .solid.success {
    background: var(--success-solid);
    color: var(--success-solid-fg);
  }
  .solid.warning {
    background: var(--warning-solid);
    color: var(--warning-solid-fg);
  }
  .solid.danger {
    background: var(--danger-solid);
    color: var(--danger-solid-fg);
  }
  .soft.accent {
    background: var(--accent-soft);
    color: var(--text-1);
  }
  .soft.neutral {
    background: var(--surface-2);
    color: var(--text-2);
  }
  .soft.success {
    background: var(--success-soft);
    color: var(--success);
  }
  .soft.warning {
    background: var(--warning-soft);
    color: var(--warning);
  }
  .soft.danger {
    background: var(--danger-soft);
    color: var(--danger);
  }
</style>
