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
  /**
   * Opens the page of a `main_menu` route (`'/study'`, `'/decks'`), wherever navigation.ts
   * places it. Does nothing if that module is off. v1.1: screens link to each other with it.
   */
  navigate?: (route: string) => void;
}

/** A screen component (every screen takes the shell's context). */
export type ScreenComponent = Component<{ ctx: ScreenCtx }>;

/**
 * Base fields of a screen. It becomes reachable when an enabled module's `ui`
 * contribution points at it: a `main_menu` item whose `route` is in `routes`, or (for
 * `slot`) any contribution of that slot, which yields a single menu entry (`label_key`).
 */
interface ScreenBase {
  id: string;
  /** `main_menu` routes that open this screen; a trailing `/*` matches a whole prefix. */
  routes?: string[];
  /** Slot whose mere presence adds one menu entry for this screen (e.g. `settings_page`). */
  slot?: { name: string; label_key: string; order: number };
  /** Shell draws the deck picker above the screen. */
  deckPicker?: boolean;
}

/**
 * One screen a feature contributes, either bundled with the shell (`component`) or loaded
 * on first open (`load`, its own chunk — v1.0 code splitting). Only the first screen the
 * shell shows should be eager; everything else uses `load: () => import('./X.svelte')`.
 */
export type ScreenDef = ScreenBase &
  (
    | { component: ScreenComponent; load?: never }
    | { load: () => Promise<{ default: ScreenComponent }>; component?: never }
  );

/** A feature's frontend registration: `features/<feature>/screen.ts` default-exports one. */
export type ScreenModule = ScreenDef[];
