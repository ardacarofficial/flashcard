/**
 * docs/04 v0.2 acceptance: a session can be run entirely from the keyboard. Drives the study
 * screen on the dev mock transport with key presses only — start, reveal, rate, hint, undo,
 * end-of-session summary, repeat mistakes, back.
 */
import { describe, expect, it } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { api, CLASSIC_NOTETYPE_ID } from '$lib/api';
import { i18n } from '$lib/i18n/index.svelte';
import type { ScreenCtx } from '../types';
import StudyScreen from './StudyScreen.svelte';

i18n.locale = 'en';

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
    render(StudyScreen, { ctx: ctx(deck.id) });
    await screen.findByRole('button', { name: /Study now/ });

    await key('Enter'); // start
    await side('question', 'dog');
    expect(screen.getByTestId('study-count').textContent).toBe('1 / 2');
    expect(screen.queryByTestId('answer')).toBeNull();

    await key('h'); // hint
    expect((await screen.findByTestId('hint')).textContent).toBe('k____');

    await key(' '); // reveal
    await side('answer', 'köpek');
    await screen.findByText('10m'); // interval shown above "Good"

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
    await key(' '); // space on the answer = good

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

    await key('Enter'); // back to the start panel
    await screen.findByRole('button', { name: /Study now/ });
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
    render(StudyScreen, { ctx: ctx(deck.id) });
    await screen.findByRole('button', { name: /Study now/ });
    await key('Enter');
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
    render(StudyScreen, { ctx: ctx(deck.id) });
    await screen.findByRole('button', { name: /Study now/ });
    await key('Enter');
    await screen.findByText('one');
    await key('Escape');
    // v1.2: nothing answered → no empty summary, back on the start panel, session closed.
    await screen.findByRole('button', { name: /Study now/ });
    expect(screen.queryByText('Session complete')).toBeNull();
    expect((await api.call('study.active', {})).session).toBeNull();
  });

  it('Escape after answering shows the summary', async () => {
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
    render(StudyScreen, { ctx: ctx(deck.id) });
    await screen.findByRole('button', { name: /Study now/ });
    await key('Enter');
    await screen.findByText('three');
    await key(' ');
    await key('2');
    await screen.findByText('four');
    await key('Escape');
    await screen.findByText('Session complete');
    expect(screen.getByTestId('summary-answered').textContent).toBe('1');
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
    render(StudyScreen, { ctx: ctx(deck.id) });
    await screen.findByRole('button', { name: /Study now/ });
    await key('Enter');
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
