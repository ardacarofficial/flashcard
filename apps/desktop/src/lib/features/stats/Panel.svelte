<!--
  One block of the Statistics pages (v1.2): a 1px-bordered surface with a slim header row
  (title, optional one-line description under it, actions on the right) and a body.
  `body="rows"` drops the body padding for divided rows (FormRow, dense lists).
-->
<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    title?: string;
    description?: string;
    actions?: Snippet;
    body?: 'pad' | 'rows';
    class?: string;
    testid?: string;
    children: Snippet;
  }

  let { title, description, actions, body = 'pad', class: klass = '', testid, children }: Props = $props();
  const uid = $props.id();
</script>

<section class="fc-panel {klass}" aria-labelledby={title ? `p-${uid}` : undefined} data-testid={testid}>
  {#if title || actions}
  <header>
    <div class="titles">
      {#if title}<h2 id="p-{uid}">{title}</h2>{/if}
      {#if description}<p>{description}</p>{/if}
    </div>
    {#if actions}<div class="actions">{@render actions()}</div>{/if}
  </header>
  {/if}
  <div class="body {body}" class:headless={!title && !actions}>{@render children()}</div>
</section>

<style>
  .fc-panel {
    display: flex;
    flex-direction: column;
    min-width: 0;
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--surface-1);
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3);
    flex-wrap: wrap;
    min-height: 44px;
    padding: var(--space-2) var(--space-4);
  }
  .titles {
    min-width: 0;
    flex: 1 1 12rem;
  }
  h2 {
    margin: 0;
    font-size: var(--text-sm);
    font-weight: var(--weight-semibold);
    line-height: 1.3;
    color: var(--text-1);
  }
  p {
    margin: 2px 0 0;
    font-size: var(--text-xs);
    line-height: 1.4;
    color: var(--text-2);
  }
  .actions {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-wrap: wrap;
  }
  .body {
    flex: 1;
    min-width: 0;
  }
  .body.pad {
    padding: var(--space-1) var(--space-4) var(--space-4);
  }
  .body.rows {
    border-top: 1px solid var(--border);
  }
  .body.rows > :global(.fc-form-row) {
    padding-inline: var(--space-4);
  }
  .body.rows.headless {
    border-top: none;
  }
  .body.pad.headless {
    padding-top: var(--space-4);
  }
</style>
