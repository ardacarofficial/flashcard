/**
 * v1.3 study launcher helpers over the generated wire types (`.orchestra/v13/CONTRACT.md` §1
 * and the hint part of §2). The DTOs themselves come from `fc-api` (`$lib/api/types`); this
 * file only adds the launcher's constants and typed wrappers.
 */
import { api, ApiError } from '$lib/api';
import { tzOffsetMins } from './format';
import type { SessionDto, StartSession, StudyEmptyReason, StudyOptions, StudyOptionsRequest, StudyScope, Timestamp } from '$lib/api/types';

export type {
  HintDto,
  ResumableSession,
  StudyEmptyReason,
  StudyOptions,
  StudyOptionsRequest,
  StudyScope,
  StudyScopeOption,
} from '$lib/api/types';

/** `study.start` request (with the v1.3 `scope`). */
export type StartStudy = StartSession;

/** `CONFLICT_NOTHING_TO_STUDY` details. */
export type NothingToStudy = { reason: StudyEmptyReason; next_available_at: Timestamp | null };

export const SCOPES: readonly StudyScope[] = ['due', 'new', 'studied_today', 'struggled_today', 'all'];
/** Practice scopes: the session never changes the schedule. */
export const PRACTICE_SCOPES: readonly StudyScope[] = ['studied_today', 'struggled_today', 'all'];
/** The launcher's modes, in display order. */
export const LAUNCH_KINDS = ['normal', 'learn', 'test', 'match'] as const;
export type LaunchKind = (typeof LAUNCH_KINDS)[number];

export const NOTHING_TO_STUDY = 'CONFLICT_NOTHING_TO_STUDY';

/** `study.options`. */
export function studyOptions(req: StudyOptionsRequest): Promise<StudyOptions> {
  return api.call('study.options', { tz_offset_mins: tzOffsetMins(), ...req });
}

/** `study.start` with a scope. */
export function startStudy(req: StartStudy): Promise<SessionDto> {
  return api.call('study.start', req);
}

/** The reason of a `CONFLICT_NOTHING_TO_STUDY` error, or null for any other error. */
export function nothingToStudy(e: unknown): NothingToStudy | null {
  if (!(e instanceof ApiError) || e.code !== NOTHING_TO_STUDY) return null;
  const d = (e.details ?? {}) as Partial<NothingToStudy>;
  return { reason: d.reason ?? 'no_cards', next_available_at: typeof d.next_available_at === 'number' ? d.next_available_at : null };
}
