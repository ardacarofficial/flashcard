import type { Component } from 'svelte';
import type { DeckTreeNode } from '$lib/api/types';

/** What the shell hands every screen. Screens never reach into App state directly. */
export interface ScreenCtx {
  decks: DeckTreeNode[];
  selectedDeck: string | null;
  /** Full `A::B` path of the selected deck. */
  selectedPath: string | null;
  selectDeck: (id: string | null) => void;
  /** Runs an API call, shows its error in the shell, resolves to whether it succeeded. */
  guarded: (fn: () => Promise<unknown>) => Promise<boolean>;
  /** Route of the menu entry that opened the screen (plugin views read their target here). */
  route?: string | null;
}

/**
 * One screen a feature contributes. It becomes reachable when an enabled module's `ui`
 * contribution points at it: a `main_menu` item whose `route` is in `routes`, or (for
 * `slot`) any contribution of that slot, which yields a single menu entry (`label_key`).
 */
export interface ScreenDef {
  id: string;
  component: Component<{ ctx: ScreenCtx }>;
  /** `main_menu` routes that open this screen; a trailing `/*` matches a whole prefix. */
  routes?: string[];
  /** Slot whose mere presence adds one menu entry for this screen (e.g. `settings_page`). */
  slot?: { name: string; label_key: string; order: number };
  /** Shell draws the deck picker above the screen. */
  deckPicker?: boolean;
}

/** A feature's frontend registration: `features/<feature>/screen.ts` default-exports one. */
export type ScreenModule = ScreenDef[];
