<!--
  The Library's tree-table (v1.2, the default view): one 34 px row per deck, sub-decks
  indented under an expand chevron, columns cards · due · new · mastery · classification.
  Row actions (study, add card, ⋯) show on hover and keyboard focus. Keys: ↑/↓ Home/End move
  between rows, → expands (or steps into the first sub-deck), ← collapses (or steps to the
  parent), Enter opens the deck page. The collapsed set is a per-viewer convenience
  (`fc.library.collapsed`). Every number is the backend's (`decks.tree`, `decks.summaries`).
-->
<script lang="ts">
  import { formatCount } from '$lib/format';
  import type { DeckId, DeckSummary, DeckTreeNode } from '$lib/api/types';
  import { t } from '$lib/i18n/index.svelte';
  import { Chip, IconButton, ListRow, Menu, Play, Plus, type MenuItem } from '$lib/ui';
  import MasteryBar from './MasteryBar.svelte';

  interface Props {
    nodes: DeckTreeNode[];
    summaries: Map<DeckId, DeckSummary>;
    /** Summaries still loading (bars show a placeholder). */
    loading?: boolean;
    onopen: (id: DeckId) => void;
    onstudy: (node: DeckTreeNode) => void;
    onadd: ((node: DeckTreeNode) => void) | null;
    menu: (node: DeckTreeNode) => MenuItem[];
  }
  let { nodes, summaries, loading = false, onopen, onstudy, onadd, menu }: Props = $props();

  const KEY = 'fc.library.collapsed';
  let collapsed = $state<Record<string, boolean>>(readCollapsed());
  let list = $state<HTMLUListElement | null>(null);

  function readCollapsed(): Record<string, boolean> {
    try {
      const ids: unknown = JSON.parse(localStorage.getItem(KEY) ?? '[]');
      return Array.isArray(ids) ? Object.fromEntries(ids.filter((i) => typeof i === 'string').map((i) => [i, true])) : {};
    } catch {
      return {};
    }
  }
  function toggle(id: DeckId, open: boolean) {
    collapsed[id] = !open;
    try {
      localStorage.setItem(KEY, JSON.stringify(Object.keys(collapsed).filter((k) => collapsed[k])));
    } catch {
      /* per-viewer convenience only */
    }
  }

  type Row = { node: DeckTreeNode; depth: number; parent: DeckId | null };
  const rows = $derived.by(() => {
    const out: Row[] = [];
    const walk = (list: DeckTreeNode[], depth: number, parent: DeckId | null) => {
      for (const node of list) {
        out.push({ node, depth, parent });
        if (node.children.length && !collapsed[node.deck.id]) walk(node.children, depth + 1, node.deck.id);
      }
    };
    walk(nodes, 0, null);
    return out;
  });

  const mains = () => [...(list?.querySelectorAll<HTMLButtonElement>(':scope > li > button.main') ?? [])];
  const focusRow = (i: number) => mains()[Math.max(0, Math.min(i, rows.length - 1))]?.focus();

  function onkeydown(e: KeyboardEvent) {
    const buttons = mains();
    const i = buttons.indexOf(document.activeElement as HTMLButtonElement);
    if (i < 0) return;
    const row = rows[i]!;
    const open = row.node.children.length > 0 && !collapsed[row.node.deck.id];
    let handled = true;
    if (e.key === 'ArrowDown') focusRow(i + 1);
    else if (e.key === 'ArrowUp') focusRow(i - 1);
    else if (e.key === 'Home') focusRow(0);
    else if (e.key === 'End') focusRow(rows.length - 1);
    else if (e.key === 'ArrowRight' && row.node.children.length) {
      if (open) focusRow(i + 1);
      else toggle(row.node.deck.id, true);
    } else if (e.key === 'ArrowLeft') {
      if (open) toggle(row.node.deck.id, false);
      else if (row.parent) focusRow(rows.findIndex((r) => r.node.deck.id === row.parent));
    } else handled = false;
    if (handled) e.preventDefault();
  }

  // A click on the row's cells (not a control) opens the deck too.
  function onclick(e: MouseEvent) {
    const target = e.target as HTMLElement;
    if (target.closest('button, a, [role=menu]')) return;
    const id = target.closest('li')?.querySelector<HTMLElement>('[data-deck-id]')?.dataset.deckId;
    if (id) onopen(id);
  }
</script>

{#snippet cols(node: DeckTreeNode)}
  {@const s = summaries.get(node.deck.id) ?? null}
  {@const due = node.counts.due + node.counts.learning}
  <span class="num n" class:zero={node.counts.total === 0}>{formatCount(node.counts.total)}</span>
  <span class="num n due" class:zero={due === 0}>{formatCount(due)}</span>
  <span class="num n" class:zero={node.counts.new === 0}>{formatCount(node.counts.new)}</span>
  <MasteryBar summary={loading ? null : s} name={node.deck.name} />
  <span class="chips">
    {#each s?.chips ?? [] as chip (chip.attr_id + chip.label)}
      <Chip size="sm" label={chip.label} />
    {/each}
  </span>
{/snippet}

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
<ul class="tree-table" aria-label={t('deck.heading')} bind:this={list} {onkeydown} {onclick}>
  <ListRow header cellColumns="var(--deck-cols)">
    <span class="label">{t('library.col_name')}</span>
    {#snippet cells()}
      <span class="h">{t('library.col_cards')}</span>
      <span class="h">{t('library.col_due')}</span>
      <span class="h">{t('library.col_new')}</span>
      <span class="h start">{t('library.col_mastery')}</span>
      <span class="h start chips">{t('library.col_class')}</span>
    {/snippet}
    {#snippet actions()}<span class="actions-space"></span>{/snippet}
  </ListRow>
  {#each rows as { node, depth } (node.deck.id)}
    {@const open = !collapsed[node.deck.id]}
    <ListRow
      depth={depth}
      expanded={node.children.length ? open : undefined}
      ontoggle={() => toggle(node.deck.id, !open)}
      toggleTabbable={false}
      onclick={() => onopen(node.deck.id)}
      buttonLabel={t('library.open', { name: node.deck.name })}
      cellColumns="var(--deck-cols)"
    >
      <span class="name" data-testid="deck-row" data-deck-id={node.deck.id}>
        <span class="dot" style:background={node.deck.color ?? 'var(--accent)'} aria-hidden="true"></span>
        <span class="label">{node.deck.name}</span>
        {#if node.children.length && !open}
          <span class="sub">{t('library.subdecks', { count: node.children.length })}</span>
        {/if}
      </span>
      {#snippet cells()}{@render cols(node)}{/snippet}
      {#snippet actions()}
        <IconButton size="sm" icon={Play} label={t('library.study', { name: node.deck.name })} onclick={() => onstudy(node)} />
        {#if onadd}
          <IconButton size="sm" icon={Plus} label={t('library.add_card', { name: node.deck.name })} onclick={() => onadd(node)} />
        {/if}
        <Menu items={menu(node)} label={t('deck_page.more')} align="end" triggerClass="row-menu" />
      {/snippet}
    </ListRow>
  {/each}
</ul>

<style>
  .tree-table {
    --deck-cols: 64px 56px 56px 132px 176px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .name {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    min-width: 0;
  }
  .label {
    min-width: 0;
    overflow: hidden;
    font-weight: var(--weight-medium);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .sub {
    flex: none;
    color: var(--text-2);
    font-size: var(--text-xs);
  }
  .dot {
    flex: none;
    width: 8px;
    height: 8px;
    border-radius: var(--radius-pill);
  }
  .n {
    color: var(--text-1);
    font-size: var(--text-xs);
  }
  .due:not(.zero) {
    color: var(--accent-text);
    font-weight: var(--weight-medium);
  }
  .zero {
    color: var(--text-2);
  }
  .h {
    text-align: end;
  }
  .h.start {
    text-align: start;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
    padding-block: 3px;
  }
  .actions-space {
    display: block;
    width: calc(var(--control-h-sm) * 3 + 4px);
  }
  :global(.tree-table .fc-list-row .actions) {
    min-width: calc(var(--control-h-sm) * 3 + 4px);
    justify-content: flex-end;
  }
  :global(.tree-table .row-menu) {
    width: var(--control-h-sm);
    height: var(--control-h-sm);
  }
  @media (max-width: 1023.98px) {
    .tree-table {
      --deck-cols: 56px 48px 48px 112px;
    }
    .chips {
      display: none;
    }
  }
</style>
