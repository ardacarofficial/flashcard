/**
 * Shell-wide entry points that several places trigger (v1.2): the top bar's search field and
 * "Add card" button, the global `N` key, `Ctrl+N` and the command palette. Each is one
 * function; whoever implements it registers a handler, so callers never import a screen.
 *
 * - `openQuickAdd()` — add a card. The shell's fallback (`setQuickAddFallback`, App.svelte)
 *   opens the Create page (`add_note`); the Quick Add dialog takes over by registering itself
 *   with `setQuickAddHandler` (and gives it back with the returned function).
 * - `openPalette()` — the command palette (ShortcutHost registers it while the `palette`
 *   module is on).
 * - `openHelp()` — the keyboard shortcuts help sheet (`?`, the top bar's "?", the palette;
 *   HelpHost registers it).
 */

type Handler = () => void;
/** What Quick Add opens with: `deckId` preselects that deck (a Library row's "Add card"). */
export type QuickAddOptions = { deckId?: string | null };
type QuickAddHandler = (options?: QuickAddOptions) => void;

const state = $state<{ quickAdd: QuickAddHandler | null; fallback: Handler | null; palette: Handler | null; paletteKeys: string; help: Handler | null }>({
  quickAdd: null,
  fallback: null,
  palette: null,
  paletteKeys: 'Mod+K',
  help: null,
});

/** Reactive availability and key hints for the shell's buttons. */
export const shellCommands = {
  /** Something can add a card (the notes module is on). */
  get canQuickAdd(): boolean {
    return (state.quickAdd ?? state.fallback) !== null;
  },
  /** The palette is available (`palette` module on). */
  get canPalette(): boolean {
    return state.palette !== null;
  },
  /** The shortcuts help sheet is available (HelpHost registered it). */
  get canHelp(): boolean {
    return state.help !== null;
  },
  /** The palette's key combination as `Kbd` takes it (`Ctrl+K` from `shortcuts.list`). */
  get paletteKeys(): string {
    return state.paletteKeys;
  },
};

/** The shell's own "add a card" (opens the Create page); used while no handler is set. */
export function setQuickAddFallback(fn: Handler | null): () => void {
  state.fallback = fn;
  return () => {
    if (state.fallback === fn) state.fallback = null;
  };
}

/** Registers what "add a card" does (the Quick Add dialog); returns the unregister function. */
export function setQuickAddHandler(fn: QuickAddHandler | null): () => void {
  state.quickAdd = fn;
  return () => {
    if (state.quickAdd === fn) state.quickAdd = null;
  };
}

/**
 * Opens Quick Add (the top bar's "Add card", `N`, `Ctrl+N`), on `options.deckId` when given.
 * `false` when nothing can add cards.
 */
export function openQuickAdd(options?: QuickAddOptions): boolean {
  if (state.quickAdd) {
    state.quickAdd(options);
    return true;
  }
  if (!state.fallback) return false;
  state.fallback();
  return true;
}

/** Registers the command palette; returns the unregister function. */
export function setPaletteHandler(fn: Handler | null): () => void {
  state.palette = fn;
  return () => {
    if (state.palette === fn) state.palette = null;
  };
}

/** Sets the palette's key hint (the user may rebind it). */
export function setPaletteKeys(keys: string | null): void {
  state.paletteKeys = keys ?? 'Mod+K';
}

/** Opens the command palette. `false` when the palette module is off. */
export function openPalette(): boolean {
  const fn = state.palette;
  if (!fn) return false;
  fn();
  return true;
}

/** Registers the shortcuts help sheet; returns the unregister function. */
export function setHelpHandler(fn: Handler | null): () => void {
  state.help = fn;
  return () => {
    if (state.help === fn) state.help = null;
  };
}

/** Opens the shortcuts help sheet. `false` when nothing registered it. */
export function openHelp(): boolean {
  const fn = state.help;
  if (!fn) return false;
  fn();
  return true;
}
