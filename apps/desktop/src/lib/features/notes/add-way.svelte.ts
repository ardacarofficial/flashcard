/**
 * The four ways into "Add cards" (v1.3): one card (Quick Add), a grid (bulk), pasted text and
 * a file. Quick Add and other screens ask the page for a way with {@link requestAddWay} before
 * navigating to it (`ctx.navigate` takes plain routes, no query strings); the page takes the
 * request when it opens, or right away when it is already open.
 */
export const ADD_WAYS = ['single', 'bulk', 'paste', 'file'] as const;
export type AddWay = (typeof ADD_WAYS)[number];

const LAST = 'fc.add.last_way';

const pending = $state<{ way: AddWay | null; deckId: string | null; seq: number }>({ way: null, deckId: null, seq: 0 });

/** Asks the add page to show `way`, on `deckId` when given. */
export function requestAddWay(way: AddWay, deckId: string | null = null): void {
  pending.way = way;
  pending.deckId = deckId;
  pending.seq += 1;
}

/** Reactive: changes with every request (the open page watches it). */
export function addWayRequest(): { way: AddWay | null; deckId: string | null; seq: number } {
  return pending;
}

/** Takes the pending request (once). */
export function takeAddWay(): { way: AddWay; deckId: string | null } | null {
  const way = pending.way;
  if (!way) return null;
  const out = { way, deckId: pending.deckId };
  pending.way = null;
  pending.deckId = null;
  return out;
}

/** The way used last (the page reopens on it; never `single`, which is a dialog). */
export function lastAddWay(): AddWay {
  try {
    const v = localStorage.getItem(LAST);
    return v === 'bulk' || v === 'paste' || v === 'file' ? v : 'bulk';
  } catch {
    return 'bulk';
  }
}

export function rememberAddWay(way: AddWay): void {
  if (way === 'single') return;
  try {
    localStorage.setItem(LAST, way);
  } catch {
    /* ignore */
  }
}
