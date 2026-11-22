/**
 * docs/04 v0.2 acceptance: a session can be run entirely from the keyboard. Drives the study
 * screen on the dev mock transport with key presses only — start (v1.3: in the launcher),
 * flip, rate, hint, undo, end-of-session summary, repeat mistakes, back.
 */
import { afterEach, describe, expect, it } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { api, CLASSIC_NOTETYPE_ID } from '$lib/api';
import { i18n } from '$lib/i18n/index.svelte';
import type { ScreenCtx } from '../types';
import StudyScreen from './StudyScreen.svelte';
import LauncherHost from './LauncherHost.svelte';
// The host loads the dialog lazily; load it up front so a cold transform is not a timeout.
import './StudyLauncher.svelte';
import { setMockLearningStep } from './mock';

i18n.locale = 'en';

afterEach(() => {
  cleanup();
  setMockLearningStep(0);
});

function ctx(deck: string | null): ScreenCtx {
  return {
    decks: [],
    selectedDeck: deck,
    selectedPath: null,
    selectDeck: () => {},
    guarded: async (fn) => {
      await fn();
      return true;
    },
  };
}

/** Presses a key once the previous action has fully settled (the screen ignores keys while
 * a call is in flight, so a double press never rates twice). */
async function key(k: string, mods: { ctrlKey?: boolean } = {}) {
  await new Promise((r) => setTimeout(r, 0));
  await fireEvent.keyDown(window, { key: k, ...mods });
}

/**
 * Renders the Cards screen with the shell's launcher host (v1.3: sessions start there) and
 * starts today's cards with Enter in the launcher.
 */
async function startSession(deck: string) {
  const c = ctx(deck);
  render(LauncherHost, { ctx: c });
  render(StudyScreen, { ctx: c });
  // The page opens the launcher by itself once it is registered; the button does too.
  const button = await screen.findByTestId('study-start');
  if (!screen.queryByTestId('study-launcher')) await fireEvent.click(button);
  await screen.findByTestId('study-launcher', {}, { timeout: 5_000 });
  const start = await screen.findByTestId('launcher-start');
  await waitFor(() => expect((start as HTMLButtonElement).disabled).toBe(false));
  await waitFor(() => expect(document.activeElement?.closest('[data-testid=study-launcher]')).toBeTruthy());
  await fireEvent.keyDown(document.activeElement!, { key: 'Enter' });
  await waitFor(() => expect(screen.queryByTestId('study-launcher')).toBeNull());
}

/** Waits until the card side `id` (`question` / `answer`, rendered HTML) reads `text`. */
async function side(id: 'question' | 'answer', text: string) {
  await waitFor(() => expect(screen.getByTestId(id).textContent?.trim()).toBe(text));
}

describe('study screen', () => {
  it('runs a whole session from the keyboard', async () => {
    const deck = await api.call('decks.create', { path: 'Klavye', parent_id: null });
    for (const [front, back] of [['dog', 'köpek'], ['cat', 'kedi']]) {
      await api.call('notes.create', {
        notetype_id: CLASSIC_NOTETYPE_ID,
        deck_id: deck.id,
        fields: { front: front!, back: back! },
        tags: [],
        attributes: [],
      });
    }
    // Four answer buttons with their intervals (the v1.1 display preference).
    await api.call('settings.set', { scope: { kind: 'app' }, key: 'study.answer_buttons', value: 'four' });
    await startSession(deck.id);
    await side('question', 'dog');
    expect(screen.getByTestId('study-count').textContent).toBe('1 / 2');
    expect(screen.queryByTestId('answer')).toBeNull();

    await key('h'); // hint
    expect((await screen.findByTestId('hint')).textContent).toBe('k____');

    await key('3'); // rating keys do nothing before the back was seen
    await side('question', 'dog');

    await key(' '); // flip: the answer
    await side('answer', 'köpek');
    await screen.findByText('10m'); // interval shown above "Good"
    const card = () => document.querySelector('.fc-flashcard')!;
    expect(card().classList.contains('flipped')).toBe(true);
    await key(' '); // Space again: back to the front — no grade, still card 1
    await waitFor(() => expect(card().classList.contains('flipped')).toBe(false));
    expect(screen.getByTestId('study-count').textContent).toBe('1 / 2');
    await key(' '); // and over again
    await waitFor(() => expect(card().classList.contains('flipped')).toBe(true));
    await fireEvent.keyDown(window, { key: '1', repeat: true }); // a held key is not a grade
    expect(screen.getByTestId('study-count').textContent).toBe('1 / 2');

    await key('1'); // again → dog goes to the back
    await side('question', 'cat');

    await key('z'); // undo → dog again
    await side('question', 'dog');

    await key(' ');
    await screen.findByTestId('answer');
    await key('1'); // again (mistake)
    await screen.findByText('cat');
    await key(' ');
    await screen.findByTestId('answer');
    await key('3'); // good
    await screen.findByText('dog');
    await key(' ');
    await screen.findByTestId('answer');
    await key('3'); // good

    // Summary.
    await screen.findByText('Session complete');
    expect(screen.getByTestId('summary-answered').textContent).toBe('3');
    expect(screen.getByTestId('summary-mistakes').textContent).toBe('1');
    expect(screen.getByText('2 of 3 correct')).toBeTruthy();

    await key('r'); // repeat mistakes: only "dog"
    await side('question', 'dog');
    await key(' ');
    await screen.findByTestId('answer');
    await key('4');
    await waitFor(() => expect(screen.getByTestId('summary-answered').textContent).toBe('1'));

    await fireEvent.click(screen.getByRole('button', { name: 'Done' })); // back to the page
    await screen.findByTestId('study-idle');
  });

  it('shows and toggles flags with Ctrl+1–7 (Anki numbering)', async () => {
    const deck = await api.call('decks.create', { path: 'Bayrak', parent_id: null });
    await api.call('notes.create', {
      notetype_id: CLASSIC_NOTETYPE_ID,
      deck_id: deck.id,
      fields: { front: 'flag me', back: 'bayrak' },
      tags: [],
      attributes: [],
    });
    await startSession(deck.id);
    await side('question', 'flag me');
    expect(screen.queryByTestId('flag')).toBeNull();
    await key('7', { ctrlKey: true }); // flag 7 = purple
    expect((await screen.findByTestId('flag')).textContent).toBe('Purple');
    await key(' '); // the answer side keeps the flag
    await side('answer', 'bayrak');
    await key('7', { ctrlKey: true }); // off again, still on the answer
    await waitFor(() => expect(screen.queryByTestId('flag')).toBeNull());
    expect(screen.getByTestId('answer')).toBeTruthy();
  });

  it('Escape ends a session early', async () => {
    const deck = await api.call('decks.create', { path: 'Kaçış', parent_id: null });
    await api.call('notes.create', {
      notetype_id: CLASSIC_NOTETYPE_ID,
      deck_id: deck.id,
      fields: { front: 'one', back: 'bir' },
      tags: [],
      attributes: [],
    });
    await startSession(deck.id);
    await screen.findByText('one');
    await key('Escape');
    // Nothing answered → no empty summary, back on the page, session closed.
    await screen.findByTestId('study-idle');
    expect(screen.queryByText('Session complete')).toBeNull();
    expect((await api.call('study.active', {})).session).toBeNull();
  });

  it('Escape after answering pauses: what you did so far, the session stays open, Continue returns to it', async () => {
    const deck = await api.call('decks.create', { path: 'Özet', parent_id: null });
    for (const [front, back] of [['three', 'üç'], ['four', 'dört']] as const) {
      await api.call('notes.create', {
        notetype_id: CLASSIC_NOTETYPE_ID,
        deck_id: deck.id,
        fields: { front, back },
        tags: [],
        attributes: [],
      });
    }
    await startSession(deck.id);
    await screen.findByText('three');
    await key(' ');
    await key('2');
    await screen.findByText('four');
    await key('Escape');
    // v1.3: leaving with cards left is a pause (the launcher offers "Continue" with this session).
    await screen.findByText('Paused — you can continue where you left off');
    expect(screen.getByTestId('summary-answered').textContent).toBe('1');
    const open = (await api.call('study.active', {})).session;
    expect(open?.ended_at).toBeNull();
    await fireEvent.click(screen.getByTestId('study-continue'));
    await screen.findByText('four');
    expect((await api.call('study.active', {})).session?.id).toBe(open?.id);
  });

  it('shows two answer buttons by default: 1 = Still learning, 2 = I know it', async () => {
    await api.call('settings.set', { scope: { kind: 'app' }, key: 'study.answer_buttons', value: 'two' });
    const deck = await api.call('decks.create', { path: 'İki', parent_id: null });
    for (const [front, back] of [['sun', 'güneş'], ['moon', 'ay']]) {
      await api.call('notes.create', {
        notetype_id: CLASSIC_NOTETYPE_ID,
        deck_id: deck.id,
        fields: { front: front!, back: back! },
        tags: [],
        attributes: [],
      });
    }
    await startSession(deck.id);
    await side('question', 'sun');
    await key(' ');
    await screen.findByRole('button', { name: /Still learning/ });
    expect(screen.getByRole('button', { name: /I know it/ })).toBeTruthy();
    expect(screen.queryByRole('button', { name: /Hard/ })).toBeNull();
    await key('2'); // I know it = good: sun is done
    await side('question', 'moon');
    await key(' ');
    await screen.findByTestId('answer');
    await key('1'); // still learning = again: moon comes back
    await side('question', 'moon');
    await key(' ');
    await screen.findByTestId('answer');
    await key('2');
    await screen.findByText('Session complete');
    expect(screen.getByTestId('summary-mistakes').textContent).toBe('1');
  });
});

describe('study screen v1.3', () => {
  it('waits instead of closing when only learning cards remain', async () => {
    setMockLearningStep(10 * 60_000);
    const deck = await api.call('decks.create', { path: 'Bekle', parent_id: null });
    await api.call('notes.create', {
      notetype_id: CLASSIC_NOTETYPE_ID,
      deck_id: deck.id,
      fields: { front: 'wait', back: 'bekle' },
      tags: [],
      attributes: [],
    });
    await startSession(deck.id);
    await side('question', 'wait');
    await key(' ');
    await screen.findByTestId('answer');
    await key('1'); // again: the card waits for its learning step
    const waiting = await screen.findByTestId('study-waiting');
    expect(waiting.textContent).toMatch(/Next card in 10 min/);
    expect(screen.queryByText('Session complete')).toBeNull();
    // The session is still open.
    expect((await api.call('study.active', {})).session?.ended_at).toBeNull();
    await fireEvent.click(screen.getByTestId('study-finish'));
    await screen.findByText('Session complete');
    expect(screen.getByTestId('summary-answered').textContent).toBe('1');
  });

  it('Space on a focused answer button flips instead of grading', async () => {
    await api.call('settings.set', { scope: { kind: 'app' }, key: 'study.answer_buttons', value: 'two' });
    const deck = await api.call('decks.create', { path: 'Odak', parent_id: null });
    await api.call('notes.create', {
      notetype_id: CLASSIC_NOTETYPE_ID,
      deck_id: deck.id,
      fields: { front: 'focus', back: 'odak' },
      tags: [],
      attributes: [],
    });
    await startSession(deck.id);
    await side('question', 'focus');
    await key(' ');
    const know = await screen.findByTestId('rate-good');
    know.focus();
    await fireEvent.keyDown(know, { key: ' ' });
    await waitFor(() => expect(document.querySelector('.fc-flashcard')!.classList.contains('flipped')).toBe(false));
    expect(screen.getByTestId('study-count').textContent).toBe('1 / 1');
    expect(screen.queryByText('Session complete')).toBeNull();
    // The on-screen legend tells the truth: Space flips, 1 2 rate.
    expect(screen.getByTestId('study-legend').textContent).toMatch(/Space flip/);
    expect(screen.getByTestId('study-legend').textContent).toMatch(/1 2 rate/);
  });
});

describe('study screen keys (v1.3 J7)', () => {
  async function threeCards(path: string) {
    await api.call('settings.set', { scope: { kind: 'app' }, key: 'study.answer_buttons', value: 'two' });
    const deck = await api.call('decks.create', { path, parent_id: null });
    for (const [front, back] of [['one', 'bir'], ['two', 'iki'], ['three', 'üç']].map(([f, b]) => [`${f} ${path}`, `${b} ${path}`])) {
      await api.call('notes.create', {
        notetype_id: CLASSIC_NOTETYPE_ID,
        deck_id: deck.id,
        fields: { front: front!, back: back! },
        tags: [],
        attributes: [],
      });
    }
    await startSession(deck.id);
    await side('question', `one ${path}`);
  }

  it('holding 2 grades exactly one card', async () => {
    await threeCards('Basılı');
    await key(' ');
    await screen.findByTestId('answer');
    await key('2'); // the key goes down: one grade
    await side('question', 'two Basılı');
    expect(screen.getByTestId('study-count').textContent).toBe('2 / 3');
    // Still held while the next card is turned: the repeats grade nothing.
    await key(' ');
    await side('answer', 'iki Basılı');
    for (let i = 0; i < 5; i++) await fireEvent.keyDown(window, { key: '2', repeat: true });
    await new Promise((r) => setTimeout(r, 50));
    expect(screen.getByTestId('study-count').textContent).toBe('2 / 3');
    expect(screen.getByTestId('answer').textContent?.trim()).toBe('iki Basılı');
  });

  it('plain keys typed into a text field or contenteditable do not flip, grade, pause or bury', async () => {
    await threeCards('Yazı');
    const input = document.body.appendChild(document.createElement('input'));
    const editor = document.body.appendChild(document.createElement('div'));
    editor.setAttribute('contenteditable', 'true');
    try {
      for (const target of [input, editor]) {
        for (const k of [' ', 'h', 's', 'b', 'z', 'n']) await fireEvent.keyDown(target, { key: k });
      }
      await new Promise((r) => setTimeout(r, 50));
      expect(screen.queryByTestId('answer')).toBeNull();
      expect(screen.queryByTestId('hint')).toBeNull();
      await key(' ');
      await screen.findByTestId('answer');
      for (const target of [input, editor]) {
        for (const k of ['1', '2']) await fireEvent.keyDown(target, { key: k });
      }
      await new Promise((r) => setTimeout(r, 50));
      expect(screen.getByTestId('study-count').textContent).toBe('1 / 3');
      await side('answer', 'bir Yazı');
    } finally {
      input.remove();
      editor.remove();
    }
  });
});
