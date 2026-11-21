<!--
  One settings row inside a `SettingsSection body="rows"` (v1.2): label + one-line description
  on the left, the control on the right, ~44 px high, a 1px line below. `stack` puts wide
  controls (lists, text areas, swatches) under the text; below 640 px every row stacks.
  Pass `labelFor` when the control is a plain lib/ui control with `hideLabel` and its own id,
  so clicking the label focuses it; Switch / Checkbox carry their own label (`ariaLabel`).
-->
<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    label: string;
    description?: string;
    /** id of the control the label names (`<label for>`). */
    labelFor?: string;
    /** Control below the text (wide controls: lists, text areas). */
    stack?: boolean;
    children?: Snippet;
    class?: string;
  }

  let { label, description, labelFor, stack = false, children, class: klass = '' }: Props = $props();
</script>

<div class="fc-form-row {klass}" class:stack>
  <div class="text">
    {#if labelFor}
      <label class="label" for={labelFor}>{label}</label>
    {:else}
      <span class="label">{label}</span>
    {/if}
    {#if description}<p class="desc">{description}</p>{/if}
  </div>
  {#if children}<div class="control">{@render children()}</div>{/if}
</div>

<style>
  .fc-form-row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, auto);
    align-items: center;
    gap: var(--space-2) var(--space-6);
    min-height: 44px;
    padding: var(--space-2) 0;
    border-bottom: 1px solid var(--border);
  }
  .stack {
    grid-template-columns: minmax(0, 1fr);
    padding-bottom: var(--space-3);
  }
  .text {
    min-width: 0;
  }
  .label {
    display: block;
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
    line-height: var(--leading-tight);
    color: var(--text-1);
  }
  .desc {
    margin: 3px 0 0;
    font-size: var(--text-xs);
    color: var(--text-2);
    line-height: var(--leading);
  }
  .control {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: var(--space-2);
    flex-wrap: wrap;
    min-width: 0;
  }
  .control :global(.fc-field) {
    min-width: 14rem;
  }
  .stack .control {
    justify-content: flex-start;
  }
  .stack .control :global(.fc-field) {
    flex: 1 1 100%;
  }
  @media (max-width: 639.98px) {
    .fc-form-row {
      grid-template-columns: minmax(0, 1fr);
    }
    .control {
      justify-content: flex-start;
    }
  }
</style>
