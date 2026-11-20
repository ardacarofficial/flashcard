<!--
  Frame of one Settings (or Statistics) page: the page's PageHeader and a vertical stack of
  sections. Every v1.1 S4 screen starts with it so titles, widths and gaps match.
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
  .fc-settings-page.narrow {
    max-width: 760px;
  }
  .stack {
    display: flex;
    flex-direction: column;
    gap: var(--space-6);
  }
</style>
