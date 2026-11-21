<!-- Multi-line text input. `autoGrow` grows it with its content up to `maxRows`. `bind:value`. -->
<script lang="ts">
  import type { HTMLTextareaAttributes } from 'svelte/elements';
  import Field from './Field.svelte';

  interface Props extends Omit<HTMLTextareaAttributes, 'value'> {
    value?: string | null;
    label?: string;
    hint?: string;
    error?: string | null;
    hideLabel?: boolean;
    rows?: number;
    autoGrow?: boolean;
    maxRows?: number;
    element?: HTMLTextAreaElement | null;
  }

  let {
    value = $bindable(''),
    label,
    hint,
    error,
    hideLabel = false,
    rows = 3,
    autoGrow = false,
    maxRows = 12,
    id,
    required,
    element = $bindable(null),
    class: klass = '',
    ...rest
  }: Props = $props();

  $effect(() => {
    void value;
    if (!autoGrow || !element) return;
    const el = element;
    el.style.height = 'auto';
    const line = parseFloat(getComputedStyle(el).lineHeight) || 20;
    el.style.height = `${Math.min(el.scrollHeight, line * maxRows + 16)}px`;
  });
</script>

<Field {label} {hint} {error} {hideLabel} required={!!required} id={id ?? undefined} class={String(klass ?? "")}>
  {#snippet control({ id: cid, describedBy, invalid })}
    <textarea
      bind:this={element}
      bind:value
      id={cid}
      {rows}
      {required}
      class:invalid
      aria-invalid={invalid || undefined}
      aria-describedby={describedBy}
      {...rest}
    ></textarea>
  {/snippet}
</Field>

<style>
  textarea {
    width: 100%;
    padding: 6px 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius-control);
    background: var(--surface-2);
    color: var(--text-1);
    font: inherit;
    font-size: var(--text-sm);
    line-height: var(--leading);
    resize: vertical;
  }
  textarea::placeholder {
    color: var(--text-3);
  }
  textarea:hover:not(:disabled) {
    border-color: var(--border-strong);
  }
  textarea:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: 2px;
  }
  textarea:focus {
    border-color: var(--border-strong);
    background: var(--surface-1);
  }
  textarea.invalid {
    border-color: var(--danger);
  }
  textarea:disabled {
    background: var(--disabled-bg);
    color: var(--disabled-fg);
  }
</style>
