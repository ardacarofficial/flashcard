<!--
  Label + control + hint/error, the frame every form control uses. The control snippet gets
  the ids to wire up: `<Field label=… >{#snippet control({ id, describedBy, invalid })}…{/snippet}</Field>`.
-->
<script lang="ts">
  import type { Snippet } from 'svelte';

  interface ControlIds {
    id: string;
    /** Value for the control's `aria-describedby` (hint and/or error), if any. */
    describedBy: string | undefined;
    invalid: boolean;
  }

  interface Props {
    label?: string;
    hint?: string;
    /** Error text; marks the control invalid. */
    error?: string | null;
    /** Control id (default: generated). */
    id?: string;
    required?: boolean;
    /** Hide the label visually (it stays the accessible name). */
    hideLabel?: boolean;
    control: Snippet<[ControlIds]>;
    class?: string;
  }

  const uid = $props.id();
  let { label, hint, error, id, required = false, hideLabel = false, control, class: klass = '' }: Props = $props();

  const controlId = $derived(id ?? `fc-field-${uid}`);
  const hintId = $derived(`${controlId}-hint`);
  const errorId = $derived(`${controlId}-error`);
  const describedBy = $derived([hint ? hintId : null, error ? errorId : null].filter(Boolean).join(' ') || undefined);
</script>

<div class="fc-field {klass}">
  {#if label}
    <label for={controlId} class:sr-only={hideLabel}>
      {label}{#if required}<span class="req" aria-hidden="true">*</span>{/if}
    </label>
  {/if}
  {@render control({ id: controlId, describedBy, invalid: !!error })}
  {#if hint}<p id={hintId} class="hint">{hint}</p>{/if}
  {#if error}<p id={errorId} class="error-text" role="alert">{error}</p>{/if}
</div>

<style>
  .fc-field {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    min-width: 0;
  }
  label {
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
    color: var(--text-1);
  }
  .req {
    color: var(--danger);
    margin-inline-start: 2px;
  }
  .hint,
  .error-text {
    margin: 0;
    font-size: var(--text-xs);
    line-height: var(--leading-tight);
  }
  .hint {
    color: var(--text-2);
  }
  .error-text {
    color: var(--danger);
  }
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
    white-space: nowrap;
  }
</style>
