import type { Rating } from '$lib/api/types';

/** Where the study screen is. */
export type StudyPhase = 'idle' | 'question' | 'answer' | 'done';

export type StudyAction =
  | { kind: 'start' }
  | { kind: 'reveal' }
  | { kind: 'rate'; rating: Rating }
  | { kind: 'choose'; index: number }
  | { kind: 'flag'; flag: number }
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

/** What the shown card allows. */
export interface KeyOptions {
  /** Options of a choice card (digits 1…n pick one on the question). */
  choices?: number;
  /** The flags module is on (Ctrl/⌘+1–7 toggles flag n, Anki numbering). */
  flags?: boolean;
}

/**
 * The study keyboard map (docs/02 §13): Space shows the answer, 1–4 rate, Space/Enter on the
 * answer rate "good", H hint, S suspend, B bury, Z (or Ctrl/⌘+Z) undo, Esc ends. Enter starts
 * a session and leaves the summary; R repeats the mistakes. On a choice card's question the
 * digits pick an option; Ctrl/⌘+1–7 toggles a flag. `typing` = focus is in a text field: then
 * only Escape is ours (the field gets every other key).
 */
export function keyAction(
  phase: StudyPhase,
  e: KeyInput,
  typing = false,
  opts: KeyOptions = {},
): StudyAction | null {
  const key = e.key.length === 1 ? e.key.toLowerCase() : e.key;
  const mod = !!(e.ctrlKey || e.metaKey);
  const card = phase === 'question' || phase === 'answer';
  if (key === 'Escape') return card ? { kind: 'end' } : null;
  if (typing || e.altKey) return null;
  if (key === 'z') return phase === 'idle' ? null : { kind: 'undo' };
  const digit = /^[1-9]$/.test(key) ? Number(key) : 0;
  if (mod) return card && opts.flags && digit >= 1 && digit <= 7 ? { kind: 'flag', flag: digit } : null;
  if (phase === 'question' && digit >= 1 && digit <= (opts.choices ?? 0)) return { kind: 'choose', index: digit - 1 };
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
