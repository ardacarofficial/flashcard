<!--
  App sidebar: `header` (brand), the main items (`children`, NavItems) and a `footer` (inbox,
  settings, profile). Icons + labels; below 1024 px wide only icons (labels stay the
  accessible names and show as hover hints). `collapsed` forces the narrow form.
-->
<script lang="ts">
  import type { Snippet } from 'svelte';

  interface Props {
    /** Accessible name of the navigation landmark. */
    label: string;
    collapsed?: boolean;
    header?: Snippet;
    children: Snippet;
    footer?: Snippet;
    class?: string;
  }

  let { label, collapsed = false, header, children, footer, class: klass = '' }: Props = $props();
</script>

<nav class="fc-sidebar {klass}" class:collapsed aria-label={label}>
  {#if header}<div class="header">{@render header()}</div>{/if}
  <ul class="main">{@render children()}</ul>
  {#if footer}<ul class="footer">{@render footer()}</ul>{/if}
</nav>

<style>
  .fc-sidebar {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    width: var(--sidebar-w);
    height: 100%;
    padding: var(--space-4) var(--space-3);
    background: var(--surface-2);
    border-inline-end: 1px solid var(--border);
    overflow-y: auto;
    overflow-x: hidden;
  }
  .header {
    padding: 0 var(--space-2) var(--space-4);
  }
  ul {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .main {
    flex: 1;
  }
  .footer {
    padding-top: var(--space-3);
    border-top: 1px solid var(--border);
  }
  .collapsed {
    width: var(--sidebar-w-collapsed);
    padding-inline: var(--space-2);
    align-items: center;
  }
  @media (max-width: 1023.98px) {
    .fc-sidebar {
      width: var(--sidebar-w-collapsed);
      padding-inline: var(--space-2);
      align-items: center;
    }
  }
</style>
