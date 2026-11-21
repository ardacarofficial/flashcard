<!-- Single-line text input with label, hint, error and an optional leading icon. `bind:value`. -->
<script lang="ts">
  import type { HTMLInputAttributes } from 'svelte/elements';
  import type { IconNode } from 'lucide';
  import Field from './Field.svelte';
  import Icon from './Icon.svelte';

  interface Props extends Omit<HTMLInputAttributes, 'size' | 'value' | 'type'> {
    value?: string | number | null;
    type?: 'text' | 'search' | 'email' | 'password' | 'number' | 'url' | 'tel' | 'date' | 'time' | 'color';
    label?: string;
    hint?: string;
    error?: string | null;
    hideLabel?: boolean;
    size?: 'sm' | 'md' | 'lg';
    icon?: IconNode;
    element?: HTMLInputElement | null;
  }

  let {
    value = $bindable(''),
    type = 'text',
    label,
    hint,
    error,
    hideLabel = false,
    size = 'md',
    icon,
    id,
    required,
    element = $bindable(null),
    class: klass = '',
    ...rest
  }: Props = $props();
</script>

<Field {label} {hint} {error} {hideLabel} required={!!required} id={id ?? undefined} class={String(klass ?? "")}>
  {#snippet control({ id: cid, describedBy, invalid })}
    <div class="fc-input {size}" class:invalid class:has-icon={!!icon}>
      {#if icon}<span class="lead"><Icon {icon} size={14} /></span>{/if}
      <input
        bind:this={element}
        bind:value
        id={cid}
        {type}
        {required}
        aria-invalid={invalid || undefined}
        aria-describedby={describedBy}
        {...rest}
      />
    </div>
  {/snippet}
</Field>

<style>
  .fc-input {
    position: relative;
    display: flex;
    align-items: center;
  }
  input {
    width: 100%;
    height: var(--control-h-md);
    padding: 0 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius-control);
    background: var(--surface-2);
    color: var(--text-1);
    font: inherit;
    font-size: var(--text-sm);
    transition:
      border-color var(--motion-fast) var(--ease),
      background-color var(--motion-fast) var(--ease);
  }
  input::placeholder {
    color: var(--text-3);
  }
  input:hover:not(:disabled) {
    border-color: var(--border-strong);
  }
  input:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: 2px;
  }
  input:focus {
    border-color: var(--border-strong);
    background: var(--surface-1);
  }
  input:disabled {
    background: var(--disabled-bg);
    color: var(--disabled-fg);
    cursor: not-allowed;
  }
  input[type='number'],
  input[type='date'],
  input[type='time'] {
    font-family: var(--font-mono);
    font-variant-numeric: tabular-nums;
  }
  input[type='color'] {
    width: var(--control-h-md);
    padding: 3px;
    cursor: pointer;
  }
  .sm input {
    height: var(--control-h-sm);
    font-size: var(--text-xs);
    padding: 0 var(--space-2);
  }
  .lg input {
    height: var(--control-h-lg);
    font-size: var(--text-md);
    padding: 0 var(--space-3);
  }
  .has-icon input {
    padding-inline-start: calc(10px + 14px + var(--space-2));
  }
  .lead {
    position: absolute;
    inset-inline-start: 10px;
    display: inline-flex;
    color: var(--text-3);
    pointer-events: none;
  }
  .invalid input {
    border-color: var(--danger);
  }
</style>
