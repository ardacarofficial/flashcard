import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/svelte';
import { i18n } from '$lib/i18n/index.svelte';
import { api } from '$lib/api';
import BrowseScreen from './BrowseScreen.svelte';
import type { ScreenCtx } from '../types';

i18n.locale = 'en';

const ctx: ScreenCtx = {
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
};

describe('<BrowseScreen> on the dev mock', () => {
  it('lists matching cards and shows a positioned query error', async () => {
    const decks = (await api.deck.tree()).roots;
    await api.note.createClassic(decks[0]!.deck.id, 'kılıç', 'sword', []);
    render(BrowseScreen, { ctx: { ...ctx, decks } });
    expect(await screen.findByText('1 card')).toBeTruthy();
    const input = screen.getByLabelText('Search', { selector: 'input' });
    await fireEvent.input(input, { target: { value: 'tag:"x' } });
    const alert = await screen.findByRole('alert', {}, { timeout: 2000 });
    expect(alert.textContent).toContain('A quote is not closed.');
    expect(alert.textContent).toContain('at 5–6');
  });

  it('loads more with the keyset cursor of the page on screen (v1.4)', async () => {
    const decks = (await api.deck.tree()).roots;
    for (let i = 0; i < 104; i++) await api.note.createClassic(decks[0]!.deck.id, `kalem ${i}`, 'pen', []);
    const call = vi.spyOn(api, 'call');
    render(BrowseScreen, { ctx: { ...ctx, decks } });
    const input = screen.getByLabelText('Search', { selector: 'input' });
    await fireEvent.input(input, { target: { value: 'kalem' } });
    await fireEvent.submit(screen.getByRole('search'));
    expect(await screen.findByText('Showing 100 of 104')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Load more' }));
    await vi.waitFor(() => expect(screen.queryByText('Showing 100 of 104')).toBeNull());
    const pages = call.mock.calls.filter(([m]) => m === 'search.cards').map(([, b]) => b as Record<string, unknown>);
    const last = pages.at(-1)!;
    expect(last.cursor).toBe('@100');
    expect(last.offset).toBeUndefined();
    expect(last.query).toBe('kalem');
    call.mockRestore();
  });
});
