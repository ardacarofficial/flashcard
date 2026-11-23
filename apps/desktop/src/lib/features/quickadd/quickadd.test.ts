import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { api } from '$lib/api';
import { i18n } from '$lib/i18n/index.svelte';
import { openQuickAdd } from '$lib/shell/commands.svelte';
import { classicColumn, splitTags } from '../notes/create';
import { takeAddWay } from '../notes/add-way.svelte';
import type { NoteTypeDto } from '$lib/api/types';
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

  it('opens on the deck the caller names (a Library row "Add card") without selecting it', async () => {
    const selectDeck = vi.fn();
    const ctx = await ctxWith({ selectDeck });
    const target = ctx.decks[ctx.decks.length - 1]!;
    const spy = vi.spyOn(api, 'call');
    const { unmount } = render(QuickAddHost, { ctx, enabled: true });
    expect(openQuickAdd({ deckId: target.deck.id })).toBe(true);
    const term = (await screen.findByLabelText(/^Term/, {}, { timeout: 3000 })) as HTMLTextAreaElement;
    await fireEvent.input(term, { target: { value: 'preselected-deck' } });
    await fireEvent.keyDown(term, { key: 'Enter', ctrlKey: true });
    await waitFor(() => expect(spy.mock.calls.some(([m]) => m === 'notes.create')).toBe(true));
    const sent = spy.mock.calls.find(([m]) => m === 'notes.create')!;
    expect(sent[1]).toMatchObject({ deck_id: target.deck.id });
    // The page behind stays where it was (no deck page opened).
    expect(selectDeck).not.toHaveBeenCalled();
    spy.mockRestore();
    unmount();
  });

  it('v1.3: "Paste" / "From a file" / "Bulk" in the footer open the add page on the chosen deck', async () => {
    const navigate = vi.fn();
    const selectDeck = vi.fn();
    const ctx = await ctxWith({ navigate, selectDeck });
    const target = ctx.decks[0]!;
    render(QuickAddDialog, { open: true, ctx, deck: target.deck.id });
    await screen.findByLabelText(/^Term/);
    const ways = screen.getByRole('group', { name: 'Other ways:' });
    expect(Array.from(ways.querySelectorAll('button'), (b) => b.textContent?.trim())).toEqual(['Row by row', 'Paste a list', 'From a file']);
    await fireEvent.click(screen.getByRole('button', { name: 'Paste a list' }));
    expect(navigate).toHaveBeenCalledWith('/notes/new');
    expect(selectDeck).toHaveBeenCalledWith(target.deck.id);
    expect(takeAddWay()).toEqual({ way: 'paste', deckId: target.deck.id });
  });

  it('v1.3: a vocabulary type asks term, meaning and an optional example first; a duplicate is explained', async () => {
    const vocab: NoteTypeDto = {
      id: 'nt-vocab',
      name: 'Vocabulary',
      kind: 'standard',
      fields: [
        { key: 'word', name: 'Word', kind: 'rich_text', required: false, unique: true, searchable: true, default_value: null, role: 'term' },
        { key: 'meaning', name: 'Meaning', kind: 'rich_text', required: true, unique: false, searchable: true, default_value: null, role: 'meaning' },
        { key: 'pronunciation', name: 'Pronunciation', kind: 'text', required: false, unique: false, searchable: true, default_value: null },
        // Roles as the backend sends them: the example field is picked by its role, not its key.
        { key: 'example', name: 'Example sentence', kind: 'rich_text', required: false, unique: false, searchable: true, default_value: null, role: 'example' },
      ],
      templates: [],
      css: '',
      sort_field: 'word',
      version: 1,
      builtin: 'vocabulary',
      updated_at: 0,
    };
    localStorage.setItem('fc.cards.last_notetype', vocab.id);
    const real = api.call.bind(api);
    const spy = vi.spyOn(api, 'call').mockImplementation(((method: string, req: never) => {
      if (method === 'notetypes.list') return Promise.resolve({ notetypes: [vocab] });
      if (method === 'notes.create') return Promise.reject({ code: 'CONFLICT_NOTE_DUPLICATE', messageKey: 'errors.conflict.note_duplicate', details: {} });
      return real(method as never, req);
    }) as typeof api.call);
    const ctx = await ctxWith();
    render(QuickAddDialog, { open: true, ctx, deck: ctx.decks[0]!.deck.id });
    const word = await screen.findByLabelText('Word');
    expect(screen.getByLabelText(/^Meaning/)).toBeTruthy();
    expect(screen.getByLabelText('Example sentence (optional)')).toBeTruthy();
    // Pronunciation waits under "More".
    expect(screen.queryByLabelText('Pronunciation')).toBeNull();
    await fireEvent.input(word, { target: { value: 'go' } });
    await fireEvent.keyDown(word, { key: 'Enter', ctrlKey: true });
    expect(await screen.findByText(/“go” is already there: a card with the same Word exists/)).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Add anyway' })).toBeTruthy();
    spy.mockRestore();
    localStorage.removeItem('fc.cards.last_notetype');
  });
});
