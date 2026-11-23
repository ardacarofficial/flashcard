/** v1.4: the deck language of the word index on the Library and the deck page (dev mock). */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { i18n } from '$lib/i18n/index.svelte';
import { api } from '$lib/api';
import LibraryScreen from './LibraryScreen.svelte';
import type { ScreenCtx } from '../types';
import { LANGUAGE_KEY, languageName } from './language';

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

/** `words.stats` answers `unassigned` notes; everything else goes to the mock. */
function stubStats(unassigned: number) {
  const real = api.call.bind(api);
  const asked: string[] = [];
  const spy = vi.spyOn(api, 'call').mockImplementation(((method: string, params: never) => {
    asked.push(method);
    if (method === 'words.stats') return Promise.resolve({ threshold_days: 21, languages: [], unassigned_notes: unassigned });
    return (real as (m: string, p: unknown) => Promise<unknown>)(method, params);
  }) as typeof api.call);
  return { spy, asked };
}

describe('deck language (words.language)', () => {
  afterEach(() => vi.restoreAllMocks());

  it('the Library shows the "no language" row only when notes are unassigned, and it opens a deck', async () => {
    const decks = (await api.deck.tree()).roots;
    const selected: (string | null)[] = [];
    stubStats(3);
    const first = render(LibraryScreen, { ctx: ctx({ decks, selectDeck: (id) => selected.push(id) }) });
    const row = await screen.findByTestId('library-unassigned');
    expect(row.textContent).toContain("3 notes have no language, so their words aren't counted among the words you know.");
    expect(row.textContent).toContain('Give its deck a language');
    // Every mock deck is without a language: the first one (tree order) opens.
    await fireEvent.click(row);
    await waitFor(() => expect(selected).toEqual([decks[0]!.deck.id]));
    first.unmount();
    vi.restoreAllMocks();

    const { asked } = stubStats(0);
    render(LibraryScreen, { ctx: ctx({ decks }) });
    await screen.findAllByTestId('deck-row');
    await waitFor(() => expect(asked).toContain('words.stats'));
    expect(screen.queryByTestId('library-unassigned')).toBeNull();
  });

  it('the deck page says the language is unknown and sets it for this deck', async () => {
    const decks = (await api.deck.tree()).roots;
    const deck = decks[0]!;
    render(LibraryScreen, { ctx: ctx({ decks, selectedDeck: deck.deck.id }) });
    const line = await screen.findByTestId('deck-language');
    await waitFor(() => expect(line.textContent).toContain("This deck has no language: its words aren't counted."));

    const picker = screen.getByRole('combobox', { name: `Language of ${deck.deck.name}` });
    await fireEvent.click(picker);
    await fireEvent.click(screen.getByRole('option', { name: languageName('en', 'en') }));
    await waitFor(() => expect(screen.getByTestId('deck-language').textContent).toContain('Language'));
    const stored = await api.call('settings.get', { scope: { kind: 'deck', id: deck.deck.id }, keys: [LANGUAGE_KEY] });
    expect(stored.values[0]?.value).toBe('en');
    await api.call('settings.set', { scope: { kind: 'deck', id: deck.deck.id }, key: LANGUAGE_KEY, value: '' });
  });

  it('nothing about languages while the words module is off', async () => {
    const decks = (await api.deck.tree()).roots;
    await api.call('modules.set_enabled', { id: 'words', enabled: false });
    try {
      const { asked } = stubStats(5);
      render(LibraryScreen, { ctx: ctx({ decks }) });
      await screen.findAllByTestId('deck-row');
      await new Promise((r) => setTimeout(r, 20));
      expect(asked).not.toContain('words.stats');
      expect(screen.queryByTestId('library-unassigned')).toBeNull();
    } finally {
      await api.call('modules.set_enabled', { id: 'words', enabled: true });
    }
  });
});
