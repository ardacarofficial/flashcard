/**
 * Pure helpers of the keyboard shortcuts help sheet (v1.3). The table itself is the backend's
 * (`shortcuts.list`, the same source the Shortcuts screen edits): this file only groups the
 * bound rows by where they work and filters them. Nothing here knows which shortcuts exist.
 */
import type { ShortcutList } from '$lib/api/types';
import { filterItems, type PaletteItem } from '../browse/palette';

/** One bound shortcut as the sheet shows it. */
export interface HelpRow {
  action: string;
  label: string;
  /** Effective combination in the backend's form (`Ctrl+Shift+Z`), as `Kbd` takes it. */
  combo: string;
}

/** The rows of one context (`global`, `study`, `browse`…), in the backend's order. */
export interface HelpGroup {
  context: string;
  rows: HelpRow[];
}

/**
 * Known contexts in display order: everywhere, studying, lists, editor. Any other context
 * (one the backend adds later) follows them in first-seen order.
 */
export const CONTEXT_ORDER: readonly string[] = ['global', 'study', 'browse', 'editor'];

/**
 * Bound shortcuts of `list` grouped by context (unbound rows are left out), filtered by
 * `query` over label, action id and combination (every word, case/accent-insensitive).
 * `label(label_key, action)` resolves a row's display name. Empty groups are dropped.
 */
export function helpGroups(
  list: Pick<ShortcutList, 'items'>,
  label: (key: string, action: string) => string,
  query: string,
): HelpGroup[] {
  const bound: (HelpRow & { context: string })[] = [];
  for (const i of list.items) {
    if (!i.combo) continue;
    bound.push({ action: i.action, label: label(i.label_key, i.action), combo: i.combo, context: i.context });
  }
  // The palette's matcher over label, id (the action) and the screen `hint` (the combination).
  // It returns the very item objects it was given, so rows are matched back by identity.
  const asItems: PaletteItem[] = bound.map((r) => ({ kind: 'screen', id: r.action, label: r.label, screenId: r.action, hint: r.combo }));
  const kept = new Set(filterItems(asItems, query));

  const byContext = new Map<string, HelpRow[]>();
  bound.forEach((r, n) => {
    if (!byContext.has(r.context)) byContext.set(r.context, []);
    if (kept.has(asItems[n]!)) byContext.get(r.context)!.push({ action: r.action, label: r.label, combo: r.combo });
  });
  const known = CONTEXT_ORDER.filter((c) => byContext.has(c));
  const others = [...byContext.keys()].filter((c) => !CONTEXT_ORDER.includes(c));
  return [...known, ...others]
    .map((context) => ({ context, rows: byContext.get(context)! }))
    .filter((g) => g.rows.length > 0);
}

/** Number of bound shortcuts in `list` (0 → the sheet says nothing is bound yet). */
export function boundCount(list: Pick<ShortcutList, 'items'>): number {
  return list.items.filter((i) => !!i.combo).length;
}
