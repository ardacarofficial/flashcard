/**
 * Pure helpers of the command palette and keyboard shortcuts. The backend decides what is
 * listed (`palette.list`: every public method of the registry) and what is bound
 * (`shortcuts.list`); this file only turns that into UI items and maps key events.
 */
import type { PaletteEntry, ShortcutDto } from '$lib/api/types';

/** One palette row: a screen of the menu or a backend method. */
export type PaletteItem =
  | { kind: 'screen'; id: string; label: string; screenId: string }
  | { kind: 'method'; id: string; label: string; entry: PaletteEntry };

/** Menu screens first, then every enabled method (label falls back to the method name). */
export function paletteItems(
  entries: readonly PaletteEntry[],
  screens: readonly { id: string; label: string }[],
  translate: (key: string) => string,
): PaletteItem[] {
  const items: PaletteItem[] = screens.map((s) => ({
    kind: 'screen',
    id: `screen:${s.id}`,
    label: s.label,
    screenId: s.id,
  }));
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

const fold = (s: string) =>
  s
    .toLocaleLowerCase('tr')
    .normalize('NFD')
    .replace(/[̀-ͯ]/g, '')
    .replace(/ı/g, 'i');

/** Items whose label or id contains every word of `query` (accent/case-insensitive). */
export function filterItems(items: readonly PaletteItem[], query: string): PaletteItem[] {
  const words = fold(query).split(/\s+/).filter(Boolean);
  if (words.length === 0) return [...items];
  return items.filter((i) => {
    const hay = fold(`${i.label} ${i.id}`);
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
