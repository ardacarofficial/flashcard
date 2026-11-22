/**
 * "On this page" keys (v1.3): the keys a screen handles itself — Home's Enter, the Create
 * page's Ctrl+Enter, the Library's arrows, the test paper's A–D — which the backend's shortcut
 * table (`shortcuts.list`) does not hold. A screen registers them while it is shown
 * (`usePageKeys` in its script); the `?` help sheet lists them first. Labels are i18n keys.
 */
import { onDestroy } from 'svelte';
import type { MessageKey } from '$lib/i18n/index.svelte';

export interface PageKey {
  /** One combo (`'Mod+Enter'`), or keys shown side by side (`['ArrowUp', 'ArrowDown']`). */
  keys: string | string[];
  label_key: MessageKey;
}

type Entry = { get: () => readonly PageKey[] | null };
/** Registrations in creation order (a page, then a panel or session inside it). */
let stack = $state.raw<Entry[]>([]);

/** The keys of the page on screen: the newest registration that has some right now. */
export const pageKeys = {
  get list(): readonly PageKey[] {
    for (let i = stack.length - 1; i >= 0; i--) {
      const keys = stack[i]!.get();
      if (keys) return keys;
    }
    return [];
  },
};

/**
 * Registers the page's keys for as long as the calling component lives (call it during
 * component initialisation). `keys` may be a getter (reactive; `null` = none right now, the
 * enclosing page's keys show instead). A component created later (a deck page inside the
 * Library, a session over a page) wins while it has keys.
 */
export function usePageKeys(keys: readonly PageKey[] | (() => readonly PageKey[] | null)): void {
  const entry: Entry = { get: typeof keys === 'function' ? keys : () => keys };
  stack = [...stack, entry];
  onDestroy(() => {
    stack = stack.filter((e) => e !== entry);
  });
}
