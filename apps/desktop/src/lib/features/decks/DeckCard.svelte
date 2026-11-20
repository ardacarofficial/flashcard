<!--
  A deck as a card (Library grid, Home's recent decks): name, card and sub-deck counts, due
  count, classification chips and the New / Learning / Mastered bar. Every number comes from
  the backend (`decks.tree` counts, `decks.summaries`); `summary` is null while it loads.
-->
<script lang="ts">
  import type { DeckSummary, DeckTreeNode } from '$lib/api/types';
  import { t } from '$lib/i18n/index.svelte';
  import { Badge, Card, Chip, ProgressBar, Skeleton } from '$lib/ui';

  interface Props {
    node: DeckTreeNode;
    summary: DeckSummary | null;
    onopen: () => void;
    /** Show the classification chips (off on Home: fewer things per card). */
    chips?: boolean;
  }
  let { node, summary, onopen, chips = true }: Props = $props();

  const due = $derived(node.counts.due + node.counts.learning);
</script>

<Card onclick={onopen} label={t('library.open', { name: node.deck.name })} testid="deck-row" padding="md" class="deck-card">
  <span class="top">
    <span class="swatch" style:background={node.deck.color ?? 'var(--accent)'} aria-hidden="true"></span>
    <span class="name">{node.deck.name}</span>
    {#if due > 0}
      <Badge tone="accent" variant="soft">{t('library.due', { count: due })}</Badge>
    {/if}
  </span>
  <span class="meta">
    {t('library.cards', { count: node.counts.total })}
    {#if node.children.length > 0}
      <span aria-hidden="true">·</span> {t('library.subdecks', { count: node.children.length })}
    {/if}
  </span>
  {#if chips && summary && summary.chips.length > 0}
    <span class="chips">
      {#each summary.chips as chip (chip.attr_id + chip.label)}
        <Chip size="sm" label={chip.label} />
      {/each}
    </span>
  {/if}
  <span class="progress">
    {#if summary}
      <ProgressBar
        size="sm"
        label={t('library.mastery', { name: node.deck.name })}
        max={Math.max(1, summary.new + summary.learning + summary.mastered)}
        segments={[
          { value: summary.mastered, label: t('deck_page.mastered'), tone: 'success' },
          { value: summary.learning, label: t('deck_page.learning'), tone: 'warning' },
          { value: summary.new, label: t('deck_page.new'), tone: 'neutral' },
        ]}
      />
      <span class="pct">{t('library.mastered', { pct: summary.mastered_pct })}</span>
    {:else}
      <Skeleton shape="text" width="100%" />
    {/if}
  </span>
</Card>

<style>
  :global(.deck-card.fc-card) {
    display: flex;
    flex-direction: column;
    gap: var(--space-2);
    height: 100%;
  }
  .top {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
  }
  .swatch {
    flex: none;
    width: 10px;
    height: 10px;
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
  .meta {
    font-size: var(--text-sm);
    color: var(--text-2);
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
  }
  .progress {
    display: flex;
    flex-direction: column;
    gap: var(--space-1);
    margin-top: auto;
    padding-top: var(--space-2);
  }
  .pct {
    font-size: var(--text-xs);
    color: var(--text-2);
  }
</style>
