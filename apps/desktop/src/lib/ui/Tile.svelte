<!-- Tile: a large clickable choice with icon, title and short description (study-mode tiles on the deck page). -->
<script lang="ts">
  import type { IconNode } from 'lucide';
  import Icon from './Icon.svelte';

  interface Props {
    icon: IconNode;
    title: string;
    description?: string;
    /** Small text in the corner (e.g. a count). */
    meta?: string;
    disabled?: boolean;
    onclick: () => void;
    class?: string;
  }

  let { icon, title, description, meta, disabled = false, onclick, class: klass = '' }: Props = $props();
</script>

<button type="button" class="fc-tile {klass}" {disabled} {onclick}>
  <span class="icon"><Icon {icon} size={24} /></span>
  <span class="title">{title}</span>
  {#if description}<span class="desc">{description}</span>{/if}
  {#if meta}<span class="meta">{meta}</span>{/if}
</button>

<style>
  .fc-tile {
    position: relative;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: var(--space-1);
    width: 100%;
    min-height: 120px;
    padding: var(--space-4);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--surface-1);
    color: var(--text-1);
    font: inherit;
    text-align: start;
    cursor: pointer;
    transition: border-color var(--motion-fast) var(--ease), box-shadow var(--motion-fast) var(--ease);
  }
  .fc-tile:hover:not(:disabled) {
    border-color: var(--accent);
    box-shadow: var(--shadow-1);
  }
  .fc-tile:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: 2px;
  }
  .fc-tile:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 40px;
    height: 40px;
    margin-bottom: var(--space-2);
    border-radius: var(--radius-control);
    background: var(--accent-soft);
    color: var(--accent);
  }
  .title {
    font-size: var(--text-md);
    font-weight: var(--weight-semibold);
  }
  .desc {
    font-size: var(--text-sm);
    color: var(--text-2);
    line-height: var(--leading-tight);
  }
  .meta {
    position: absolute;
    top: var(--space-4);
    inset-inline-end: var(--space-4);
    font-size: var(--text-xs);
    color: var(--text-2);
  }
</style>
