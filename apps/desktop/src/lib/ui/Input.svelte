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
      {#if icon}<span class="lead"><Icon {icon} size={16} /></span>{/if}
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
    padding: 0 var(--space-3);
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-control);
    background: var(--surface-1);
    color: var(--text-1);
    font: inherit;
    font-size: var(--text-sm);
    transition: border-color var(--motion-fast) var(--ease);
  }
  input::placeholder {
    color: var(--text-2);
  }
  input:hover:not(:disabled) {
    border-color: var(--text-2);
  }
  input:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: 1px;
    border-color: var(--accent);
  }
  input:disabled {
    opacity: 0.55;
  }
  .sm input {
    height: var(--control-h-sm);
    font-size: var(--text-xs);
  }
  .lg input {
    height: var(--control-h-lg);
    font-size: var(--text-md);
    padding: 0 var(--space-4);
  }
  .has-icon input {
    padding-inline-start: calc(var(--space-3) + 16px + var(--space-2));
  }
  .lead {
    position: absolute;
    inset-inline-start: var(--space-3);
    display: inline-flex;
    color: var(--text-2);
    pointer-events: none;
  }
  .invalid input {
    border-color: var(--danger);
  }
</style>
