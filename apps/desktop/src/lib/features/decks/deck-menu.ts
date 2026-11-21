/**
 * The deck actions (v1.2) shared by the Library's rows and the deck page: the ⋯ menu (rename,
 * new sub-deck, deck options, export, delete), "study" and "add card". Routes come from the
 * enabled modules (rule 7): an action whose module is off has no entry.
 */
import type { DeckTreeNode, ModuleDto } from '$lib/api/types';
import { t } from '$lib/i18n/index.svelte';
import { openQuickAdd } from '$lib/shell/commands.svelte';
import { Download, Pencil, Plus, SlidersHorizontal, Trash2, type MenuItem } from '$lib/ui';
import type { ScreenCtx } from '../types';
import { menuRoute } from './library';
import { studyNow } from '../study/intent';

/** A dialog the ⋯ menu asks `DeckDialogs` to open. */
export type DeckRequest = { kind: 'rename' | 'sub' | 'delete'; node: DeckTreeNode };

/** Selects `node`'s deck and opens `route` (a study mode, deck options, export…). */
export function goWithDeck(ctx: ScreenCtx, node: DeckTreeNode, route: string | null): void {
  if (!route) return;
  ctx.selectDeck(node.deck.id);
  ctx.navigate?.(route);
}

/** Starts the spaced-repetition session (`/study`) on `node`'s deck. */
export function studyDeck(ctx: ScreenCtx, modules: ModuleDto[], node: DeckTreeNode): void {
  studyNow();
  goWithDeck(ctx, node, menuRoute(modules, { route: '/study' }) ?? '/study');
}

/**
 * Adds a card to `node`'s deck: Quick Add opened on that deck (the page behind stays where it
 * is); without Quick Add, the Create page with the deck selected. `null` when nothing can add
 * cards (the notes module is off).
 */
export function addCardAction(ctx: ScreenCtx, modules: ModuleDto[]): ((node: DeckTreeNode) => void) | null {
  const route = menuRoute(modules, { route: '/notes/new' });
  if (!route && modules.length > 0) return null;
  return (node) => {
    if (!openQuickAdd({ deckId: node.deck.id })) goWithDeck(ctx, node, route);
  };
}

/** The ⋯ menu of a deck. */
export function deckMenu(ctx: ScreenCtx, modules: ModuleDto[], node: DeckTreeNode, request: (r: DeckRequest) => void): MenuItem[] {
  const presets = menuRoute(modules, { route: '/presets' });
  const io = menuRoute(modules, { route: '/io' });
  return [
    { id: 'rename', label: t('deck_page.rename'), icon: Pencil, onselect: () => request({ kind: 'rename', node }) },
    { id: 'sub', label: t('deck_page.new_sub'), icon: Plus, onselect: () => request({ kind: 'sub', node }) },
    ...(presets ? [{ id: 'options', label: t('deck_page.settings'), icon: SlidersHorizontal, onselect: () => goWithDeck(ctx, node, presets) }] : []),
    ...(io ? [{ id: 'export', label: t('deck_page.export'), icon: Download, onselect: () => goWithDeck(ctx, node, io) }] : []),
    { id: 'sep', separator: true },
    { id: 'delete', label: t('deck_page.delete'), icon: Trash2, danger: true, onselect: () => request({ kind: 'delete', node }) },
  ];
}
