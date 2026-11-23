import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { i18n } from '$lib/i18n/index.svelte';
import { api } from '$lib/api';
import WordsScreen from './WordsScreen.svelte';
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
  navigate: () => {},
};

describe('<WordsScreen> on the dev mock', () => {
  it('shows the counts of words.stats on the status chips', async () => {
    const stats = await api.call('words.stats', {});
    const c = stats.languages[0]!.counts;
    render(WordsScreen, { ctx });
    expect((await screen.findByTestId('words-count-new')).textContent).toBe(String(c.new));
    expect(screen.getByTestId('words-count-learning').textContent).toBe(String(c.learning));
    expect(screen.getByTestId('words-count-known').textContent).toBe(String(c.known));
    expect(screen.getByTestId('words-count-ignored').textContent).toBe(String(c.ignored));
    expect(screen.getByTestId('words-count-all').textContent).toBe(String(c.new + c.learning + c.known + c.ignored));
    // Notes without a language are named, not counted.
    expect(screen.getByTestId('words-unassigned').textContent).toContain('12 cards have no language');
    expect((await screen.findAllByTestId('word-row')).length).toBeGreaterThan(5);
  });

  it('opens a word: status sentence, meanings, level and where it was seen', async () => {
    const spy = vi.spyOn(api, 'call');
    render(WordsScreen, { ctx });
    const row = (await screen.findAllByTestId('word-row')).find((r) => r.textContent?.includes('give up'))!;
    await fireEvent.click(row);
    const panel = await screen.findByTestId('word-detail');
    expect(within(panel).getByTestId('word-say').textContent).toBe("You're learning it: it becomes known as you study its card.");
    // Meanings come from words.lookup (field roles); nothing from reading mode.
    expect((await within(panel).findByTestId('word-meanings')).textContent).toContain('vazgeçmek');
    expect(spy.mock.calls.map((c) => c[0]).filter((m) => String(m).startsWith('reading.'))).toEqual([]);
    spy.mockRestore();
    expect(await within(panel).findByText('Level B1')).toBeTruthy();
    expect((await within(panel).findByTestId('word-encounter')).textContent).toContain('Breaking Bad · S1E3');
    // Esc closes the panel.
    await fireEvent.keyDown(window, { key: 'Escape' });
    await waitFor(() => expect(screen.queryByTestId('word-detail')).toBeNull());
  });

  it('filters by typing and moves with the arrow keys', async () => {
    render(WordsScreen, { ctx });
    await screen.findAllByTestId('word-row');
    const box = screen.getByRole('searchbox');
    await fireEvent.input(box, { target: { value: 'dead' } });
    await waitFor(() => expect(screen.getAllByTestId('word-row')).toHaveLength(1));
    await fireEvent.keyDown(box, { key: 'ArrowDown' });
    await fireEvent.keyDown(box, { key: 'Enter' });
    expect((await screen.findByTestId('word-detail')).textContent).toContain('deadline');
  });

  it('"I already know it" calls words.mark and reads list and stats again', async () => {
    const spy = vi.spyOn(api, 'call');
    render(WordsScreen, { ctx });
    const row = (await screen.findAllByTestId('word-row')).find((r) => r.textContent?.includes('deadline'))!;
    await fireEvent.click(row);
    const panel = await screen.findByTestId('word-detail');
    spy.mockClear();
    await fireEvent.click(within(panel).getByRole('button', { name: 'I already know it' }));
    await waitFor(() =>
      expect(spy).toHaveBeenCalledWith('words.mark', { language: 'en', lemmas: ['deadline'], mark: 'known' }),
    );
    await waitFor(() => {
      const methods = spy.mock.calls.map((c) => c[0]);
      expect(methods).toContain('words.stats');
      expect(methods).toContain('words.list');
    });
    // The row now says "known" (the mock's words.mark changes it).
    await waitFor(() => expect(within(screen.getByTestId('word-detail')).getByTestId('word-say').textContent).toBe(
      "You said you know it; if it has a card, it's still asked.",
    ));
    await api.call('words.mark', { language: 'en', lemmas: ['deadline'], mark: null });
    spy.mockRestore();
  });
});
