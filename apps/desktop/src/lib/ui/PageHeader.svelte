<!--
  Page header: the page's one `<h1>`, an optional subtitle and back link, chips (`meta`) and
  actions on the right. Section sub-navigation (Tabs) goes in `tabs`. The shell's own header
  parts (deck picker, section tabs — `page-chrome.ts`) are drawn here too unless
  `chrome={false}`, so every page starts with its title.
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import Button from './Button.svelte';
  import { ChevronLeft } from './icons';
  import { getPageChrome } from './page-chrome';

  interface Props {
    title: string;
    subtitle?: string;
    back?: { label: string; onclick: () => void };
    meta?: Snippet;
    actions?: Snippet;
    tabs?: Snippet;
    /** Draw the shell's deck picker and section tabs (default). `false` on sub-pages. */
    chrome?: boolean;
    class?: string;
  }

  let { title, subtitle, back, meta, actions, tabs, chrome = true, class: klass = '' }: Props = $props();
  const shell = getPageChrome();
  const shellActions = $derived(chrome ? (shell?.actions ?? null) : null);
  const shellTabs = $derived(chrome ? (shell?.tabs ?? null) : null);
</script>

<header class="fc-page-header {klass}">
  {#if back}
    <Button variant="ghost" size="sm" icon={ChevronLeft} class="back" onclick={back.onclick}>{back.label}</Button>
  {/if}
  <div class="row">
    <div class="titles">
      <h1>{title}</h1>
      {#if subtitle}<p class="subtitle">{subtitle}</p>{/if}
      {#if meta}<div class="meta">{@render meta()}</div>{/if}
    </div>
    {#if actions || shellActions}
      <div class="actions">
        {#if shellActions}{@render shellActions()}{/if}
        {#if actions}{@render actions()}{/if}
      </div>
    {/if}
  </div>
  {#if tabs}<div class="tabs">{@render tabs()}</div>{:else if shellTabs}<div class="tabs">{@render shellTabs()}</div>{/if}
</header>

<style>
  .fc-page-header {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    margin-bottom: var(--space-6);
  }
  .fc-page-header :global(.back) {
    align-self: flex-start;
    margin-inline-start: calc(-1 * var(--space-3));
  }
  .row {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: var(--space-4);
    flex-wrap: wrap;
  }
  .titles {
    min-width: 0;
  }
  h1 {
    margin: 0;
    font-size: var(--text-xl);
    font-weight: var(--weight-bold);
    line-height: var(--leading-tight);
    letter-spacing: -0.01em;
    overflow-wrap: anywhere;
  }
  .subtitle {
    margin: var(--space-1) 0 0;
    color: var(--text-2);
    font-size: var(--text-sm);
  }
  .meta {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-2);
    margin-top: var(--space-2);
  }
  .actions {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-wrap: wrap;
  }
  .tabs {
    margin-top: var(--space-3);
  }
</style>
