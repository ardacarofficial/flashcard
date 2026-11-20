/**
 * v0.10 mode screens against a scripted backend (the rules are fc-app's; these tests prove the
 * screens only show and report): learn mode stages by keyboard, a printable/scorable test, the
 * study screen's timer, confidence buttons, focus mode and inline card composer.
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import type { ScreenCtx } from '../types';

const call = vi.hoisted(() => vi.fn<(method: string, params: any) => Promise<any>>());
// Module switches are tracked app-wide once (`trackModules`), so one set for the whole file.
const enabled = vi.hoisted(() => ({
  value: { study: true, learn: true, practice_test: true, browse: true, timed: true, confidence: true, focus: true, inline_cards: true } as Record<string, boolean>,
}));

vi.mock('$lib/api', () => {
  class ApiError extends Error {
    code: string;
    constructor(code: string) {
      super(code);
      this.code = code;
    }
  }
  return {
    ApiError,
    api: {
      call: (m: string, p: unknown) => call(m, p),
      on: async () => () => {},
      modules: {
        list: async () => ({
          modules: Object.entries(enabled.value).map(([id, on]) => ({ id, enabled: on })),
        }),
      },
    },
  };
});

import { i18n } from '$lib/i18n/index.svelte';
import { secondsLeft, testAnswers, emptyInput } from './modes';
import LearnScreen from './LearnScreen.svelte';
import TestScreen from './TestScreen.svelte';
import StudyScreen from '../study/StudyScreen.svelte';

i18n.locale = 'en';

function ctx(): ScreenCtx {
  return {
    decks: [],
    selectedDeck: 'deck-1',
    selectedPath: 'Deck',
    selectDeck: () => {},
    guarded: async (fn) => {
      await fn();
      return true;
    },
  };
}

const sides = (q: string) => ({
  template_key: 'forward',
  template_name: '1',
  card_type: 'classic',
  question: q,
  answer: q,
  css: '',
  interaction: { kind: 'self_grade' },
  tts: [],
});

const card = (id: string, q: string) => ({
  card_id: id,
  note_id: 'n',
  deck_id: 'deck-1',
  state: 'new',
  sides: sides(q),
  flags: 0,
  type_answer: false,
  intervals: (['again', 'hard', 'good', 'easy'] as const).map((rating, i) => ({
    rating,
    interval_ms: (i + 1) * 60_000,
    scheduled_days: 0,
  })),
  hints_used: 0,
});

function session(over: Record<string, unknown>) {
  return {
    id: 's-1',
    kind: 'normal',
    mode: 'scheduled',
    deck_id: 'deck-1',
    started_at: 0,
    ended_at: null,
    counts: { new: 1, learning: 0, review: 0 },
    answered: 0,
    correct: 0,
    mistakes: 0,
    current: card('c-1', 'badger'),
    next_due: null,
    modes: null,
    ...over,
  };
}

/** Default answers for the calls every screen makes on mount. */
function base(method: string): unknown {
  switch (method) {
    case 'study.active':
      return { session: null };
    case 'shortcuts.list':
      return { items: [] };
    case 'flags.list':
      return { flags: [] };
    case 'saved_queries.list':
      return { items: [] };
    case 'settings.get':
      return { values: [] };
    case 'study.end':
      return {};
  }
  throw new Error(`unexpected ${method}`);
}

async function key(k: string, target: Element | Window = window) {
  await new Promise((r) => setTimeout(r, 0));
  await fireEvent.keyDown(target, { key: k });
}

afterEach(() => {
  call.mockReset();
  document.documentElement.className = '';
});

describe('mode helpers', () => {
  it('counts down and builds test answers by question type', () => {
    expect(secondsLeft(10_000, 4_100)).toBe(6);
    expect(secondsLeft(10_000, 12_000)).toBe(0);
    const qs = [
      { index: 0, kind: 'multiple_choice' },
      { index: 1, kind: 'true_false' },
      { index: 2, kind: 'written' },
      { index: 3, kind: 'written' },
    ] as any;
    const input = emptyInput();
    input.choice[0] = 2;
    input.truth[1] = false;
    input.text[2] = ' otter ';
    input.text[3] = '   ';
    expect(testAnswers(qs, input)).toEqual([
      { index: 0, choice: 2, text: null, truth: null },
      { index: 1, choice: null, text: null, truth: false },
      { index: 2, choice: null, text: 'otter', truth: null },
    ]);
  });
});

describe('learn screen', () => {
  it('asks multiple choice, then written, by keyboard until the card is learned', async () => {
    const learn = (stage: string | null, learned: number) => ({
      rating_scale: 'buttons',
      confidence: [],
      timer: null,
      timed_out: 0,
      learn: {
        stage,
        options: stage === 'choice' ? ['kedi', 'porsuk', 'vaşak', 'su samuru'] : [],
        mistakes: 0,
        progress: { total: 1, learned, choice: stage === 'choice' ? 1 : 0, written: stage === 'written' ? 1 : 0 },
      },
      test: null,
    });
    const choice = session({ kind: 'learn', modes: learn('choice', 0) });
    const written = session({ kind: 'learn', modes: learn('written', 0) });
    const done = session({ kind: 'learn', current: null, modes: learn(null, 1) });
    call.mockImplementation(async (m, p) => {
      if (m === 'study.start') return choice;
      if (m === 'modes.learn.answer') {
        if (p.choice !== null) {
          expect(p.choice).toBe(1);
          return { grade: 'correct', stage: 'choice', learned: false, expected: 'porsuk', diff: [], session: written };
        }
        expect(p.given).toBe('porsuk');
        return { grade: 'correct', stage: 'written', learned: true, expected: 'porsuk', diff: [], session: done };
      }
      return base(m);
    });
    render(LearnScreen, { ctx: ctx() });
    await screen.findByRole('button', { name: /Start learning/ });
    await key('Enter');
    await screen.findByText('Pick the answer');
    expect(call).toHaveBeenCalledWith('study.start', expect.objectContaining({ kind: 'learn', deck_id: 'deck-1' }));
    await key('2'); // porsuk
    expect((await screen.findByTestId('learn-feedback')).textContent).toContain('Correct!');
    await key('Enter');
    const input = await screen.findByLabelText('Your answer');
    await fireEvent.input(input, { target: { value: 'porsuk' } });
    await key('Enter', input);
    expect((await screen.findByTestId('learn-feedback')).textContent).toContain('Learned!');
    await key('Enter');
    await screen.findByText('You learned all 1 cards.');
    expect(call).toHaveBeenCalledWith('study.end', { session_id: 's-1' });
  });
});

describe('test screen', () => {
  it('creates a mixed paper, prints the key and shows the score', async () => {
    const questions = [
      { index: 0, card_id: 'c1', kind: 'true_false', prompt_html: '<p>q1</p>', options: [], statement: 'a1', key: null },
      { index: 1, card_id: 'c2', kind: 'multiple_choice', prompt_html: '<p>q2</p>', options: ['x', 'a2'], statement: null, key: null },
      { index: 2, card_id: 'c3', kind: 'written', prompt_html: '<p>q3</p>', options: [], statement: null, key: null },
    ];
    const keyed = questions.map((q, i) => ({
      ...q,
      key: { option: i === 1 ? 1 : null, truth: i === 0 ? true : null, expected: `a${i + 1}` },
    }));
    call.mockImplementation(async (m, p) => {
      if (m === 'study.start') {
        expect(p).toMatchObject({ kind: 'test', filter: { query: 'exam:TYT' }, modes: { test: { questions: 3 } } });
        return session({ kind: 'test', current: null, modes: { test: { questions: 3, submitted: false, percent: null } } });
      }
      if (m === 'modes.test.paper') return { session_id: 's-1', questions: p.with_key ? keyed : questions, answers: null, result: null };
      if (m === 'modes.test.submit') {
        expect(p.answers).toEqual([
          { index: 0, choice: null, text: null, truth: true },
          { index: 1, choice: 1, text: null, truth: null },
          { index: 2, choice: null, text: 'a3', truth: null },
        ]);
        return {
          session_id: 's-1',
          questions: keyed,
          answers: p.answers,
          result: {
            total: 3, correct: 3, almost: 0, wrong: 0, blank: 0, points: 3, percent: 100,
            items: [0, 1, 2].map((index) => ({ index, verdict: 'correct' })),
          },
        };
      }
      return base(m);
    });
    const print = vi.spyOn(window, 'print').mockImplementation(() => {});
    render(TestScreen, { ctx: ctx() });
    await fireEvent.input(await screen.findByLabelText('Query'), { target: { value: 'exam:TYT' } });
    await fireEvent.input(screen.getByLabelText('Questions'), { target: { value: '3' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Create test' }));
    await screen.findByTestId('question-2');
    expect(screen.queryByText(/Answer: a3/)).toBeNull(); // no key on the paper
    await fireEvent.click(screen.getByRole('button', { name: 'Print answer key' }));
    await waitFor(() => expect(print).toHaveBeenCalled());
    expect(screen.queryByText(/Answer: a3/)).toBeNull(); // key gone again after printing
    await fireEvent.click(screen.getAllByLabelText('True')[0]!);
    await fireEvent.click(screen.getByLabelText('a2'));
    await fireEvent.input(screen.getByLabelText('Your answer'), { target: { value: 'a3' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Submit' }));
    expect((await screen.findByTestId('test-score')).textContent).toContain('Score: 100%');
    expect(screen.getByText(/Answer: a3/)).toBeTruthy();
  });
});

describe('study screen modes', () => {
  it('expires a timed card at its deadline and rates on the confidence scale with keys 1–5', async () => {
    const levels = [1, 2, 3, 4, 5].map((level) => ({
      level,
      rating: (['again', 'again', 'hard', 'good', 'easy'] as const)[level - 1],
      interval_ms: level * 60_000,
    }));
    const modes = (deadline: number) => ({
      rating_scale: 'confidence',
      confidence: levels,
      timer: { seconds: 10, on_timeout: 'again', deadline },
      timed_out: 0,
      learn: null,
      test: null,
    });
    const first = session({ modes: modes(Date.now() + 60_000) });
    const second = session({ current: card('c-2', 'otter'), modes: modes(Date.now() + 300) });
    const third = session({ current: card('c-3', 'heron'), modes: modes(Date.now() + 60_000) });
    call.mockImplementation(async (m, p) => {
      if (m === 'study.start') {
        expect(p.modes).toEqual({ timed: { seconds: 10, on_timeout: 'again' }, rating_scale: 'confidence', test: null });
        return first;
      }
      if (m === 'modes.confidence.answer') {
        expect(p).toMatchObject({ card_id: 'c-1', level: 3 });
        return second;
      }
      if (m === 'modes.timed.expire') {
        expect(p).toEqual({ session_id: 's-1', card_id: 'c-2' });
        return third;
      }
      return base(m);
    });
    render(StudyScreen, { ctx: ctx() });
    await fireEvent.click(await screen.findByLabelText('Time limit'));
    await fireEvent.change(screen.getByLabelText('Rating'), { target: { value: 'confidence' } });
    await key('Enter');
    await screen.findByTestId('timer');
    await key(' '); // reveal
    await screen.findByText('How sure were you?');
    expect(screen.queryByRole('button', { name: /Good/ })).toBeNull(); // no four buttons
    await key('3');
    await waitFor(() => expect(screen.getByTestId('question').textContent).toContain('otter'));
    // The countdown runs out → the backend is told; the next card shows.
    await waitFor(() => expect(screen.getByTestId('question').textContent).toContain('heron'), { timeout: 2_000 });
  });

  it('enters focus mode with F and creates inline cards with N without leaving the card', async () => {
    call.mockImplementation(async (m, p) => {
      if (m === 'study.start') return session({});
      if (m === 'modes.inline.preview')
        return { cards: [{ line: 1, front: 'porsuk', back: 'badger', duplicate: false }], truncated: false };
      if (m === 'modes.inline.create') {
        expect(p).toMatchObject({ deck_id: 'deck-1', text: 'porsuk :: badger' });
        return { note_ids: ['n-9'], duplicates: 0, truncated: false };
      }
      return base(m);
    });
    render(StudyScreen, { ctx: ctx() });
    await key('Enter');
    await screen.findByTestId('focus-toggle');
    await key('f');
    expect(document.documentElement.classList.contains('fc-focus')).toBe(true);
    expect(document.documentElement.classList.contains('fc-focus-hide-counts')).toBe(true);
    await key('f');
    expect(document.documentElement.classList.contains('fc-focus')).toBe(false);
    await key('n');
    const area = await screen.findByRole('textbox', { name: 'New cards from text' });
    await fireEvent.input(area, { target: { value: 'porsuk :: badger' } });
    await screen.findByTestId('inline-preview');
    await fireEvent.keyDown(area, { key: 'Enter', ctrlKey: true });
    await screen.findByText('1 card(s) created.');
    expect(screen.getByTestId('question').textContent).toContain('badger'); // still on the same card
  });
});
