/**
 * The study launcher (v1.3): ONE dialog every "study" entry opens — Home's button and deck
 * rows, the Library row action, the deck page's "Study" and mode buttons, the palette, a mode
 * page opened on its own. Openers never start a session: they call `openLauncher` with what
 * they know (deck, maybe a mode or scope); the learner sees the counts and presses Start.
 *
 * The launcher host (`LauncherHost.svelte`, mounted by the shell while the study module is on)
 * registers the handler. On Start it starts the session itself (so an empty scope keeps the
 * dialog open with the reason) and hands the session to the mode's screen (`takeHandoff`).
 */
import type { SessionDto } from '$lib/api/types';
import type { LaunchKind, StudyScope } from './contract';

/** What an opener asks for; anything left out comes from the remembered choice. */
export interface LaunchRequest {
  /** Deck (`null` = every deck). */
  deckId: string | null;
  /** Preselected mode (a mode button); otherwise the deck's last mode. */
  kind?: LaunchKind;
  /** Preselected scope ("Study again": struggled today); otherwise the mode's last scope. */
  scope?: StudyScope;
  /**
   * A given card set (query language), e.g. Statistics' "Study these": the launcher opens
   * with the filter filled in (its options shown) instead of a scope.
   */
  query?: string;
  /** Preset the "practice" switch (schedule untouched). */
  practice?: boolean;
}

type Handler = (r: LaunchRequest) => void;

const state = $state<{ handler: Handler | null; seq: number }>({ handler: null, seq: 0 });

/** Reactive: something can open the launcher (the study module is on). */
export const launcher = {
  get available(): boolean {
    return state.handler !== null;
  },
};

/** Registers the launcher (its host); returns the unregister function. */
export function setLauncherHandler(fn: Handler | null): () => void {
  state.handler = fn;
  return () => {
    if (state.handler === fn) state.handler = null;
  };
}

/** Opens the launcher; `false` when nothing can (the study module is off). */
export function openLauncher(r: LaunchRequest): boolean {
  if (!state.handler) return false;
  state.handler(r);
  return true;
}

/** Mode screen → route (the launcher navigates there after a successful start). */
export const KIND_ROUTE: Readonly<Record<LaunchKind, string>> = {
  normal: '/study',
  learn: '/learn',
  // v1.4: the quiz ("Test") runs on the Learn screen (its multiple-choice step) until the
  // approved prototype's screen is built.
  quiz: '/learn',
  test: '/test',
  match: '/match',
};

/** A started (or resumed) session on its way to its screen. */
export interface Handoff {
  kind: LaunchKind;
  session: SessionDto;
  /** The scope it was started with (the summary's "Study again" preselects from it). */
  scope: StudyScope | null;
}

let pending: Handoff | null = null;

/** The launcher leaves a session for the screen it is about to open. */
export function handOff(h: Handoff): void {
  pending = h;
  state.seq++;
}

/**
 * Reactive: changes with every hand-off. A mode screen reads it in an effect, so a session
 * handed to the screen that is already open (the summary's "Study again") still arrives.
 */
export function handoffSeq(): number {
  return state.seq;
}

/** Read-once: the session handed to screen `kind`, if any (one for another screen stays put). */
export function takeHandoff(kind: LaunchKind): Handoff | null {
  const h = pending;
  if (!h || h.kind !== kind) return null;
  pending = null;
  return h;
}

/** The launcher kind of a session (`custom` / `mistakes` sessions run on the Cards screen). */
export function launchKindOf(kind: SessionDto['kind']): LaunchKind {
  return kind === 'learn' || kind === 'quiz' || kind === 'test' || kind === 'match' ? kind : 'normal';
}
