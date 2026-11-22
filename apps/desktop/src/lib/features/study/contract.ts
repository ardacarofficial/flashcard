/**
 * v1.3 study-launcher wire shapes (`.orchestra/v13/CONTRACT.md` §1 and the hint part of §2).
 *
 * TEMPORARY: R1/R2 implement these in `fc-api` in parallel; once their branch is merged and
 * `cargo run -p fc-api --bin gen-ts` has run, every type below exists in `$lib/api/types`
 * with the same name and shape. Then replace the declarations with
 * `export type { … } from '$lib/api/types'` and drop the casts in `studyOptions` /
 * `startStudy`. Nothing else in the UI imports these shapes from anywhere but this file.
 */
import { api, ApiError } from '$lib/api';
import type { DeckId, HintDto, SessionDto, StartSession, StudyKind, StudyMode, Timestamp } from '$lib/api/types';

/** What a session started now would study (CONTRACT §1; snake_case on the wire). */
export type StudyScope = 'due' | 'new' | 'studied_today' | 'struggled_today' | 'all';

/** Why a scope is empty right now. */
export type StudyEmptyReason =
  | 'no_cards'
  | 'nothing_due'
  | 'daily_limit_reached'
  | 'waiting'
  | 'none_studied_today'
  | 'none_struggled_today';

/** `study.options` request. */
export type StudyOptionsRequest = { deck_id: DeckId | null; include_subdecks: boolean | null };

/** An unfinished session the launcher offers to continue. */
export type ResumableSession = {
  session: SessionDto;
  deck_name: string | null;
  answered: number;
  remaining: number;
};

/** One scope row of the launcher. */
export type StudyScopeOption = {
  scope: StudyScope;
  /** Exactly how many cards a session started now with this scope would contain. */
  count: number;
  /** `scheduled` for due/new; `trial` (practice, schedule untouched) for the rest. */
  mode: StudyMode;
  empty_reason: StudyEmptyReason | null;
  next_available_at: Timestamp | null;
};

/** `study.options` response. */
export type StudyOptions = {
  deck_id: DeckId | null;
  deck_name: string | null;
  /** Unfinished sessions relevant to this deck (all kinds), newest first. */
  resumable: ResumableSession[];
  /** Always all five scopes, in {@link SCOPES} order. */
  scopes: StudyScopeOption[];
  /** Kinds available now (module switches): subset of normal, learn, test, match. */
  modes: StudyKind[];
};

/** `study.start` with the v1.3 `scope` (null = the kind's old behaviour). */
export type StartStudy = StartSession & { scope?: StudyScope | null };

/** `study.hint` with the v1.3 context hint (CONTRACT §2); older backends send neither field. */
export type HintView = HintDto & { kind?: 'context' | 'letters' | null; html?: string | null };

/** `CONFLICT_NOTHING_TO_STUDY` details. */
export type NothingToStudy = { reason: StudyEmptyReason; next_available_at: Timestamp | null };

export const SCOPES: readonly StudyScope[] = ['due', 'new', 'studied_today', 'struggled_today', 'all'];
/** Practice scopes: the session never changes the schedule. */
export const PRACTICE_SCOPES: readonly StudyScope[] = ['studied_today', 'struggled_today', 'all'];
/** The launcher's modes, in display order. */
export const LAUNCH_KINDS = ['normal', 'learn', 'test', 'match'] as const;
export type LaunchKind = (typeof LAUNCH_KINDS)[number];

export const NOTHING_TO_STUDY = 'CONFLICT_NOTHING_TO_STUDY';

/** `study.options` (not yet in the generated method table: see the file comment). */
export function studyOptions(req: StudyOptionsRequest): Promise<StudyOptions> {
  return api.call('study.options' as never, req as never) as Promise<StudyOptions>;
}

/** `study.start` with a scope. */
export function startStudy(req: StartStudy): Promise<SessionDto> {
  return api.call('study.start', req);
}

/** The reason of a `CONFLICT_NOTHING_TO_STUDY` error, or null for any other error. */
export function nothingToStudy(e: unknown): NothingToStudy | null {
  if (!(e instanceof ApiError) || (e.code as string) !== NOTHING_TO_STUDY) return null;
  const d = e.details as Partial<NothingToStudy>;
  return { reason: d.reason ?? 'no_cards', next_available_at: typeof d.next_available_at === 'number' ? d.next_available_at : null };
}
