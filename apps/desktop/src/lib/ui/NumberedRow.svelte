<!--
  NumberedRow: one numbered entry of an editable list (Create's term | definition rows). A
  bordered card with the number in a round badge on the left, the row's fields in the middle
  and its actions on the right; `invalid` + `error` mark a row the save rejected.
  Use inside a `<ol>` (the badge is decorative; the list gives the position).
-->
<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    n: number;
    invalid?: boolean;
    error?: string | null;
    actions?: Snippet;
    children: Snippet;
    class?: string;
  }

  let { n, invalid = false, error = null, actions, children, class: klass = '' }: Props = $props();
</script>

<li class="fc-numbered-row {klass}" class:invalid>
  <span class="num" aria-hidden="true">{n}</span>
  <div class="body">{@render children()}</div>
  {#if actions}<div class="actions">{@render actions()}</div>{/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
</li>

<style>
  .fc-numbered-row {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr) auto;
    column-gap: var(--space-4);
    align-items: start;
    padding: var(--space-4);
    background: var(--surface-1);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    transition: border-color var(--motion-fast) var(--ease), box-shadow var(--motion-fast) var(--ease);
  }
  .fc-numbered-row:focus-within {
    border-color: var(--accent);
    box-shadow: var(--shadow-1);
  }
  .fc-numbered-row.invalid {
    border-color: var(--danger);
  }
  .num {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 28px;
    height: 28px;
    margin-top: 26px;
    padding: 0 var(--space-1);
    border-radius: var(--radius-pill);
    background: var(--surface-2);
    color: var(--text-2);
    font-size: var(--text-sm);
    font-weight: var(--weight-semibold);
    font-variant-numeric: tabular-nums;
  }
  .fc-numbered-row:focus-within .num {
    background: var(--accent-soft);
    color: var(--accent);
  }
  .body {
    min-width: 0;
  }
  .actions {
    display: flex;
    gap: var(--space-1);
    margin-top: 24px;
  }
  .error {
    grid-column: 2 / -1;
    margin: var(--space-2) 0 0;
    color: var(--danger);
    font-size: var(--text-sm);
  }
</style>
