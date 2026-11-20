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
  /** `combo → study action` from the shortcut editor ([`studyBindings`]); absent = defaults. */
  bindings?: ReadonlyMap<string, string>;
}

/**
 * Rebindable study actions (backend `palette::UI_ACTIONS`, context `study`; edited in the
 * shortcut editor) with their default keys, in the backend's combo form.
 */
export const STUDY_DEFAULT_KEYS: Readonly<Record<string, string>> = {
  Space: 'study.reveal',
  '1': 'study.again',
  '2': 'study.hard',
  '3': 'study.good',
  '4': 'study.easy',
  H: 'study.hint',
  S: 'study.suspend',
  B: 'study.bury',
  Z: 'study.undo',
  Escape: 'study.end',
};

const NAMED: Readonly<Record<string, string>> = { ' ': 'Space', Spacebar: 'Space', Esc: 'Escape' };

/** Unmodified key in the backend's combo form (`H`, `Space`, `1`). */
function plainCombo(key: string): string {
  const k = NAMED[key] ?? key;
  return k.length === 1 ? k.toUpperCase() : k;
}

/**
 * `combo → study action`: the defaults, with every study-context item of `shortcuts.list`
 * replacing its action's key (`combo: null` = unbound).
 */
export function studyBindings(items: readonly { action: string; context: string; combo: string | null }[]): Map<string, string> {
  const map = new Map(Object.entries(STUDY_DEFAULT_KEYS));
  const listed = items.filter((i) => i.context === 'study');
  for (const i of listed) for (const [combo, action] of map) if (action === i.action) map.delete(combo);
  for (const i of listed) if (i.combo) map.set(i.combo, i.action);
  return map;
}

/**
 * The study keyboard map (docs/02 §13). Rebindable (defaults): Space shows the answer, 1–4
 * rate, H hint, S suspend, B bury, Z undo, Esc ends — `opts.bindings` (from the shortcut
 * editor) replaces these defaults. Fixed: on the answer the reveal key and Enter rate "good";
 * Enter starts a session, shows the answer and leaves the summary; R repeats the mistakes; on a
 * choice card's question the digits pick an option; Ctrl/⌘+1–7 toggles a flag; Ctrl/⌘+Z
 * undoes only without `bindings` (otherwise it is the shell's global `ui.undo`). `typing` =
 * focus is in a text field: then only the end key is ours.
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
  const bindings = opts.bindings ?? new Map(Object.entries(STUDY_DEFAULT_KEYS));
  const bound = mod || e.altKey ? undefined : bindings.get(plainCombo(e.key));
  if (bound === 'study.end') return card ? { kind: 'end' } : null;
  if (typing || e.altKey) return null;
  const digit = /^[1-9]$/.test(key) ? Number(key) : 0;
  if (mod) {
    if (key === 'z') return opts.bindings || phase === 'idle' ? null : { kind: 'undo' };
    return card && opts.flags && digit >= 1 && digit <= 7 ? { kind: 'flag', flag: digit } : null;
  }
  if (bound === 'study.undo') return phase === 'idle' ? null : { kind: 'undo' };
  switch (phase) {
    case 'idle':
      return key === 'Enter' ? { kind: 'start' } : null;
    case 'done':
      if (key === 'Enter') return { kind: 'back' };
      if (key === 'r') return { kind: 'repeat' };
      return null;
    case 'question':
    case 'answer': {
      if (phase === 'question' && digit >= 1 && digit <= (opts.choices ?? 0)) return { kind: 'choose', index: digit - 1 };
      if (bound === 'study.hint') return phase === 'question' ? { kind: 'hint' } : null;
      if (bound === 'study.suspend') return { kind: 'suspend' };
      if (bound === 'study.bury') return { kind: 'bury' };
      if (phase === 'question') return bound === 'study.reveal' || key === 'Enter' ? { kind: 'reveal' } : null;
      const rating = { 'study.again': 'again', 'study.hard': 'hard', 'study.good': 'good', 'study.easy': 'easy' }[bound ?? ''] as Rating | undefined;
      if (rating) return { kind: 'rate', rating };
      if (bound === 'study.reveal' || key === 'Enter') return { kind: 'rate', rating: 'good' };
      return null;
    }
  }
}
