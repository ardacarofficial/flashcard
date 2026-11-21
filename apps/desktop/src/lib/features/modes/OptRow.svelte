<!--
  One dense option row of a session's start panel (v1.2): label (+ short description) on the
  left, the control on the right; rows are divided by a hairline. Controls inside use
  `hideLabel` / `ariaLabel` so the row's label is the only visible one.
-->
<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    label: string;
    description?: string;
    /** id of the control: clicking the label focuses it (its own hidden label stays its name). */
    labelFor?: string;
    children?: Snippet;
  }

  let { label, description, labelFor, children }: Props = $props();
</script>

<div class="opt-row">
  <div class="text">
    <!-- The control keeps its own (hidden) label as its name; clicking this one only focuses it. -->
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <span class="label" onclick={() => labelFor && document.getElementById(labelFor)?.focus()}>{label}</span>
    {#if description}<span class="desc">{description}</span>{/if}
  </div>
  {#if children}<div class="control">{@render children()}</div>{/if}
</div>

<style>
  .opt-row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    align-items: center;
    gap: var(--space-2) var(--space-6);
    min-height: 48px;
    padding: var(--space-2) var(--space-4);
    border-top: 1px solid var(--border);
  }
  .text {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .label {
    font-size: var(--text-sm);
    font-weight: var(--weight-medium);
    color: var(--text-1);
  }
  .desc {
    font-size: var(--text-xs);
    color: var(--text-2);
  }
  .control {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: var(--space-2);
    flex-wrap: wrap;
    min-width: 0;
  }
  @media (max-width: 640px) {
    .opt-row {
      grid-template-columns: minmax(0, 1fr);
    }
    .control {
      justify-content: flex-start;
    }
  }
</style>
