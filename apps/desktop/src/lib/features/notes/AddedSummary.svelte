<!--
  The one confirmation every way of adding cards ends with (v1.3): "12 cards added · Undo ·
  Open deck · Study now", plus what was updated or skipped and why. Undo is the app's normal
  `oplog.undo` (the caller says how many steps its add took); the deck links select the deck
  and open the Library's deck page or the study launcher.
-->
<script lang="ts">
  import { formatCount } from '$lib/format';
  import { t, td } from '$lib/i18n/index.svelte';
  import { BookOpen, Button, CircleAlert, CircleCheck, Icon, Play, Undo2 } from '$lib/ui';
  import type { ScreenCtx } from '../types';

  interface Props {
    ctx: ScreenCtx;
    /** Notes added (each one is a "card" in the UI's words). */
    added: number;
    updated?: number;
    skipped?: number;
    /** Plain-language lines on what was skipped or changed (row numbers included). */
    reasons?: string[];
    /** Deck the cards went to (`null`: several, e.g. a package — no deck links). */
    deckId: string | null;
    /** Reverts the add; `null`: nothing to undo (or it was undone already). */
    onundo?: (() => Promise<void>) | null;
    /** One line, no box (Quick Add). */
    compact?: boolean;
    /** What is said after Undo (default: the added cards are gone). */
    undoneText?: string;
  }
  let { ctx, added, updated = 0, skipped = 0, reasons = [], deckId, onundo = null, compact = false, undoneText }: Props = $props();

  let undoing = $state(false);
  let undone = $state(false);
  let failed = $state<string | null>(null);
  const known = $derived(!!deckId && (ctx.decks.length === 0 || hasDeck(ctx.decks, deckId)));

  function hasDeck(nodes: ScreenCtx['decks'], id: string): boolean {
    return nodes.some((n) => n.deck.id === id || hasDeck(n.children, id));
  }

  async function undo() {
    if (!onundo || undoing) return;
    undoing = true;
    failed = null;
    try {
      await onundo();
      undone = true;
    } catch (e) {
      failed = td((e as { messageKey?: string }).messageKey ?? 'errors.internal.unexpected');
    } finally {
      undoing = false;
    }
  }

  function open(route: string) {
    if (!deckId) return;
    ctx.selectDeck(deckId);
    ctx.navigate?.(route);
  }
</script>

<div class="summary" class:compact class:undone role="status" data-testid="added-summary">
  <p class="line">
    <span class="icon" aria-hidden="true"><Icon icon={undone ? Undo2 : added > 0 || updated > 0 ? CircleCheck : CircleAlert} size={16} /></span>
    <span class="text">
      {#if undone}
        {undoneText ?? t('add.summary.undone')}
      {:else}
        <strong>{t('add.summary.added', { count: added, n: formatCount(added) })}</strong>
        {#if updated > 0}<span class="sep" aria-hidden="true">·</span><span>{t('add.summary.updated', { count: updated, n: formatCount(updated) })}</span>{/if}
        {#if skipped > 0}<span class="sep" aria-hidden="true">·</span><span class="warn">{t('add.summary.skipped', { count: skipped, n: formatCount(skipped) })}</span>{/if}
      {/if}
    </span>
  </p>
  {#if !undone}
    <div class="acts">
      {#if onundo && (added > 0 || updated > 0)}
        <Button size="sm" variant="ghost" icon={Undo2} loading={undoing} onclick={() => void undo()}>{t('add.summary.undo')}</Button>
      {/if}
      {#if known && ctx.navigate && added + updated > 0}
        <Button size="sm" variant="ghost" icon={BookOpen} onclick={() => open('/decks')}>{t('add.summary.open_deck')}</Button>
        <Button size="sm" variant={compact ? 'ghost' : 'secondary'} icon={Play} onclick={() => open('/study')}>{t('add.summary.study')}</Button>
      {/if}
    </div>
  {/if}
  {#if failed}<p class="failed" role="alert">{failed}</p>{/if}
  {#if reasons.length > 0 && !undone}
    <details class="reasons">
      <summary>{t('add.summary.why', { count: reasons.length, n: formatCount(reasons.length) })}</summary>
      <ul>
        {#each reasons as r, i (i)}<li>{r}</li>{/each}
      </ul>
    </details>
  {/if}
</div>

<style>
  .summary {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: var(--space-2) var(--space-3);
    padding: var(--space-3) var(--space-4);
    border: 1px solid var(--border);
    border-left: 3px solid var(--success);
    border-radius: var(--radius-card);
    background: var(--surface-1);
  }
  .summary.undone {
    border-left-color: var(--border-strong);
  }
  .summary.compact {
    padding: 0;
    border: 0;
    background: none;
  }
  .line {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    margin: 0 auto 0 0;
    min-width: 0;
    font-size: var(--text-sm);
    color: var(--text-1);
  }
  .icon {
    display: inline-flex;
    color: var(--success);
  }
  .undone .icon {
    color: var(--text-2);
  }
  .text {
    display: inline-flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: var(--space-1) var(--space-2);
  }
  strong {
    font-weight: var(--weight-semibold);
  }
  .sep {
    color: var(--text-3);
  }
  .warn {
    color: var(--warning);
  }
  .acts {
    display: flex;
    flex-wrap: wrap;
    gap: var(--space-1);
  }
  .failed {
    flex: 1 1 100%;
    margin: 0;
    color: var(--danger);
    font-size: var(--text-sm);
  }
  .reasons {
    flex: 1 1 100%;
    font-size: var(--text-sm);
    color: var(--text-1);
  }
  .reasons summary {
    cursor: pointer;
    color: var(--text-2);
  }
  .reasons ul {
    margin: var(--space-2) 0 0;
    padding-inline-start: var(--space-6);
  }
</style>
