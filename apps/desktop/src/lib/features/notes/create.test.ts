import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '@testing-library/svelte';
import { api, CLASSIC_NOTETYPE_ID } from '$lib/api';
import type { ImportPreview, NoteTypeDto } from '$lib/api/types';
import { i18n } from '$lib/i18n/index.svelte';
import type { ScreenCtx } from '../types';
import { appendRows, blankRow, editableFields, rowsFromPreview, toDrafts } from './create';
import CreateScreen from './CreateScreen.svelte';

i18n.locale = 'en';

const classic: NoteTypeDto = {
  id: CLASSIC_NOTETYPE_ID,
  name: 'Classic',
  kind: 'standard',
  fields: [
    { key: 'front', name: 'Front', kind: 'rich_text', required: true, unique: true, searchable: true, default_value: null },
    { key: 'back', name: 'Back', kind: 'rich_text', required: false, unique: false, searchable: true, default_value: null },
  ],
  templates: [],
  css: '',
  sort_field: 'front',
  version: 1,
  builtin: 'classic',
  updated_at: 0,
};

describe('create rows', () => {
  it('sends every editable field; an image goes after the last text field', () => {
    const row = { ...blankRow(), fields: { front: 'cat' }, image: '![](media:' + 'a'.repeat(64) + ')' };
    expect(editableFields(classic).map((f) => f.key)).toEqual(['front', 'back']);
    expect(toDrafts([row], classic)).toEqual([{ fields: { front: 'cat', back: row.image } }]);
  });

  it('maps pasted columns as suggested and replaces trailing blank rows', () => {
    const preview = {
      rows: [
        ['kedi', 'cat'],
        ['köpek', 'dog'],
      ],
      suggested_columns: [
        { kind: 'field', key: 'back' },
        { kind: 'field', key: 'front' },
      ],
    } as unknown as ImportPreview;
    const pasted = rowsFromPreview(preview);
    expect(pasted.map((r) => r.fields)).toEqual([
      { back: 'kedi', front: 'cat' },
      { back: 'köpek', front: 'dog' },
    ]);
    const kept = { ...blankRow(), fields: { front: 'x' } };
    expect(appendRows([kept, blankRow(), blankRow()], pasted)).toHaveLength(3);
  });
});

function ctxWith(over: Partial<ScreenCtx> = {}): ScreenCtx {
  return {
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
  };
}

describe('<CreateScreen> on the dev mock', () => {
  it('saves a new deck with its rows as ONE notes.create_many call', async () => {
    const decks = (await api.deck.tree()).roots;
    const selectDeck = vi.fn();
    render(CreateScreen, { ctx: ctxWith({ decks, selectDeck }) });
    await fireEvent.input(await screen.findByLabelText(/^Title/), { target: { value: 'Animals' } });
    const terms = await screen.findAllByLabelText(/^Term \d/);
    await fireEvent.input(terms[0]!, { target: { value: 'cat' } });
    await fireEvent.input(screen.getByLabelText('Definition 1'), { target: { value: 'kedi' } });
    // Enter on the last field of the last row adds a row.
    await fireEvent.input(terms[2]!, { target: { value: 'dog' } });
    await fireEvent.keyDown(screen.getByLabelText('Definition 3'), { key: 'Enter' });
    expect(await screen.findByLabelText('Term 4')).toBeTruthy();

    const spy = vi.spyOn(api, 'call');
    await fireEvent.click(screen.getAllByRole('button', { name: 'Create' }).at(-1)!);
    await waitFor(() => expect(selectDeck).toHaveBeenCalled());
    const sent = spy.mock.calls.filter(([m]) => m === 'notes.create_many');
    expect(sent).toHaveLength(1);
    expect(spy.mock.calls.some(([m]) => m === 'notes.create')).toBe(false);
    const req = sent[0]![1] as { target: { kind: string; path: string }; notes: { fields: Record<string, string> }[] };
    expect(req.target).toMatchObject({ kind: 'new_deck', path: 'Animals' });
    expect(req.notes.map((n) => n.fields.front)).toEqual(['cat', '', 'dog', '']);
    spy.mockRestore();

    const tree = (await api.deck.tree()).roots;
    const animals = tree.find((n) => n.deck.name === 'Animals')!;
    const notes = await api.note.list(animals.deck.id);
    expect(notes.items.map((n) => n.sort_field).sort()).toEqual(['cat', 'dog']);
  });

  it('Ctrl+Enter in a cell saves (it is not a cell move); without a title it says so', async () => {
    const decks = (await api.deck.tree()).roots;
    const selectDeck = vi.fn();
    render(CreateScreen, { ctx: ctxWith({ decks, selectDeck }) });
    const terms = await screen.findAllByLabelText(/^Term \d/);
    await fireEvent.input(terms[0]!, { target: { value: 'owl' } });
    await fireEvent.keyDown(terms[0]!, { key: 'Enter', ctrlKey: true });
    expect(await screen.findByText('Give the new deck a title.')).toBeTruthy();

    await fireEvent.input(screen.getByLabelText(/^Title/), { target: { value: 'Birds' } });
    const spy = vi.spyOn(api, 'call');
    await fireEvent.keyDown(screen.getByLabelText('Term 1'), { key: 'Enter', ctrlKey: true });
    await waitFor(() => expect(spy.mock.calls.some(([m]) => m === 'notes.create_many')).toBe(true));
    spy.mockRestore();
  });
});

describe('paste to import', () => {
  // v1.3: the separators are their own control (shared by Paste and From a file).
  it.each([
    ['\t', 'Tab'],
    [' - ', 'Dash'],
    ['  ', 'Spaces'],
    [',', 'Comma'],
  ])('the separator control shows the separator Rust detected (%j → %s)', async (sep, label) => {
    const { default: Separators } = await import('../io/Separators.svelte');
    render(Separators, {
      options: { field_separator: sep, record_separator: '\n', quotes: true, max_fields: null },
      confidence: 0.9,
      onchange: () => {},
    });
    expect(screen.getByText(new RegExp(`^${label} between fields`))).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Change' }));
    const group = await screen.findByRole('radiogroup', { name: 'Between term and definition' }, { timeout: 2000 });
    const checked = Array.from(group.querySelectorAll('[aria-checked="true"]'), (b) => b.textContent?.trim());
    expect(checked).toEqual([label]);
  });
});

describe('dev mock of io.import.preview', () => {
  it.each([
    ['kedi\tcat\nköpek\tdog', '\t'],
    ['kedi  cat\nköpek  dog', '  '],
    ['kedi - cat\nköpek - dog', ' - '],
  ])('detects the separator like fc-interop instead of always answering comma (%j)', async (text, sep) => {
    const p = await api.call('io.import.preview', {
      source: { kind: 'text', text },
      format: 'text',
      text_options: null,
      notetype_id: CLASSIC_NOTETYPE_ID,
      deck_id: null,
      limit: 10,
    });
    expect(p.text_options?.field_separator).toBe(sep);
    expect(p.rows).toEqual([
      ['kedi', 'cat'],
      ['köpek', 'dog'],
    ]);
  });
});
