import type { Rating } from '$lib/api/types';

/**
 * Where the Cards screen is. `question`: the back has not been seen yet; `answer`: it has
 * (at least once — the learner may have flipped back to the front); `waiting`: the session is
 * open but every remaining card waits for its learning step.
 */
export type StudyPhase = 'idle' | 'question' | 'answer' | 'waiting' | 'done';

export type StudyAction =
  | { kind: 'flip' }
  | { kind: 'rate'; rating: Rating }
  | { kind: 'choose'; index: number }
  | { kind: 'flag'; flag: number }
  | { kind: 'hint' }
  | { kind: 'suspend' }
  | { kind: 'bury' }
  | { kind: 'undo' }
  | { kind: 'end' }
  | { kind: 'repeat' };

export const RATINGS: readonly Rating[] = ['again', 'hard', 'good', 'easy'];

export interface KeyInput {
  key: string;
  ctrlKey?: boolean;
  metaKey?: boolean;
  altKey?: boolean;
  /** The key is held down (auto-repeat): never an action. */
  repeat?: boolean;
}

/** What the shown card allows. */
export interface KeyOptions {
  /** Options of a choice card (digits 1…n pick one on the question). */
  choices?: number;
  /** The flags module is on (Ctrl/⌘+1–7 toggles flag n, Anki numbering). */
  flags?: boolean;
  /** `combo → study action` from the shortcut editor ([`studyBindings`]); absent = defaults. */
  bindings?: ReadonlyMap<string, string>;
  /**
   * v1.1 two-button display (`study.answer_buttons = two`): the first rating key answers
   * "Still learning" (again), the second "I know it" (good); the third and fourth do nothing.
   */
  twoButtons?: boolean;
}

/**
 * Rebindable study actions (backend `palette::UI_ACTIONS`, context `study`; edited in the
 * shortcut editor) with their default keys, in the backend's combo form. v1.3: the action id
 * `study.reveal` is kept (users' stored overrides keep working) but its job is now "flip the
 * card" — it toggles front ⇄ back and never grades.
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

/** The key bound to `action` (`'Space'`, `'1'`, `'J'`), or null when it is unbound. */
export function keyFor(action: string, bindings?: ReadonlyMap<string, string>): string | null {
  for (const [combo, a] of bindings ?? Object.entries(STUDY_DEFAULT_KEYS)) if (a === action) return combo;
  return null;
}

/** `aria-keyshortcuts` form of a combo (`Space`, `Control+1`); undefined when unbound. */
export function ariaKeys(combo: string | null): string | undefined {
  return combo ? combo.replace(/\bCtrl\b/g, 'Control') : undefined;
}

/** Which rating each rating action gives on the current display (`null` = no rating). */
export function ratingOf(action: string | undefined, twoButtons = false): Rating | null {
  switch (action) {
    case 'study.again':
      return 'again';
    case 'study.hard':
      return twoButtons ? 'good' : 'hard';
    case 'study.good':
      return twoButtons ? null : 'good';
    case 'study.easy':
      return twoButtons ? null : 'easy';
    default:
      return null;
  }
}

/**
 * The Cards keyboard model (v1.3; docs/02 §13). One key, one job:
 * - the flip key (default Space) turns the card, front ⇄ back, as often as the learner
 *   likes; it never grades;
 * - the rating keys (default 1/2 with two buttons, 1–4 with four) grade — only once the back
 *   has been seen; before that they do nothing (on a choice card's question the digits pick
 *   an option instead, and the options are on screen);
 * - H hint (question only), S "pause", B "tomorrow", Z undo the last grade, Esc leave;
 *   Ctrl/⌘+1–7 toggles a flag; Ctrl/⌘+Z undoes only while the shell has no bindings (it is
 *   otherwise the shell's global `ui.undo`, which does the same);
 * - Enter is never a page shortcut: it activates the focused control (the card flips when it
 *   has focus, a button presses);
 * - a held key repeats nothing (`repeat`), arrows are not ours, and while typing
 *   (`typing` = focus in a text field) only the leave key is.
 * On the summary R repeats the mistakes.
 */
export function keyAction(phase: StudyPhase, e: KeyInput, typing = false, opts: KeyOptions = {}): StudyAction | null {
  if (e.repeat) return null;
  const key = e.key.length === 1 ? e.key.toLowerCase() : e.key;
  const mod = !!(e.ctrlKey || e.metaKey);
  const card = phase === 'question' || phase === 'answer';
  const open = card || phase === 'waiting';
  const bindings = opts.bindings ?? new Map(Object.entries(STUDY_DEFAULT_KEYS));
  const bound = mod || e.altKey ? undefined : bindings.get(plainCombo(e.key));
  if (bound === 'study.end') return open ? { kind: 'end' } : null;
  if (typing || e.altKey) return null;
  const digit = /^[1-9]$/.test(key) ? Number(key) : 0;
  if (mod) {
    if (key === 'z') return opts.bindings || phase === 'idle' ? null : { kind: 'undo' };
    return card && opts.flags && digit >= 1 && digit <= 7 ? { kind: 'flag', flag: digit } : null;
  }
  if (bound === 'study.undo') return phase === 'idle' ? null : { kind: 'undo' };
  if (phase === 'done') return key === 'r' ? { kind: 'repeat' } : null;
  if (!card) return null;
  if (phase === 'question' && digit >= 1 && digit <= (opts.choices ?? 0)) return { kind: 'choose', index: digit - 1 };
  if (bound === 'study.reveal') return { kind: 'flip' };
  if (bound === 'study.hint') return phase === 'question' ? { kind: 'hint' } : null;
  if (bound === 'study.suspend') return { kind: 'suspend' };
  if (bound === 'study.bury') return { kind: 'bury' };
  const rating = phase === 'answer' ? ratingOf(bound, opts.twoButtons) : null;
  return rating ? { kind: 'rate', rating } : null;
}
