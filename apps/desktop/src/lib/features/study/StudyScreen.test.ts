/**
 * docs/04 v0.2 acceptance: a session can be run entirely from the keyboard. Drives the study
 * screen on the dev mock transport with key presses only — start, reveal, rate, hint, undo,
 * end-of-session summary, repeat mistakes, back.
 */
import { describe, expect, it } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/svelte';
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
async function key(k: string) {
  await new Promise((r) => setTimeout(r, 0));
  await fireEvent.keyDown(window, { key: k });
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
    render(StudyScreen, { ctx: ctx(deck.id) });
    await screen.findByRole('button', { name: /Study now/ });

    await key('Enter'); // start
    expect((await screen.findByTestId('question')).textContent).toBe('dog');
    expect(screen.queryByTestId('answer')).toBeNull();

    await key('h'); // hint
    expect((await screen.findByTestId('hint')).textContent).toBe('k____');

    await key(' '); // reveal
    expect((await screen.findByTestId('answer')).textContent).toBe('köpek');
    await screen.findByText('10m'); // interval shown above "Good"

    await key('1'); // again → dog goes to the back
    expect((await screen.findByTestId('question')).textContent).toBe('cat');

    await key('z'); // undo → dog again
    await screen.findByText('dog');
    expect(screen.getByTestId('question').textContent).toBe('dog');

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
    expect(screen.getByText('Cards studied: 3')).toBeTruthy();
    expect(screen.getByText('Mistakes: 1')).toBeTruthy();
    expect(screen.getByText('Accuracy: 67%')).toBeTruthy();

    await key('r'); // repeat mistakes: only "dog"
    expect((await screen.findByTestId('question')).textContent).toBe('dog');
    await key(' ');
    await screen.findByTestId('answer');
    await key('4');
    await screen.findByText('Cards studied: 1');

    await key('Enter'); // back to the start panel
    await screen.findByRole('button', { name: /Study now/ });
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
    await screen.findByText('Session complete');
    expect(screen.getByText('Cards studied: 0')).toBeTruthy();
  });
});
