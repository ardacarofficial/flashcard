/**
 * The desktop shell's v0.9 hooks (src-tauri/src/outside.rs): handled `flashcard://` links,
 * clipboard suggestions and the quick-add prefill. Outside Tauri (tests, `pnpm dev:mock`)
 * nothing arrives. No decisions here: the backend already decided what a link means and
 * whether copied text is worth suggesting.
 */
import { isTauri } from '$lib/api/transport';
import type { ApiError, DeepLinkOutcome, DeckId } from '$lib/api/types';

/** One handled link, as the shell queued it. */
export interface LinkResult {
  url: string;
  ok?: DeepLinkOutcome;
  error?: ApiError;
}

export interface Shell {
  /** Outcomes of links handled since the last call (cold start included). */
  takeDeepLinks(): Promise<LinkResult[]>;
  /** Links held while the app is locked (they run after unlock); the lock screen lists them. */
  heldDeepLinks(): Promise<string[]>;
  /** Drops the held links unhandled. */
  discardHeldDeepLinks(): Promise<void>;
  /** Quick-add prefill (clipboard text), if the settings allow it. */
  prefill(): Promise<string | null>;
  /** Listens to a shell event (`fc://deep-link`, `fc://clipboard`, `fc://quick-add`). */
  listen(event: string, cb: (payload: unknown) => void): Promise<() => void>;
  /** Hides the current window (quick add). */
  hide(): Promise<void>;
}

export const DEEP_LINK_EVENT = 'fc://deep-link';
/** A link arrived while locked and is held for after unlock. */
export const DEEP_LINK_HELD_EVENT = 'fc://deep-link-held';
export const CLIPBOARD_EVENT = 'fc://clipboard';
export const QUICK_ADD_EVENT = 'fc://quick-add';

const tauriShell: Shell = {
  async takeDeepLinks() {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<LinkResult[]>('take_deep_links');
  },
  async heldDeepLinks() {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<string[]>('held_deep_links');
  },
  async discardHeldDeepLinks() {
    const { invoke } = await import('@tauri-apps/api/core');
    await invoke<number>('discard_held_deep_links');
  },
  async prefill() {
    const { invoke } = await import('@tauri-apps/api/core');
    return invoke<string | null>('quick_add_prefill');
  },
  async listen(event, cb) {
    const { getCurrentWebviewWindow } = await import('@tauri-apps/api/webviewWindow');
    return getCurrentWebviewWindow().listen(event, (e) => cb(e.payload));
  },
  async hide() {
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    await getCurrentWindow().hide();
  },
};

const noShell: Shell = {
  takeDeepLinks: async () => [],
  heldDeepLinks: async () => [],
  discardHeldDeepLinks: async () => {},
  prefill: async () => null,
  listen: async () => () => {},
  hide: async () => {},
};

export function defaultShell(): Shell {
  return isTauri() ? tauriShell : noShell;
}

/** Where a handled link takes the user: a menu route and optionally a deck to select. */
export interface LinkTarget {
  route: string;
  deck: DeckId | null;
  /** The link proposed a card (it is in the inbox). */
  proposed: boolean;
}

export function linkTarget(outcome: DeepLinkOutcome): LinkTarget {
  if (outcome.kind === 'inbox') return { route: '/inbox', deck: null, proposed: true };
  const t = outcome.target;
  switch (t.kind) {
    case 'deck':
      return { route: '/decks', deck: t.deck_id, proposed: false };
    case 'note':
      return { route: '/notes', deck: null, proposed: false };
    case 'study':
      return { route: t.query ? '/browse' : '/study', deck: t.deck_id, proposed: false };
  }
}
