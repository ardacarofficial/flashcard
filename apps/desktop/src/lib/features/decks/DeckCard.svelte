<!--
  A deck as a compact card (the Library's secondary grid view): name with its due count, card
  and sub-deck counts, classification chips and the New / Learning / Mastered bar. Every
  number comes from the backend (`decks.tree` counts, `decks.summaries`); `summary` is null
  while it loads.
-->
<script lang="ts">
  import { formatCount } from '$lib/format';
  import type { DeckSummary, DeckTreeNode } from '$lib/api/types';
  import { t } from '$lib/i18n/index.svelte';
  import { Card, Chip } from '$lib/ui';
  import MasteryBar from './MasteryBar.svelte';

  interface Props {
    node: DeckTreeNode;
    summary: DeckSummary | null;
    onopen: () => void;
    /** Show the classification chips. */
    chips?: boolean;
  }
  let { node, summary, onopen, chips = true }: Props = $props();

  const due = $derived(node.counts.due + node.counts.learning);
</script>

<Card onclick={onopen} label={t('library.open', { name: node.deck.name })} testid="deck-row" padding="sm" class="deck-card">
  <span class="top">
    <span class="dot" style:background={node.deck.color ?? 'var(--accent)'} aria-hidden="true"></span>
    <span class="name">{node.deck.name}</span>
    {#if due > 0}
      <span class="due"><span class="num">{formatCount(due)}</span> {t('library.due_short')}</span>
    {/if}
  </span>
  <span class="meta">
    {t('library.cards', { count: node.counts.total })}
    {#if node.children.length > 0}
      <span aria-hidden="true">·</span> {t('library.subdecks', { count: node.children.length })}
    {/if}
    {#if node.counts.new > 0}
      <span aria-hidden="true">·</span> {t('library.new_count', { count: node.counts.new })}
    {/if}
  </span>
  {#if chips && summary && summary.chips.length > 0}
    <span class="chips">
      {#each summary.chips as chip (chip.attr_id + chip.label)}
        <Chip size="sm" label={chip.label} />
      {/each}
    </span>
  {/if}
  <span class="progress"><MasteryBar {summary} name={node.deck.name} long /></span>
</Card>

<style>
  :global(.deck-card.fc-card) {
    display: flex;
    flex-direction: column;
    gap: 6px;
    height: 100%;
  }
  .top {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
  }
  .dot {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: var(--radius-pill);
  }
  .name {
    flex: 1;
    min-width: 0;
    font-size: var(--text-md);
    font-weight: var(--weight-semibold);
    line-height: var(--leading-tight);
    overflow-wrap: anywhere;
  }
  .due {
    flex: none;
    color: var(--accent-text);
    font-size: var(--text-xs);
    font-weight: var(--weight-medium);
  }
  .meta {
    color: var(--text-2);
    font-size: var(--text-xs);
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
  }
  .progress {
    margin-top: auto;
    padding-top: var(--space-1);
  }
</style>
