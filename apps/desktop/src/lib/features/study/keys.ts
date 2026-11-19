import type { Rating } from '$lib/api/types';

/** Where the study screen is. */
export type StudyPhase = 'idle' | 'question' | 'answer' | 'done';

export type StudyAction =
  | { kind: 'start' }
  | { kind: 'reveal' }
  | { kind: 'rate'; rating: Rating }
  | { kind: 'hint' }
  | { kind: 'suspend' }
  | { kind: 'bury' }
  | { kind: 'undo' }
  | { kind: 'end' }
  | { kind: 'repeat' }
  | { kind: 'back' };

export const RATINGS: readonly Rating[] = ['again', 'hard', 'good', 'easy'];

export interface KeyInput {
  key: string;
  ctrlKey?: boolean;
  metaKey?: boolean;
  altKey?: boolean;
}

/**
 * The study keyboard map (docs/02 §13): Space shows the answer, 1–4 rate, Space/Enter on the
 * answer rate "good", H hint, S suspend, B bury, Z (or Ctrl/⌘+Z) undo, Esc ends. Enter starts
 * a session and leaves the summary; R repeats the mistakes. `typing` = focus is in a text
 * field: then only Escape is ours (the field gets every other key).
 */
export function keyAction(phase: StudyPhase, e: KeyInput, typing = false): StudyAction | null {
  const key = e.key.length === 1 ? e.key.toLowerCase() : e.key;
  const mod = !!(e.ctrlKey || e.metaKey);
  if (key === 'Escape') return phase === 'question' || phase === 'answer' ? { kind: 'end' } : null;
  if (typing || e.altKey) return null;
  if (key === 'z') return phase === 'idle' ? null : { kind: 'undo' };
  if (mod) return null;
  switch (phase) {
    case 'idle':
      return key === 'Enter' ? { kind: 'start' } : null;
    case 'done':
      if (key === 'Enter') return { kind: 'back' };
      if (key === 'r') return { kind: 'repeat' };
      return null;
    case 'question':
    case 'answer': {
      if (key === 'h') return phase === 'question' ? { kind: 'hint' } : null;
      if (key === 's') return { kind: 'suspend' };
      if (key === 'b') return { kind: 'bury' };
      if (phase === 'question') return key === ' ' || key === 'Enter' ? { kind: 'reveal' } : null;
      const n = ['1', '2', '3', '4'].indexOf(key);
      if (n >= 0) return { kind: 'rate', rating: RATINGS[n]! };
      if (key === ' ' || key === 'Enter') return { kind: 'rate', rating: 'good' };
      return null;
    }
  }
}
