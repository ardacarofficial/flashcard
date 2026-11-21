import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { api } from '$lib/api';
import { i18n } from '$lib/i18n/index.svelte';
import { openQuickAdd } from '$lib/shell/commands.svelte';
import { classicColumn, splitTags } from '../notes/create';
import type { ScreenCtx } from '../types';
import QuickAddDialog from './QuickAddDialog.svelte';
import QuickAddHost from './QuickAddHost.svelte';

i18n.locale = 'en';

async function ctxWith(over: Partial<ScreenCtx> = {}): Promise<ScreenCtx> {
  return {
    decks: (await api.deck.tree()).roots,
    selectedDeck: null,
    selectedPath: null,
    selectDeck: () => {},
    guarded: async () => true,
    navigate: () => {},
    ...over,
  };
}

describe('quick add helpers', () => {
  it('splits tags on commas and spaces', () => {
    expect(splitTags(' verb, irregular  a1 ')).toEqual(['verb', 'irregular', 'a1']);
    expect(splitTags('')).toEqual([]);
  });

  it('captions Classic as Term / Definition only', () => {
    expect(classicColumn(null, 0)).toBeNull();
  });
});

describe('<QuickAddDialog> on the dev mock', () => {
  it('Ctrl+Enter adds one note, keeps the dialog open, clears the fields and counts', async () => {
    const ctx = await ctxWith();
    const spy = vi.spyOn(api, 'call');
    render(QuickAddDialog, { open: true, ctx });
    const term = (await screen.findByLabelText(/^Term/)) as HTMLTextAreaElement;
    await fireEvent.input(term, { target: { value: 'quixotic-test' } });
    // Enter moves on to the next field…
    await fireEvent.keyDown(term, { key: 'Enter' });
    const def = screen.getByLabelText(/^Definition/) as HTMLTextAreaElement;
    await fireEvent.input(def, { target: { value: 'hayalperest' } });
    await fireEvent.input(screen.getByLabelText('Tags'), { target: { value: 'adj, b2' } });
    // …and Ctrl+Enter adds from anywhere.
    await fireEvent.keyDown(def, { key: 'Enter', ctrlKey: true });

    await waitFor(() => expect(screen.getByTestId('quick-add-count').textContent).toContain('1'));
    const sent = spy.mock.calls.filter(([m]) => m === 'notes.create');
    expect(sent).toHaveLength(1);
    expect(sent[0]![1]).toMatchObject({ fields: { front: 'quixotic-test', back: 'hayalperest' }, tags: ['adj', 'b2'] });
    expect((screen.getByLabelText(/^Term/) as HTMLTextAreaElement).value).toBe('');
    expect(screen.getByRole('dialog')).toBeTruthy();
    // Tags stay for the next card.
    expect((screen.getByLabelText('Tags') as HTMLInputElement).value).toBe('adj, b2');
    spy.mockRestore();
  });

  it('Esc with typed text asks first; a second Esc closes', async () => {
    const ctx = await ctxWith();
    const result = render(QuickAddDialog, { open: true, ctx });
    const term = await screen.findByLabelText(/^Term/);
    await fireEvent.input(term, { target: { value: 'half typed' } });
    await fireEvent.keyDown(term, { key: 'Escape' });
    expect(await screen.findByText(/Press Esc again/)).toBeTruthy();
    await fireEvent.keyDown(term, { key: 'Escape' });
    await waitFor(() => expect(screen.queryByRole('dialog')).toBeNull());
    result.unmount();
  });

  it('a duplicate offers "Add anyway"', async () => {
    const ctx = await ctxWith();
    render(QuickAddDialog, { open: true, ctx });
    const add = async () => {
      await fireEvent.input(screen.getByLabelText(/^Term/), { target: { value: 'twice-added' } });
      await fireEvent.keyDown(screen.getByLabelText(/^Term/), { key: 'Enter', ctrlKey: true });
    };
    await screen.findByLabelText(/^Term/);
    await add();
    await waitFor(() => expect(screen.getByTestId('quick-add-count')).toBeTruthy());
    await add();
    expect(await screen.findByRole('button', { name: 'Add anyway' })).toBeTruthy();
    // The text stays so it can be changed or added anyway.
    expect((screen.getByLabelText(/^Term/) as HTMLTextAreaElement).value).toBe('twice-added');
  });
});

describe('<QuickAddHost>', () => {
  it('takes over the shell\'s "add a card" while enabled', async () => {
    const ctx = await ctxWith();
    const { unmount } = render(QuickAddHost, { ctx, enabled: true });
    expect(openQuickAdd()).toBe(true);
    expect(await screen.findByRole('dialog', {}, { timeout: 3000 })).toBeTruthy();
    unmount();
    expect(openQuickAdd()).toBe(false);
  });
});
