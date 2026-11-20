<!-- Tree view of the Library: one row per deck (indented), with due count and mastery bar. -->
<script lang="ts">
  import type { DeckId, DeckSummary, DeckTreeNode } from '$lib/api/types';
  import { t } from '$lib/i18n/index.svelte';
  import { Badge, ChevronDown, ChevronRight, IconButton, ProgressBar } from '$lib/ui';
  import Self from './DeckTreeRows.svelte';

  interface Props {
    nodes: DeckTreeNode[];
    summaries: Map<DeckId, DeckSummary>;
    depth?: number;
    onopen: (id: DeckId) => void;
  }
  let { nodes, summaries, depth = 0, onopen }: Props = $props();

  let collapsed = $state<Record<string, boolean>>({});
</script>

<ul class="rows" aria-label={depth === 0 ? t('deck.heading') : undefined}>
  {#each nodes as node (node.deck.id)}
    {@const s = summaries.get(node.deck.id) ?? null}
    {@const open = !collapsed[node.deck.id]}
    {@const due = node.counts.due + node.counts.learning}
    <li>
      <div class="row" style:padding-inline-start="calc({depth} * var(--space-6) + var(--space-2))">
        {#if node.children.length}
          <IconButton
            size="sm"
            icon={open ? ChevronDown : ChevronRight}
            label={open ? t('deck.collapse') : t('deck.expand')}
            aria-expanded={open}
            onclick={() => (collapsed[node.deck.id] = open)}
          />
        {:else}
          <span class="spacer" aria-hidden="true"></span>
        {/if}
        <button type="button" class="name" data-testid="deck-row" onclick={() => onopen(node.deck.id)}>
          <span class="swatch" style:background={node.deck.color ?? 'var(--accent)'} aria-hidden="true"></span>
          <span class="label">{node.deck.name}</span>
        </button>
        <span class="count">{t('library.cards', { count: node.counts.total })}</span>
        <span class="due">
          {#if due > 0}<Badge tone="accent" variant="soft">{t('library.due', { count: due })}</Badge>{/if}
        </span>
        <span class="bar">
          {#if s}
            <ProgressBar
              size="sm"
              label={t('library.mastery', { name: node.deck.name })}
              max={Math.max(1, s.new + s.learning + s.mastered)}
              segments={[
                { value: s.mastered, label: t('deck_page.mastered'), tone: 'success' },
                { value: s.learning, label: t('deck_page.learning'), tone: 'warning' },
                { value: s.new, label: t('deck_page.new'), tone: 'neutral' },
              ]}
            />
          {/if}
        </span>
        <span class="pct">{s ? t('library.mastered', { pct: s.mastered_pct }) : ''}</span>
      </div>
      {#if node.children.length && open}
        <Self nodes={node.children} {summaries} depth={depth + 1} {onopen} />
      {/if}
    </li>
  {/each}
</ul>

<style>
  .rows {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .row {
    display: grid;
    grid-template-columns: 32px minmax(0, 1fr) 96px 96px minmax(80px, 160px) 112px;
    align-items: center;
    gap: var(--space-3);
    min-height: 48px;
    padding-inline-end: var(--space-3);
    border-bottom: 1px solid var(--border);
  }
  .spacer {
    width: 32px;
  }
  .name {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
    padding: var(--space-2) 0;
    border: 0;
    background: none;
    color: var(--text-1);
    font: inherit;
    font-weight: var(--weight-medium);
    text-align: start;
    cursor: pointer;
  }
  .name:hover .label {
    text-decoration: underline;
  }
  .name:focus-visible {
    outline: 2px solid var(--focus-ring);
    outline-offset: 2px;
    border-radius: var(--radius-control);
  }
  .label {
    overflow-wrap: anywhere;
  }
  .swatch {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: var(--radius-pill);
  }
  .count,
  .pct {
    font-size: var(--text-sm);
    color: var(--text-2);
  }
  .pct {
    text-align: end;
  }
  @media (max-width: 1023.98px) {
    .row {
      grid-template-columns: 32px minmax(0, 1fr) 88px minmax(60px, 120px);
    }
    .count,
    .pct {
      display: none;
    }
  }
</style>
