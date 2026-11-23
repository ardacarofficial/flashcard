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
  navigate?: (route: string, where?: 'settings') => void;
  /**
   * Where the page is shown (v1.3): a sidebar section, Settings or the Inbox — a page placed
   * in two places (AI: Create and Settings) shows the part that belongs there. `where:
   * 'settings'` in {@link navigate} opens a route's Settings place.
   */
  place?: 'section' | 'settings' | 'inbox' | null;
  /**
   * A study screen says a session is running. The shell then switches to the focus layout —
   * v1.2: no sidebar, no top bar, no deck picker or tabs; the screen draws its own session bar
   * (`modes/SessionFrame`). Reset on every page change.
   */
  setFocus?: (on: boolean) => void;
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
  /**
   * v1.4: a Settings page of the shell's own (rows from several modules' settings), placed by
   * `route` (navigation.ts) and shown while at least one of `modules` is enabled.
   */
  settingsPage?: { route: string; label_key: string; modules: string[] };
  /**
   * Shell adds the deck picker to the screen's header (PageHeader actions). `'all'`: no
   * selection means every deck (study sessions), so the empty choice reads "All decks".
   */
  deckPicker?: boolean | 'all';
  /** `wide`: the page uses `--content-max-wide` (tables, charts) instead of `--content-max`. */
  width?: 'wide';
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
