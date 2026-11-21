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
    column-gap: var(--space-3);
    align-items: start;
    padding: var(--space-2) var(--space-3);
    background: var(--surface-1);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    transition: border-color var(--motion-fast) var(--ease);
  }
  .fc-numbered-row:focus-within {
    border-color: var(--border-strong);
  }
  .fc-numbered-row.invalid {
    border-color: var(--danger);
  }
  .num {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    min-width: 24px;
    height: var(--control-h-md);
    margin-top: 20px;
    padding: 0 var(--space-1);
    color: var(--text-2);
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    font-variant-numeric: tabular-nums;
  }
  .fc-numbered-row:focus-within .num {
    color: var(--accent-text);
  }
  .body {
    min-width: 0;
  }
  .actions {
    display: flex;
    gap: var(--space-1);
    margin-top: 20px;
  }
  .error {
    grid-column: 2 / -1;
    margin: var(--space-2) 0 0;
    color: var(--danger);
    font-size: var(--text-sm);
  }
</style>
