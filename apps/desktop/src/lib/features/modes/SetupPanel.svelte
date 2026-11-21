<!--
  The start panel of a study mode (v1.2): what will be studied and the start button on one
  line, a notice when there is nothing to study, then the options as dense rows (`OptRow`).
-->
<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    title: string;
    /** One line under the title (counts in mono, or a short intro). */
    meta?: Snippet;
    actions: Snippet;
    notice?: string | null;
    /** Option rows / toggles under the head. */
    children?: Snippet;
    testid?: string;
  }

  let { title, meta, actions, notice = null, children, testid }: Props = $props();
</script>

<section class="setup" data-testid={testid}>
  <div class="head">
    <div class="text">
      <h2>{title}</h2>
      {#if meta}<div class="meta">{@render meta()}</div>{/if}
    </div>
    <div class="actions">{@render actions()}</div>
  </div>
  {#if notice}<p class="notice" role="status">{notice}</p>{/if}
  {#if children}{@render children()}{/if}
</section>

<style>
  .setup {
    max-width: 760px;
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--surface-1);
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-4);
    flex-wrap: wrap;
    padding: var(--space-4);
  }
  .text {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
  }
  h2 {
    margin: 0;
    font-size: var(--text-lg);
    font-weight: var(--weight-semibold);
  }
  .meta {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-1) var(--space-3);
    font-size: var(--text-sm);
    color: var(--text-2);
  }
  .actions {
    display: flex;
    gap: var(--space-2);
    flex-wrap: wrap;
  }
  .notice {
    margin: 0;
    padding: var(--space-2) var(--space-4);
    border-top: 1px solid var(--border);
    background: var(--surface-2);
    color: var(--text-2);
    font-size: var(--text-sm);
  }
</style>
