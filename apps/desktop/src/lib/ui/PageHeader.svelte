<!--
  Page header: the page's one `<h1>`, an optional subtitle and back link, chips (`meta`) and
  actions on the right. Section sub-navigation (Tabs) goes in `tabs`.
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import Button from './Button.svelte';
  import { ChevronLeft } from './icons';

  interface Props {
    title: string;
    subtitle?: string;
    back?: { label: string; onclick: () => void };
    meta?: Snippet;
    actions?: Snippet;
    tabs?: Snippet;
    class?: string;
  }

  let { title, subtitle, back, meta, actions, tabs, class: klass = '' }: Props = $props();
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
    {#if actions}<div class="actions">{@render actions()}</div>{/if}
  </div>
  {#if tabs}<div class="tabs">{@render tabs()}</div>{/if}
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
    margin-top: var(--space-2);
  }
</style>
