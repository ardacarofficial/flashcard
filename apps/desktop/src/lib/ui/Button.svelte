<!--
  Button. `primary` = the one main action of a view; `secondary` default; `ghost` for quiet
  toolbar actions; `danger` for destructive ones. `loading` disables it and shows a spinner.
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { HTMLButtonAttributes } from 'svelte/elements';
  import type { IconNode } from 'lucide';
  import Icon from './Icon.svelte';
  import Spinner from './Spinner.svelte';

  interface Props extends Omit<HTMLButtonAttributes, 'children'> {
    variant?: 'primary' | 'secondary' | 'ghost' | 'danger';
    size?: 'sm' | 'md' | 'lg';
    loading?: boolean;
    /** Leading icon. */
    icon?: IconNode;
    /** Trailing icon. */
    iconEnd?: IconNode;
    fullWidth?: boolean;
    children?: Snippet;
    /** The underlying element, for focus management. */
    element?: HTMLButtonElement | null;
  }

  let {
    variant = 'secondary',
    size = 'md',
    loading = false,
    icon,
    iconEnd,
    fullWidth = false,
    disabled = false,
    type = 'button',
    children,
    element = $bindable(null),
    class: klass = '',
    ...rest
  }: Props = $props();

  const iconSize = $derived(size === 'lg' ? 20 : 16);
</script>

<button
  bind:this={element}
  {type}
  class="fc-btn {variant} {size} {klass}"
  class:full={fullWidth}
  class:loading
  disabled={disabled || loading}
  aria-busy={loading || undefined}
  {...rest}
>
  {#if loading}
    <Spinner size={iconSize} />
  {:else if icon}
    <Icon {icon} size={iconSize} />
  {/if}
  {#if children}<span class="label">{@render children()}</span>{/if}
  {#if iconEnd && !loading}<Icon icon={iconEnd} size={iconSize} />{/if}
</button>

<style>
  .fc-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: var(--space-2);
    height: var(--control-h-md);
    padding: 0 var(--space-4);
    border: 1px solid transparent;
    border-radius: var(--radius-control);
    font: inherit;
    font-size: var(--text-sm);
    font-weight: var(--weight-semibold);
    line-height: 1;
    white-space: nowrap;
    cursor: pointer;
    transition:
      background-color var(--motion-fast) var(--ease),
      border-color var(--motion-fast) var(--ease),
      color var(--motion-fast) var(--ease);
  }
  .fc-btn:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: 2px;
  }
  .sm {
    height: var(--control-h-sm);
    padding: 0 var(--space-3);
    font-size: var(--text-xs);
    gap: var(--space-1);
  }
  .lg {
    height: var(--control-h-lg);
    padding: 0 var(--space-6);
    font-size: var(--text-md);
  }
  .full {
    width: 100%;
  }
  .primary {
    background: var(--accent);
    color: var(--accent-fg);
  }
  .primary:hover:not(:disabled) {
    background: var(--accent-hover);
  }
  .secondary {
    background: var(--surface-1);
    border-color: var(--border-strong);
    color: var(--text-1);
  }
  .secondary:hover:not(:disabled) {
    background: var(--surface-2);
  }
  .ghost {
    background: transparent;
    color: var(--text-1);
  }
  .ghost:hover:not(:disabled) {
    background: var(--surface-2);
  }
  .danger {
    background: var(--danger-solid);
    color: var(--danger-solid-fg);
  }
  .danger:hover:not(:disabled) {
    background: color-mix(in srgb, var(--danger-solid) 88%, var(--text-1));
  }
  .fc-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .fc-btn.loading {
    opacity: 0.8;
    cursor: progress;
  }
  .label {
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
