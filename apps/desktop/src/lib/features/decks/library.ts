/**
 * Small lookups shared by the Library, the deck page and Home (v1.1). They only index what
 * the backend returned (`decks.tree`, `decks.summaries`); every number is computed in Rust.
 */
import type { DeckCounts, DeckId, DeckSummaries, DeckSummary, DeckTreeNode, Mastery, ModuleDto } from '$lib/api/types';
import type { Tone } from '$lib/ui';

/** Every node of the tree by deck id. */
export function nodesById(roots: DeckTreeNode[]): Map<DeckId, DeckTreeNode> {
  const out = new Map<DeckId, DeckTreeNode>();
  const walk = (nodes: DeckTreeNode[]) => {
    for (const n of nodes) {
      out.set(n.deck.id, n);
      walk(n.children);
    }
  };
  walk(roots);
  return out;
}

/** Summaries by deck id. */
export function summariesById(s: DeckSummaries | null): Map<DeckId, DeckSummary> {
  return new Map((s?.decks ?? []).map((d) => [d.deck_id, d]));
}

/** Route of an enabled `main_menu` contribution, by contribution id or route. */
export function menuRoute(modules: ModuleDto[], match: { id?: string; route?: string }): string | null {
  for (const m of modules) {
    if (!m.enabled) continue;
    for (const c of m.ui) {
      if (c.slot !== 'main_menu' || !c.route) continue;
      if ((match.id && c.id === match.id) || (match.route && c.route === match.route)) return c.route;
    }
  }
  return null;
}

/**
 * "To study today" numbers of a deck (or `decks.summaries.today`), exactly as the core sends
 * them: the queue-based `today_*` fields — what a session would serve (v1.4: no fallback, no
 * sum here).
 */
export function todayCounts(c: DeckCounts): { due: number; learning: number; new: number; total: number } {
  return { due: c.today_due, learning: c.today_learning, new: c.today_new, total: c.today_total };
}

/** Whether module `id` is enabled. */
export const moduleOn = (modules: ModuleDto[], id: string): boolean => modules.some((m) => m.id === id && m.enabled);

/** Chip tone of a mastery level (always shown with its text too). */
export const masteryTone: Record<Mastery, Tone> = { new: 'neutral', learning: 'warning', mastered: 'success' };
