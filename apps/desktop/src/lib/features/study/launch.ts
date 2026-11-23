/**
 * Pure helpers of the study launcher (v1.3): what is preselected, what the learner's last
 * choices were (a per-viewer convenience in localStorage, like the Library's view toggle —
 * there is no generic UX-state API to put it behind), how a choice becomes a `study.start`
 * request. Every count and rule is the backend's (`study.options`); nothing here decides
 * what is due.
 */
import type { StartModes, StudyFilter, TestQuestionKind } from '$lib/api/types';
import type { MessageKey } from '$lib/i18n/index.svelte';
import { LAUNCH_KINDS, SCOPES, type LaunchKind, type StartStudy, type StudyEmptyReason, type StudyOptions, type StudyScope, type StudyScopeOption } from './contract';

/** The remembered choices: last mode per deck, last scope per mode. */
export interface LaunchPrefs {
  modeByDeck: Record<string, LaunchKind>;
  scopeByMode: Partial<Record<LaunchKind, StudyScope>>;
}

const PREFS_KEY = 'fc.study.launcher';
const ALL_DECKS = '*';

const isKind = (v: unknown): v is LaunchKind => typeof v === 'string' && (LAUNCH_KINDS as readonly string[]).includes(v);
const isScope = (v: unknown): v is StudyScope => typeof v === 'string' && (SCOPES as readonly string[]).includes(v);

export function emptyPrefs(): LaunchPrefs {
  return { modeByDeck: {}, scopeByMode: {} };
}

/** Parses stored prefs, dropping anything unknown. */
export function parsePrefs(raw: string | null): LaunchPrefs {
  const out = emptyPrefs();
  if (!raw) return out;
  try {
    const v = JSON.parse(raw) as Partial<LaunchPrefs>;
    for (const [deck, kind] of Object.entries(v.modeByDeck ?? {})) if (isKind(kind)) out.modeByDeck[deck] = kind;
    for (const [kind, scope] of Object.entries(v.scopeByMode ?? {})) if (isKind(kind) && isScope(scope)) out.scopeByMode[kind] = scope;
  } catch {
    /* corrupt: start over */
  }
  return out;
}

export function loadPrefs(): LaunchPrefs {
  try {
    return parsePrefs(localStorage.getItem(PREFS_KEY));
  } catch {
    return emptyPrefs();
  }
}

/** Remembers the choice that was just started. */
export function rememberChoice(prefs: LaunchPrefs, deckId: string | null, kind: LaunchKind, scope: StudyScope | null): LaunchPrefs {
  const next: LaunchPrefs = {
    modeByDeck: { ...prefs.modeByDeck, [deckId ?? ALL_DECKS]: kind },
    scopeByMode: scope ? { ...prefs.scopeByMode, [kind]: scope } : { ...prefs.scopeByMode },
  };
  try {
    localStorage.setItem(PREFS_KEY, JSON.stringify(next));
  } catch {
    /* per-viewer convenience only */
  }
  return next;
}

/** A mode's scope when nothing is remembered (each mode's v1.2 behaviour). */
export const DEFAULT_SCOPE: Readonly<Record<LaunchKind, StudyScope>> = {
  normal: 'due',
  learn: 'new',
  // v1.4 Test (quiz) is practice, like the exam: every card of the deck.
  quiz: 'all',
  test: 'all',
  match: 'all',
};

/** The mode shown first: the asked one, else the deck's last one, else Cards — if available. */
export function pickKind(options: Pick<StudyOptions, 'modes'>, asked: LaunchKind | undefined, prefs: LaunchPrefs, deckId: string | null): LaunchKind | null {
  const modes = options.modes.map((m) => m.kind).filter(isKind);
  for (const k of [asked, prefs.modeByDeck[deckId ?? ALL_DECKS], 'normal' as const]) if (k && modes.includes(k)) return k;
  return modes[0] ?? null;
}

/**
 * The scope shown first for `kind`: the asked one, else the mode's last one, else its default
 * — the first of those that has cards; then the first scope that has any; with nothing to
 * study at all, the asked/remembered/default one (shown disabled with its reason).
 */
export function pickScope(scopes: readonly StudyScopeOption[], kind: LaunchKind, asked: StudyScope | undefined, prefs: LaunchPrefs): StudyScope {
  const wanted = [asked, prefs.scopeByMode[kind], DEFAULT_SCOPE[kind]].filter(isScope);
  const count = (s: StudyScope) => scopes.find((o) => o.scope === s)?.count ?? 0;
  return wanted.find((s) => count(s) > 0) ?? SCOPES.find((s) => count(s) > 0) ?? wanted[0]!;
}

/** i18n key of an empty scope's reason ("Günlük yeni kart sınırın doldu"). */
export function emptyReasonKey(scope: StudyScope, reason: StudyEmptyReason): MessageKey {
  // Today's queue holds reviews AND new cards: its limit is "today's limit", not the review one.
  if (reason === 'daily_limit_reached') return scope === 'new' ? 'study.launcher.empty.new_limit' : 'study.launcher.empty.daily_limit';
  return `study.launcher.empty.${reason}` as MessageKey;
}

/** Whole minutes until `at` (at least 1), for "next card in N min". */
export function minutesUntil(at: number, now: number): number {
  return Math.max(1, Math.ceil((at - now) / 60_000));
}

/** The launcher's choice, everything `study.start` needs. */
export interface LaunchChoice {
  kind: LaunchKind;
  scope: StudyScope;
  deckId: string | null;
  /** Cards (Test: questions) at most; `null` = all. */
  limit: number | null;
  /** "Practice" switch for the scheduled scopes (Cards, Learn). */
  practice: boolean;
  /** Query-language filter / saved query / classification ("custom session", Cards). */
  query: string;
  savedQueryId: string;
  attribute: { attrId: string; value: string } | null;
  /** Cards: time limit / rating scale. */
  modes: StartModes | null;
  /** Test: question types (`null` = all). */
  testKinds: TestQuestionKind[] | null;
  /** Test: question count when no limit is given. */
  testQuestions: number;
}

/** Whether the choice narrows the cards with a filter instead of a scope. */
export function isCustom(c: Pick<LaunchChoice, 'kind' | 'query' | 'savedQueryId' | 'attribute'>): boolean {
  return c.query.trim() !== '' || c.savedQueryId !== '' || (c.kind === 'normal' && c.attribute !== null);
}

/** `study.start` of a launcher choice. */
export function startRequest(c: LaunchChoice, scope: StudyScopeOption | undefined, tz: number): StartStudy {
  const custom = isCustom(c);
  const q = c.query.trim();
  const filter: StudyFilter | null =
    custom || (c.limit !== null && c.kind !== 'test')
      ? {
          attributes: c.kind === 'normal' && c.attribute ? [{ attr_id: c.attribute.attrId, values: [c.attribute.value] }] : [],
          // A query decides due-ness itself (`is:due`); the classification picker studies what is due.
          due_only: custom && c.kind === 'normal' && q === '' && c.savedQueryId === '' ? true : null,
          query: c.savedQueryId === '' && q !== '' ? q : null,
          saved_query_id: c.savedQueryId === '' ? null : c.savedQueryId,
          limit: c.kind === 'test' ? null : c.limit,
        }
      : null;
  const scheduled = scope?.mode ?? 'scheduled';
  const practiceAllowed = c.kind === 'normal' || c.kind === 'learn';
  const req: StartStudy = {
    kind: c.kind === 'normal' && custom ? 'custom' : c.kind,
    mode: practiceAllowed && c.practice ? 'trial' : scheduled,
    deck_id: c.deckId,
    filter,
    scope: custom ? null : c.scope,
    tz_offset_mins: tz,
  };
  if (c.kind === 'normal') req.modes = c.modes;
  if (c.kind === 'test') {
    req.modes = { timed: null, rating_scale: null, test: { questions: c.limit ?? c.testQuestions, kinds: c.testKinds && c.testKinds.length ? c.testKinds : null } };
  }
  return req;
}

/** How many cards the session will hold (what the Start button says); null = unknown (filter). */
export function plannedCount(c: Pick<LaunchChoice, 'kind' | 'limit' | 'query' | 'savedQueryId' | 'attribute' | 'testQuestions'>, scope: StudyScopeOption | undefined): number | null {
  if (isCustom(c) || !scope) return null;
  const cap = c.limit ?? (c.kind === 'test' ? c.testQuestions : null);
  return cap === null ? scope.count : Math.min(cap, scope.count);
}

/**
 * v1.4 A/B (prototype `modlar.html`, "Hangi kartlar?"): A = progressive — today's cards only,
 * the other four scopes one click away (opened by itself when another scope is chosen);
 * B = all five always shown. Flip to `false` for B.
 */
export const SCOPES_PROGRESSIVE = true;

const ADVANCED_KEY = 'fc.study.launcher.advanced';

/** The launcher's "Gelişmiş" switch (a per-viewer convenience, like the remembered mode). */
export function loadAdvanced(): boolean {
  try {
    return localStorage.getItem(ADVANCED_KEY) === '1';
  } catch {
    return false;
  }
}

export function rememberAdvanced(on: boolean): void {
  try {
    localStorage.setItem(ADVANCED_KEY, on ? '1' : '0');
  } catch {
    /* per-viewer convenience only */
  }
}
