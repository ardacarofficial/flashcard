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
    /** `data-testid` of the button (test and visual-QA hooks). */
    testid?: string;
  }

  let { icon, title, description, meta, disabled = false, onclick, class: klass = '', testid }: Props = $props();
</script>

<button type="button" class="fc-tile {klass}" {disabled} {onclick} data-testid={testid}>
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
    min-height: 0;
    padding: var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-card);
    background: var(--surface-1);
    color: var(--text-1);
    font: inherit;
    text-align: start;
    cursor: pointer;
    transition: border-color var(--motion-fast) var(--ease), background-color var(--motion-fast) var(--ease);
  }
  .fc-tile:hover:not(:disabled) {
    border-color: var(--border-strong);
    background: var(--surface-2);
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
    width: 28px;
    height: 28px;
    margin-bottom: var(--space-1);
    border: 1px solid var(--border);
    border-radius: var(--radius-control);
    background: var(--surface-2);
    color: var(--accent-text);
  }
  .title {
    font-size: var(--text-sm);
    font-weight: var(--weight-semibold);
  }
  .desc {
    font-size: var(--text-xs);
    color: var(--text-2);
    line-height: var(--leading-tight);
  }
  .meta {
    position: absolute;
    top: var(--space-3);
    inset-inline-end: var(--space-3);
    font-family: var(--font-mono);
    font-size: var(--text-2xs);
    color: var(--text-2);
  }
</style>
