import { describe, expect, it } from 'vitest';
import { keyAction } from './keys';
import { intervalLabel } from './format';

describe('study keymap', () => {
  it('drives a whole session', () => {
    expect(keyAction('idle', { key: 'Enter' })).toEqual({ kind: 'start' });
    expect(keyAction('question', { key: ' ' })).toEqual({ kind: 'reveal' });
    expect(keyAction('question', { key: 'Enter' })).toEqual({ kind: 'reveal' });
    expect(keyAction('answer', { key: '1' })).toEqual({ kind: 'rate', rating: 'again' });
    expect(keyAction('answer', { key: '4' })).toEqual({ kind: 'rate', rating: 'easy' });
    expect(keyAction('answer', { key: ' ' })).toEqual({ kind: 'rate', rating: 'good' });
    expect(keyAction('done', { key: 'Enter' })).toEqual({ kind: 'back' });
    expect(keyAction('done', { key: 'R' })).toEqual({ kind: 'repeat' });
  });

  it('card tools and undo', () => {
    expect(keyAction('question', { key: 'h' })).toEqual({ kind: 'hint' });
    expect(keyAction('answer', { key: 'h' })).toBeNull();
    expect(keyAction('answer', { key: 's' })).toEqual({ kind: 'suspend' });
    expect(keyAction('question', { key: 'B' })).toEqual({ kind: 'bury' });
    expect(keyAction('answer', { key: 'z' })).toEqual({ kind: 'undo' });
    expect(keyAction('done', { key: 'z', ctrlKey: true })).toEqual({ kind: 'undo' });
    expect(keyAction('idle', { key: 'z' })).toBeNull();
    expect(keyAction('question', { key: 'Escape' })).toEqual({ kind: 'end' });
  });

  it('leaves keys alone while typing or with modifiers', () => {
    expect(keyAction('question', { key: 'h' }, true)).toBeNull();
    expect(keyAction('answer', { key: '3' }, true)).toBeNull();
    expect(keyAction('question', { key: 'Escape' }, true)).toEqual({ kind: 'end' });
    expect(keyAction('answer', { key: '3', ctrlKey: true })).toBeNull();
    expect(keyAction('answer', { key: '3', altKey: true })).toBeNull();
    expect(keyAction('question', { key: '3' })).toBeNull();
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
