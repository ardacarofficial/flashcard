import { describe, expect, it } from 'vitest';
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
    expect(await screen.findByText('1 card(s)')).toBeTruthy();
    const input = screen.getByLabelText('Search', { selector: 'input' });
    await fireEvent.input(input, { target: { value: 'tag:"x' } });
    const alert = await screen.findByRole('alert', {}, { timeout: 2000 });
    expect(alert.textContent).toContain('A quote is not closed.');
    expect(alert.textContent).toContain('at 5–6');
  });
});
