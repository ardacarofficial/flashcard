<!--
  Button. `primary` = the one main action of a view; `secondary` default; `ghost` for quiet
  toolbar actions; `danger` for destructive ones. `loading` disables it and shows a spinner.
  `kbd` adds a small key hint after the label (`kbd="N"`, `kbd="Mod+Enter"`); the shortcut
  itself is bound elsewhere.
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import type { HTMLButtonAttributes } from 'svelte/elements';
  import type { IconNode } from 'lucide';
  import Icon from './Icon.svelte';
  import Kbd from './Kbd.svelte';
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
    /** Key hint shown after the label (decorative: give the shortcut in `aria-keyshortcuts`). */
    kbd?: string | string[];
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
    kbd,
    disabled = false,
    type = 'button',
    children,
    element = $bindable(null),
    class: klass = '',
    ...rest
  }: Props = $props();

  const iconSize = $derived(size === 'sm' ? 14 : 16);
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
  {#if kbd}<span class="hint" aria-hidden="true"><Kbd keys={kbd} size="sm" /></span>{/if}
</button>

<style>
  .fc-btn {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: var(--space-2);
    height: var(--control-h-md);
    padding: 0 var(--space-3);
    border: 1px solid transparent;
    border-radius: var(--radius-control);
    font: inherit;
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
    line-height: 1;
    white-space: nowrap;
    cursor: pointer;
    user-select: none;
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
    padding: 0 10px;
    font-size: var(--text-xs);
    gap: 6px;
  }
  .lg {
    height: var(--control-h-lg);
    padding: 0 var(--space-4);
    font-size: var(--text-md);
  }
  .full {
    width: 100%;
  }
  .primary {
    background: var(--accent);
    color: var(--accent-fg);
    font-weight: var(--weight-semibold);
  }
  .primary:hover:not(:disabled) {
    background: var(--accent-hover);
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
  .ghost {
    background: transparent;
    color: var(--text-2);
  }
  .ghost:hover:not(:disabled) {
    background: var(--surface-2);
    color: var(--text-1);
  }
  .danger {
    background: var(--danger-soft);
    border-color: color-mix(in srgb, var(--danger) 35%, transparent);
    color: var(--danger);
  }
  .danger:hover:not(:disabled) {
    border-color: var(--danger);
  }
  .fc-btn:disabled {
    cursor: not-allowed;
  }
  .primary:disabled,
  .danger:disabled {
    background: var(--disabled-bg);
    border-color: var(--disabled-border);
    color: var(--disabled-fg);
  }
  .secondary:disabled,
  .ghost:disabled {
    color: var(--disabled-fg);
    border-color: var(--disabled-border);
  }
  .ghost:disabled {
    border-color: transparent;
  }
  .fc-btn.loading {
    opacity: 0.8;
    cursor: progress;
  }
  .label {
    overflow: hidden;
    text-overflow: ellipsis;
    /* Room for Turkish capitals' dots (Ö, İ) and cedillas, which line-height 1 clips. */
    line-height: var(--leading-tight);
  }
  .hint {
    display: inline-flex;
    margin-inline-start: 2px;
  }
  /* On the accent fill the hint is a darker tint of it, not a grey key. */
  .primary:not(:disabled) .hint :global(kbd) {
    background: color-mix(in srgb, var(--accent-fg) 12%, transparent);
    border-color: color-mix(in srgb, var(--accent-fg) 22%, transparent);
    color: var(--accent-fg);
  }
  /* Disabled: the key stays legible, in the same muted tone as the label. */
  .fc-btn:disabled .hint :global(kbd) {
    background: transparent;
    border-color: var(--disabled-border);
    color: var(--disabled-fg);
  }
</style>
