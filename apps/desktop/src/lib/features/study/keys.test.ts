import { describe, expect, it } from 'vitest';
import { ariaKeys, keyAction, keyFor, ratingOf, studyBindings } from './keys';
import { intervalLabel } from './format';

// v1.3 keyboard model (gpt-review-2 §3): one key, one job. Changed deliberately from v1.2:
// Space on the answer side no longer rates "good", Enter is no page shortcut any more (it
// presses the focused control), and in the two-button display 3/4 do nothing.
describe('study keymap', () => {
  it('Space flips, on both sides, and never grades', () => {
    expect(keyAction('question', { key: ' ' })).toEqual({ kind: 'flip' });
    expect(keyAction('answer', { key: ' ' })).toEqual({ kind: 'flip' });
    expect(keyAction('answer', { key: ' ' }, false, { twoButtons: true })).toEqual({ kind: 'flip' });
    for (const phase of ['question', 'answer'] as const) {
      expect(keyAction(phase, { key: ' ' })).not.toMatchObject({ kind: 'rate' });
    }
  });

  it('rating keys only work once the back has been seen', () => {
    for (const k of ['1', '2', '3', '4']) expect(keyAction('question', { key: k })).toBeNull();
    expect(keyAction('answer', { key: '1' })).toEqual({ kind: 'rate', rating: 'again' });
    expect(keyAction('answer', { key: '2' })).toEqual({ kind: 'rate', rating: 'hard' });
    expect(keyAction('answer', { key: '3' })).toEqual({ kind: 'rate', rating: 'good' });
    expect(keyAction('answer', { key: '4' })).toEqual({ kind: 'rate', rating: 'easy' });
  });

  it('a held key is one action', () => {
    expect(keyAction('question', { key: ' ', repeat: true })).toBeNull();
    expect(keyAction('answer', { key: '1', repeat: true })).toBeNull();
    expect(keyAction('answer', { key: 'z', repeat: true })).toBeNull();
    expect(keyAction('question', { key: 'Escape', repeat: true })).toBeNull();
  });

  it('Enter and the arrows are not study shortcuts', () => {
    for (const phase of ['idle', 'question', 'answer', 'waiting', 'done'] as const) {
      expect(keyAction(phase, { key: 'Enter' })).toBeNull();
      for (const k of ['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown']) expect(keyAction(phase, { key: k })).toBeNull();
    }
  });

  it('two-button display: 1 = still learning, 2 = I know it, 3/4 nothing', () => {
    const two = { twoButtons: true };
    expect(keyAction('answer', { key: '1' }, false, two)).toEqual({ kind: 'rate', rating: 'again' });
    expect(keyAction('answer', { key: '2' }, false, two)).toEqual({ kind: 'rate', rating: 'good' });
    expect(keyAction('answer', { key: '3' }, false, two)).toBeNull();
    expect(keyAction('answer', { key: '4' }, false, two)).toBeNull();
    expect(ratingOf('study.hard', true)).toBe('good');
    expect(ratingOf('study.hard', false)).toBe('hard');
  });

  it('card tools, undo, leave, summary', () => {
    expect(keyAction('question', { key: 'h' })).toEqual({ kind: 'hint' });
    expect(keyAction('answer', { key: 'h' })).toBeNull();
    expect(keyAction('answer', { key: 's' })).toEqual({ kind: 'suspend' });
    expect(keyAction('question', { key: 'B' })).toEqual({ kind: 'bury' });
    expect(keyAction('answer', { key: 'z' })).toEqual({ kind: 'undo' });
    expect(keyAction('done', { key: 'z', ctrlKey: true })).toEqual({ kind: 'undo' });
    expect(keyAction('idle', { key: 'z' })).toBeNull();
    expect(keyAction('question', { key: 'Escape' })).toEqual({ kind: 'end' });
    expect(keyAction('done', { key: 'R' })).toEqual({ kind: 'repeat' });
    expect(keyAction('done', { key: 'Escape' })).toBeNull();
  });

  it('waiting: only leave and undo', () => {
    expect(keyAction('waiting', { key: 'Escape' })).toEqual({ kind: 'end' });
    expect(keyAction('waiting', { key: 'z' })).toEqual({ kind: 'undo' });
    expect(keyAction('waiting', { key: ' ' })).toBeNull();
    expect(keyAction('waiting', { key: '1' })).toBeNull();
  });

  it('leaves keys alone while typing or with modifiers', () => {
    expect(keyAction('question', { key: 'h' }, true)).toBeNull();
    expect(keyAction('answer', { key: '3' }, true)).toBeNull();
    expect(keyAction('question', { key: ' ' }, true)).toBeNull();
    expect(keyAction('question', { key: 'Escape' }, true)).toEqual({ kind: 'end' });
    expect(keyAction('answer', { key: '3', ctrlKey: true })).toBeNull();
    expect(keyAction('answer', { key: '3', altKey: true })).toBeNull();
  });

  it('picks choice options and toggles flags', () => {
    expect(keyAction('question', { key: '2' }, false, { choices: 3 })).toEqual({ kind: 'choose', index: 1 });
    expect(keyAction('question', { key: '4' }, false, { choices: 3 })).toBeNull();
    expect(keyAction('answer', { key: '2' }, false, { choices: 3 })).toEqual({ kind: 'rate', rating: 'hard' });
    expect(keyAction('answer', { key: '7', ctrlKey: true }, false, { flags: true })).toEqual({ kind: 'flag', flag: 7 });
    expect(keyAction('question', { key: '5', metaKey: true }, false, { flags: true })).toEqual({ kind: 'flag', flag: 5 });
    expect(keyAction('question', { key: '8', ctrlKey: true }, false, { flags: true })).toBeNull();
    expect(keyAction('idle', { key: '1', ctrlKey: true }, false, { flags: true })).toBeNull();
    expect(keyAction('answer', { key: '3', ctrlKey: true }, false, { flags: false })).toBeNull();
  });
});

describe('rebound study keys (shortcut editor)', () => {
  const bindings = studyBindings([
    { action: 'study.reveal', context: 'study', combo: 'J' },
    { action: 'study.good', context: 'study', combo: 'G' },
    { action: 'study.end', context: 'study', combo: 'Q' },
    { action: 'study.undo', context: 'study', combo: 'U' },
    { action: 'study.hint', context: 'study', combo: null },
    { action: 'ui.palette', context: 'global', combo: 'Ctrl+K' },
  ]);
  const opts = { bindings };

  it("uses the user's keys instead of the defaults (stored overrides keep working)", () => {
    expect(keyAction('question', { key: 'j' }, false, opts)).toEqual({ kind: 'flip' });
    expect(keyAction('answer', { key: 'j' }, false, opts)).toEqual({ kind: 'flip' });
    expect(keyAction('question', { key: ' ' }, false, opts)).toBeNull();
    expect(keyAction('answer', { key: 'g' }, false, opts)).toEqual({ kind: 'rate', rating: 'good' });
    expect(keyAction('question', { key: 'g' }, false, opts)).toBeNull();
    expect(keyAction('answer', { key: '3' }, false, opts)).toBeNull();
    expect(keyAction('question', { key: 'h' }, false, opts)).toBeNull();
    expect(keyAction('question', { key: 'q' }, true, opts)).toEqual({ kind: 'end' });
    expect(keyAction('question', { key: 'Escape' }, false, opts)).toBeNull();
    expect(keyAction('answer', { key: 'u' }, false, opts)).toEqual({ kind: 'undo' });
  });

  it('leaves Ctrl+Z to the shell (global ui.undo) and keeps the fixed keys', () => {
    expect(keyAction('answer', { key: 'z', ctrlKey: true }, false, opts)).toBeNull();
    expect(keyAction('question', { key: '2' }, false, { ...opts, choices: 3 })).toEqual({ kind: 'choose', index: 1 });
    expect(keyAction('answer', { key: '7', ctrlKey: true }, false, { ...opts, flags: true })).toEqual({ kind: 'flag', flag: 7 });
  });

  it('tells the screen which key does what (legend, aria-keyshortcuts)', () => {
    expect(keyFor('study.reveal', bindings)).toBe('J');
    expect(keyFor('study.hint', bindings)).toBeNull();
    expect(keyFor('study.again', bindings)).toBe('1');
    expect(keyFor('study.reveal')).toBe('Space');
    expect(ariaKeys('Ctrl+1')).toBe('Control+1');
    expect(ariaKeys(null)).toBeUndefined();
  });
});

describe('interval labels', () => {
  it('picks the unit', () => {
    expect(intervalLabel(30_000)).toEqual({ key: 'study.interval.now', n: 0 });
    expect(intervalLabel(10 * 60_000)).toEqual({ key: 'study.interval.minutes', n: 10 });
    expect(intervalLabel(5 * 3_600_000)).toEqual({ key: 'study.interval.hours', n: 5 });
    expect(intervalLabel(3 * 86_400_000)).toEqual({ key: 'study.interval.days', n: 3 });
    expect(intervalLabel(60 * 86_400_000)).toEqual({ key: 'study.interval.months', n: 2 });
    expect(intervalLabel(730 * 86_400_000)).toEqual({ key: 'study.interval.years', n: 2 });
  });
});
