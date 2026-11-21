/**
 * Pure helpers of the command palette and keyboard shortcuts. The backend decides what is
 * listed (`palette.list`: every public method of the registry) and what is bound
 * (`shortcuts.list`); this file only turns that into UI items and maps key events.
 */
import type { PaletteEntry, ShortcutDto } from '$lib/api/types';

/** One palette row: a shell action (add a card, undo…), a screen of the menu or a backend method. */
export type PaletteItem =
  | { kind: 'action'; id: string; label: string; action: string; keys?: string | null }
  | { kind: 'screen'; id: string; label: string; screenId: string; keys?: string | null; hint?: string | null }
  | { kind: 'method'; id: string; label: string; entry: PaletteEntry };

/** A shell action offered at the top of the palette (`ui.new_note` → "Add card"). */
export interface PaletteAction {
  action: string;
  label: string;
  keys?: string | null;
}

/**
 * Shell actions first, then menu screens, then every enabled method (label falls back to the
 * method name). Order within each kind is the order given (the backend's for methods).
 */
export function paletteItems(
  entries: readonly PaletteEntry[],
  /** `hint`: where the screen lives (sidebar section, "Settings"): shown muted and searched too. */
  screens: readonly { id: string; label: string; keys?: string | null; hint?: string | null }[],
  translate: (key: string) => string,
  actions: readonly PaletteAction[] = [],
): PaletteItem[] {
  const items: PaletteItem[] = actions.map((a) => ({
    kind: 'action',
    id: `action:${a.action}`,
    label: a.label,
    action: a.action,
    keys: a.keys ?? null,
  }));
  for (const s of screens) {
    const hint = s.hint && s.hint !== s.label ? s.hint : null;
    items.push({ kind: 'screen', id: `screen:${s.id}`, label: s.label, screenId: s.id, keys: s.keys ?? null, hint });
  }
  for (const entry of entries) {
    if (!entry.enabled) continue;
    const label = translate(entry.label_key);
    items.push({
      kind: 'method',
      id: entry.method,
      label: label === entry.label_key ? entry.method : label,
      entry,
    });
  }
  return items;
}

/** One titled block of palette rows; `start` is the index of its first row in the flat list. */
export interface PaletteGroup {
  title: string;
  start: number;
  items: PaletteItem[];
}

/**
 * Display grouping (no ranking): actions, screens, then methods under their module's name
 * (`groupTitle(module)`), modules in first-seen order. Modules without a name share one block.
 * Returns the groups and the rows flattened in display order (for arrow-key navigation).
 */
export function groupItems(
  items: readonly PaletteItem[],
  titles: { actions: string; screens: string; groupTitle: (module: string) => string },
): { groups: PaletteGroup[]; flat: PaletteItem[] } {
  const byTitle = new Map<string, PaletteItem[]>();
  for (const i of items) {
    const title = i.kind === 'action' ? titles.actions : i.kind === 'screen' ? titles.screens : titles.groupTitle(i.entry.module);
    const list = byTitle.get(title);
    if (list) list.push(i);
    else byTitle.set(title, [i]);
  }
  const groups: PaletteGroup[] = [];
  const flat: PaletteItem[] = [];
  for (const [title, list] of byTitle) {
    groups.push({ title, start: flat.length, items: list });
    flat.push(...list);
  }
  return { groups, flat };
}

const fold = (s: string) =>
  s
    .toLocaleLowerCase('tr')
    .normalize('NFD')
    .replace(/[̀-ͯ]/g, '')
    .replace(/ı/g, 'i');

/**
 * Items whose label, id or (screens) place contains every word of `query`
 * (accent/case-insensitive): "Library" finds the Library's pages, "Settings" every settings page.
 */
export function filterItems(items: readonly PaletteItem[], query: string): PaletteItem[] {
  const words = fold(query).split(/\s+/).filter(Boolean);
  if (words.length === 0) return [...items];
  return items.filter((i) => {
    const hay = fold(`${i.label} ${i.id} ${i.kind === 'screen' ? (i.hint ?? '') : ''}`);
    return words.every((w) => hay.includes(w));
  });
}

const NAMED: Record<string, string> = {
  ' ': 'Space',
  Spacebar: 'Space',
  Esc: 'Escape',
  Del: 'Delete',
  Up: 'ArrowUp',
  Down: 'ArrowDown',
  Left: 'ArrowLeft',
  Right: 'ArrowRight',
};

/**
 * Canonical combination of a key event, in the backend's form (`fc_core::shortcut`):
 * `Ctrl+Alt+Shift+Meta+Key`, letters upper case. `null` for a lone modifier.
 */
export function comboOf(e: Pick<KeyboardEvent, 'key' | 'code' | 'ctrlKey' | 'altKey' | 'shiftKey' | 'metaKey'>): string | null {
  if (['Control', 'Alt', 'Shift', 'Meta', 'AltGraph', 'CapsLock'].includes(e.key)) return null;
  let key = NAMED[e.key] ?? e.key;
  // With Shift/Alt the produced character differs by layout; prefer the physical key.
  if (/^Key[A-Z]$/.test(e.code)) key = e.code.slice(3);
  else if (/^Digit[0-9]$/.test(e.code)) key = e.code.slice(5);
  if (key.length === 1) key = key.toUpperCase();
  const mods = [e.ctrlKey && 'Ctrl', e.altKey && 'Alt', e.shiftKey && 'Shift', e.metaKey && 'Meta'];
  return [...mods.filter(Boolean), key].join('+');
}

/** `combo → actions` for the given contexts (`global` is always active). */
export function bindingMap(items: readonly ShortcutDto[], contexts: readonly string[]): Map<string, string[]> {
  const map = new Map<string, string[]>();
  for (const i of items) {
    if (!i.combo) continue;
    if (i.context !== 'global' && !contexts.includes(i.context)) continue;
    map.set(i.combo, [...(map.get(i.combo) ?? []), i.action]);
  }
  return map;
}

/** Whether a key event targets a text input (plain keys then belong to the input). */
export function typingTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  return target.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(target.tagName);
}
