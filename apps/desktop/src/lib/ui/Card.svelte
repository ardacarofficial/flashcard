<!--
  Card: the raised surface for grouped content. With `onclick` the whole card is a button
  (keep its content non-interactive then).
-->
<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    padding?: 'none' | 'sm' | 'md' | 'lg';
    /** Light shadow (default: border only). */
    elevated?: boolean;
    onclick?: () => void;
    /** Accessible name of a clickable card (default: its text). */
    label?: string;
    /** Element for a static card. */
    as?: 'div' | 'section' | 'article' | 'li';
    children: Snippet;
    /** `data-testid` (visual-QA state hooks, docs/11 §5). */
    testid?: string;
    class?: string;
  }

  let { padding = 'md', elevated = false, onclick, label, as = 'div', children, testid, class: klass = '' }: Props = $props();
</script>

{#if onclick}
  <button type="button" class="fc-card pad-{padding} interactive {klass}" class:elevated aria-label={label} data-testid={testid} {onclick}>
    {@render children()}
  </button>
{:else}
  <svelte:element this={as} class="fc-card pad-{padding} {klass}" class:elevated data-testid={testid}>
    {@render children()}
  </svelte:element>
{/if}

<style>
  .fc-card {
    display: block;
    background: var(--surface-1);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    color: var(--text-1);
    min-width: 0;
  }
  .elevated {
    box-shadow: var(--shadow-1);
  }
  .pad-none {
    padding: 0;
  }
  .pad-sm {
    padding: var(--space-3);
  }
  .pad-md {
    padding: var(--space-4);
  }
  .pad-lg {
    padding: var(--space-6);
  }
  button.fc-card {
    width: 100%;
    font: inherit;
    text-align: start;
    cursor: pointer;
    transition: border-color var(--motion-fast) var(--ease), box-shadow var(--motion-fast) var(--ease);
  }
  .interactive:hover {
    border-color: var(--border-strong);
    box-shadow: var(--shadow-1);
  }
  .interactive:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: 2px;
  }
</style>
