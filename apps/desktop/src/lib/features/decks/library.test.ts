import { describe, expect, it } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/svelte';
import { i18n } from '$lib/i18n/index.svelte';
import { api } from '$lib/api';
import LibraryScreen from './LibraryScreen.svelte';
import HomeScreen from '../home/HomeScreen.svelte';
import type { ScreenCtx } from '../types';

i18n.locale = 'en';

const ctx = (over: Partial<ScreenCtx> = {}): ScreenCtx => ({
  decks: [],
  selectedDeck: null,
  selectedPath: null,
  selectDeck: () => {},
  guarded: async (fn) => {
    try {
      await fn();
      return true;
    } catch {
      return false;
    }
  },
  ...over,
});

describe('v1.1 Library, deck page and Home on the dev mock', () => {
  it('shows one empty state with one action when there are no decks', async () => {
    render(LibraryScreen, { ctx: ctx() });
    expect(await screen.findByText('No decks yet')).toBeTruthy();
    expect(screen.getAllByRole('button', { name: 'Create your first deck' })).toHaveLength(1);
  });

  it('lists decks as cards with their progress', async () => {
    const decks = (await api.deck.tree()).roots;
    render(LibraryScreen, { ctx: ctx({ decks }) });
    const rows = await screen.findAllByTestId('deck-row');
    expect(rows.length).toBe(decks.length);
    expect(await screen.findAllByText(/% mastered/)).toHaveLength(decks.length);
  });

  it('opens the deck page: modes, term list and inline edit', async () => {
    const decks = (await api.deck.tree()).roots;
    const deck = decks[0]!;
    await api.note.createClassic(deck.deck.id, 'kılıç', 'sword', []);
    render(LibraryScreen, { ctx: ctx({ decks, selectedDeck: deck.deck.id }) });
    expect(await screen.findByRole('heading', { level: 1, name: deck.deck.name })).toBeTruthy();
    expect(await screen.findByRole('button', { name: /Flashcards/ })).toBeTruthy();
    expect((await screen.findAllByText('sword')).length).toBeGreaterThan(0);

    await fireEvent.click(screen.getByRole('button', { name: 'Edit kılıç' }));
    const back = screen.getByLabelText('Definition', { selector: 'textarea' });
    await fireEvent.input(back, { target: { value: 'blade' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Save' }));
    expect((await screen.findAllByText('blade')).length).toBeGreaterThan(0);
    const notes = await api.call('notes.terms', { deck_id: deck.deck.id, include_subdecks: true, page: { cursor: null, limit: null } });
    expect(notes.items.find((n) => n.front.value === 'kılıç')?.back?.value).toBe('blade');
  });

  it('Home shows today and the start button', async () => {
    const decks = (await api.deck.tree()).roots;
    render(HomeScreen, { ctx: ctx({ decks }) });
    expect(await screen.findByRole('button', { name: /Start studying/ })).toBeTruthy();
    expect(await screen.findByText('Today')).toBeTruthy();
  });
});
