/** Presentation helpers of the v0.10 mode screens (all decisions are the backend's). */
import type { StartModes, TestAnswerDto, TestQuestionDto, TimedOptions, RatingScale } from '$lib/api/types';
import { isEditableTarget } from '$lib/keyboard';

/** Whole seconds left until `deadline` (never negative). */
export function secondsLeft(deadline: number, now: number): number {
  return Math.max(0, Math.ceil((deadline - now) / 1000));
}

/** `study.start.modes` from the start form (`null` = a plain session). */
export function startModes(timed: TimedOptions | null, scale: RatingScale | null): StartModes | null {
  if (!timed && !scale) return null;
  return { timed, rating_scale: scale, test: null };
}

/** What the learner filled in on a test paper, per question index. */
export interface PaperInput {
  choice: Record<number, number>;
  truth: Record<number, boolean>;
  text: Record<number, string>;
  /** v1.4 matching: per word, the option picked for it (`null` = none yet). */
  pairs: Record<number, (number | null)[]>;
}

export function emptyInput(): PaperInput {
  return { choice: {}, truth: {}, text: {}, pairs: {} };
}

/** `modes.test.submit` answers: one per filled question, the field matching its type. */
export function testAnswers(questions: TestQuestionDto[], input: PaperInput): TestAnswerDto[] {
  return questions.flatMap((q): TestAnswerDto[] => {
    const i = q.index;
    switch (q.kind) {
      case 'multiple_choice':
        return i in input.choice ? [{ index: i, choice: input.choice[i]!, text: null, truth: null }] : [];
      case 'true_false':
        return i in input.truth ? [{ index: i, choice: null, text: null, truth: input.truth[i]! }] : [];
      case 'written': {
        const t = input.text[i]?.trim() ?? '';
        return t ? [{ index: i, choice: null, text: t, truth: null }] : [];
      }
      case 'matching': {
        const p = input.pairs[i] ?? [];
        return p.some((x) => x !== null) ? [{ index: i, choice: null, text: null, truth: null, pairs: p }] : [];
      }
    }
  });
}

/** Whether a key event is a plain letter/digit press outside text fields (`$lib/keyboard`). */
export function plainKey(e: KeyboardEvent): boolean {
  return !e.ctrlKey && !e.metaKey && !e.altKey && !isEditableTarget(e.target);
}
