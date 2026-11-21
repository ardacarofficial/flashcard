<!--
  Page header: the page's one `<h1>`, an optional subtitle and back link, chips (`meta`) and
  actions on the right. Section sub-navigation (Tabs) goes in `tabs`. The shell's own header
  parts (deck picker, section tabs — `page-chrome.ts`) are drawn here too unless
  `chrome={false}`, so every page starts with its title. With `back` (a sub-page) the title is
  also appended to the top bar's trail.
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import Button from './Button.svelte';
  import { ChevronLeft } from './icons';
  import { getPageChrome } from './page-chrome';
  import type { Crumb } from './types';

  interface Props {
    title: string;
    subtitle?: string;
    back?: { label: string; onclick: () => void };
    meta?: Snippet;
    actions?: Snippet;
    tabs?: Snippet;
    /** Draw the shell's deck picker and section tabs (default). `false` on sub-pages. */
    chrome?: boolean;
    /** Crumbs before the title in the top bar's trail (with `back`), e.g. a sub-deck's parents. */
    parents?: Crumb[];
    class?: string;
  }

  let { title, subtitle, back, meta, actions, tabs, chrome = true, parents, class: klass = '' }: Props = $props();
  const shell = getPageChrome();
  const shellActions = $derived(chrome ? (shell?.actions ?? null) : null);
  const shellTabs = $derived(chrome ? (shell?.tabs ?? null) : null);

  // A sub-page (with a back link) puts its title in the top bar's trail; the page crumb links back.
  $effect(() => {
    if (!back || !shell) return;
    shell.setTrail([...(parents ?? []), { label: title }], back.onclick);
    return () => shell.setTrail(null);
  });
</script>

<header class="fc-page-header {klass}">
  {#if back}
    <Button variant="ghost" size="sm" icon={ChevronLeft} class="back" onclick={back.onclick}>{back.label}</Button>
  {/if}
  <div class="row" class:stacked={!!subtitle || !!meta}>
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
    gap: var(--space-1);
    margin-bottom: var(--space-5);
  }
  .fc-page-header :global(.back) {
    align-self: flex-start;
    margin-inline-start: -10px;
  }
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-3) var(--space-4);
    flex-wrap: wrap;
    min-height: var(--control-h-md);
  }
  /* With a subtitle or chips under the title, the actions line up with the title line (not
     with the middle of the whole block): a 32 px control centred on the 26 px title line. */
  .row.stacked {
    align-items: flex-start;
  }
  .row.stacked .actions {
    margin-top: calc((var(--text-xl) * 1.3 - var(--control-h-md)) / 2);
  }
  /* The titles take the room left of the actions (a long subtitle wraps there instead of
     pushing the actions onto their own line). */
  .titles {
    flex: 1 1 16rem;
    min-width: 0;
  }
  h1 {
    margin: 0;
    font-size: var(--text-xl);
    font-weight: var(--weight-semibold);
    line-height: var(--leading-tight);
    letter-spacing: -0.015em;
    overflow-wrap: anywhere;
  }
  .subtitle {
    margin: 2px 0 0;
    color: var(--text-2);
    font-size: var(--text-sm);
  }
  .meta {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
    margin-top: var(--space-2);
  }
  .actions {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    flex-wrap: wrap;
  }
  .tabs {
    margin-top: var(--space-2);
  }
</style>
