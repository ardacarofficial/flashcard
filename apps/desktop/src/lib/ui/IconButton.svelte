<!-- A square button showing only an icon. `label` is required: it is the accessible name and the hover hint. -->
<script lang="ts">
  import type { HTMLButtonAttributes } from 'svelte/elements';
  import type { IconNode } from 'lucide';
  import Icon from './Icon.svelte';
  import Spinner from './Spinner.svelte';

  interface Props extends Omit<HTMLButtonAttributes, 'children'> {
    icon: IconNode;
    label: string;
    variant?: 'ghost' | 'secondary' | 'primary' | 'danger';
    size?: 'sm' | 'md' | 'lg';
    /** Toggle buttons: `aria-pressed`. */
    pressed?: boolean;
    loading?: boolean;
    element?: HTMLButtonElement | null;
  }

  let {
    icon,
    label,
    variant = 'ghost',
    size = 'md',
    pressed,
    loading = false,
    disabled = false,
    type = 'button',
    element = $bindable(null),
    class: klass = '',
    ...rest
  }: Props = $props();

  const iconSize = $derived(size === 'sm' ? 14 : size === 'lg' ? 18 : 16);
</script>

<button
  bind:this={element}
  {type}
  class="fc-iconbtn {variant} {size} {klass}"
  aria-label={label}
  title={label}
  aria-pressed={pressed}
  aria-busy={loading || undefined}
  disabled={disabled || loading}
  {...rest}
>
  {#if loading}<Spinner size={iconSize} />{:else}<Icon {icon} size={iconSize} />{/if}
</button>

<style>
  .fc-iconbtn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: var(--control-h-md);
    height: var(--control-h-md);
    padding: 0;
    border: 1px solid transparent;
    border-radius: var(--radius-control);
    background: transparent;
    color: var(--text-2);
    cursor: pointer;
    transition: background-color var(--motion-fast) var(--ease), color var(--motion-fast) var(--ease);
  }
  .fc-iconbtn:hover:not(:disabled) {
    background: var(--surface-2);
    color: var(--text-1);
  }
  .fc-iconbtn:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: 2px;
  }
  .fc-iconbtn[aria-pressed='true'] {
    background: var(--surface-3);
    color: var(--accent-text);
  }
  .sm {
    width: var(--control-h-sm);
    height: var(--control-h-sm);
  }
  .lg {
    width: var(--control-h-lg);
    height: var(--control-h-lg);
  }
  .secondary {
    background: var(--surface-2);
    border-color: var(--border);
    color: var(--text-1);
  }
  .secondary:hover:not(:disabled) {
    background: var(--surface-3);
    border-color: var(--border-strong);
  }
  .primary {
    background: var(--accent);
    color: var(--accent-fg);
  }
  .primary:hover:not(:disabled) {
    background: var(--accent-hover);
    color: var(--accent-fg);
  }
  .danger {
    color: var(--danger);
  }
  .danger:hover:not(:disabled) {
    background: var(--danger-soft);
    color: var(--danger);
  }
  .fc-iconbtn:disabled {
    opacity: 0.5;
    cursor: default;
  }
</style>
