<!--
  Frame of one Settings (or Statistics) page (v1.2): the PageHeader (title + one plain
  sentence: what this page is for and when you would touch it) and the page's sections stacked with even gaps — no boxes. Every settings
  screen starts with it so titles, widths, row heights and gaps match. Destructive sections
  (`SettingsSection tone="danger"`) go last.
-->
<script lang="ts">
  import type { Snippet } from 'svelte';
  import { PageHeader } from '$lib/ui';

  interface Props {
    title: string;
    subtitle?: string;
    actions?: Snippet;
    meta?: Snippet;
    /** `narrow` (720 px, forms) or `wide` (full, charts and tables). */
    width?: 'narrow' | 'wide';
    children: Snippet;
  }

  let { title, subtitle, actions, meta, width = 'narrow', children }: Props = $props();
</script>

<div class="fc-settings-page {width}">
  <PageHeader {title} {subtitle} {actions} {meta} />
  <div class="stack">{@render children()}</div>
</div>

<style>
  .fc-settings-page {
    /* Rows stack their controls under the text when this column is narrow (FormRow). */
    container: fc-settings / inline-size;
  }
  .fc-settings-page.narrow {
    max-width: 720px;
  }
  .stack {
    display: flex;
    flex-direction: column;
    gap: var(--space-8);
  }
</style>
