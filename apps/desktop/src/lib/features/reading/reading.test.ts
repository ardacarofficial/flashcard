import { describe, expect, it } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { i18n } from '$lib/i18n/index.svelte';
import type { ScreenCtx } from '../types';
import ReadingScreen from './ReadingScreen.svelte';
import { clock, lines, segments } from './text';

i18n.locale = 'en';

const ctx = (deck: string | null): ScreenCtx => ({
  decks: [],
  selectedDeck: deck,
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
});

describe('reading helpers', () => {
  it('slices words and gaps by UTF-16 offsets from the backend', () => {
    const text = '🙂 Hi there';
    const segs = segments(text, [
      { start: 3, end: 5, lemma: 'hi', status: 'new' },
      { start: 6, end: 11, lemma: 'there', status: 'known' },
    ]);
    expect(segs.map((s) => s.text)).toEqual(['🙂 ', 'Hi', ' ', 'there']);
    expect(segs[1]!.token?.lemma).toBe('hi');
    expect(lines('a\nbc')).toEqual([
      { start: 0, end: 1 },
      { start: 2, end: 4 },
    ]);
    expect(clock(62_345)).toBe('1:02');
  });
});

describe('reading screen on the dev mock', () => {
  it('paste a text, see colours and the known share, mark and make a card', async () => {
    render(ReadingScreen, { ctx: ctx('deck-1') });
    expect(await screen.findByText('No texts yet. Paste one or open a file.')).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'New text' }));
    await fireEvent.input(screen.getByLabelText('Text'), { target: { value: 'The cat runs. The dog sleeps.' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Save and read' }));

    // The reader: every word starts new.
    expect(await screen.findByText('You know 0% of this text')).toBeTruthy();
    expect(screen.getByText('New: 5')).toBeTruthy();
    const cat = screen.getByRole('button', { name: 'cat' });
    expect(cat.className).toContain('new');

    // "I know this" on "the" → known, share goes up (backend numbers).
    await fireEvent.click(screen.getAllByRole('button', { name: 'The' })[0]!);
    await fireEvent.click(await screen.findByRole('button', { name: 'I know this' }));
    expect(await screen.findByText('You know 33% of this text')).toBeTruthy();

    // A word card for "cat" with its sentence.
    await fireEvent.click(screen.getByRole('button', { name: 'cat' }));
    await waitFor(() => expect((screen.getByRole('textbox', { name: 'Word' }) as HTMLInputElement).value).toBe('cat'));
    expect(screen.getByText('The cat runs.')).toBeTruthy();
    await fireEvent.input(screen.getByLabelText('Meaning'), { target: { value: 'kedi' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Make card' }));
    expect(await screen.findByText('Card added.')).toBeTruthy();
    await waitFor(() => expect(screen.getByRole('button', { name: 'cat' }).className).toContain('learning'));

    // Done for today → back to the library, the text is queued.
    await fireEvent.click(screen.getByRole('button', { name: 'Done for today' }));
    expect(await screen.findByText('Next in 1 day(s)')).toBeTruthy();
    await fireEvent.click(screen.getByRole('tab', { name: /Reading queue/ }));
    expect(await screen.findByRole('button', { name: 'The cat runs. The dog sleeps.' })).toBeTruthy();
  });

  it('mounts the inline card composer of the modes track in the reader (one undo step)', async () => {
    render(ReadingScreen, { ctx: ctx('deck-1') });
    await fireEvent.click(await screen.findByRole('button', { name: 'New text' }));
    await fireEvent.input(screen.getByLabelText('Text'), { target: { value: 'Porsuklar gece avlanır.' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Save and read' }));
    await fireEvent.click(await screen.findByRole('button', { name: 'New cards' }));
    const area = await screen.findByRole('textbox', { name: 'New cards from text' });
    await fireEvent.input(area, { target: { value: 'porsuk :: badger\ngece :: night' } });
    const create = await screen.findByRole('button', { name: /Create 2 card\(s\)/ });
    await fireEvent.click(create);
    expect(await screen.findByText('2 card(s) created.')).toBeTruthy();
    // Still reading the same text.
    expect(screen.getByText(/You know \d+% of this text/)).toBeTruthy();
  });

  it('without a deck the card button waits for one', async () => {
    render(ReadingScreen, { ctx: ctx(null) });
    await fireEvent.click(await screen.findByRole('button', { name: 'New text' }));
    await fireEvent.input(screen.getByLabelText('Text'), { target: { value: 'Kediler koşuyor.' } });
    await fireEvent.input(screen.getByLabelText('Language'), { target: { value: 'tr' } });
    await fireEvent.click(screen.getByRole('button', { name: 'Save and read' }));
    await fireEvent.click(await screen.findByRole('button', { name: 'Kediler' }));
    expect(await screen.findByText('Pick a deck above first.')).toBeTruthy();
    expect((screen.getByRole('button', { name: 'Make card' }) as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByRole('button', { name: 'New cards' }) as HTMLButtonElement).disabled).toBe(true);
  });
});
