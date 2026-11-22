/**
 * v1.3 "Add cards": the shared confirmation, the paste flow's preview (on the dev mock: Rust
 * stand-in parsing, the role-aware mapping, an undoable import), the way tabs, and Quick Add's
 * switch to the other ways.
 */
import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { api, CLASSIC_NOTETYPE_ID } from '$lib/api';
import type { FieldDef, NoteTypeDto } from '$lib/api/types';
import { i18n } from '$lib/i18n/index.svelte';
import type { ScreenCtx } from '../types';
import AddedSummary from './AddedSummary.svelte';
import CreateScreen from './CreateScreen.svelte';
import ImportFlow from '../io/ImportFlow.svelte';
import { addWayRequest, requestAddWay, takeAddWay } from './add-way.svelte';

i18n.locale = 'en';

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

const f = (key: string, name: string, extra: Partial<FieldDef> = {}): FieldDef => ({
  key,
  name,
  kind: 'rich_text',
  required: false,
  unique: false,
  searchable: true,
  default_value: null,
  ...extra,
});
/** The dev mock's vocabulary type (io mock maps its fields by this id). */
const vocabulary: NoteTypeDto = {
  id: '01900000-0000-7000-8000-00000000000b',
  name: 'Vocabulary',
  kind: 'standard',
  fields: [f('word', 'Word', { unique: true }), f('meaning', 'Meaning', { required: true }), f('pronunciation', 'Pronunciation', { kind: 'text' }), f('example', 'Example sentence')],
  templates: [],
  css: '',
  sort_field: 'word',
  version: 1,
  builtin: 'vocabulary',
  updated_at: 0,
};

describe('<AddedSummary> (the one confirmation)', () => {
  it('says what was added, updated and skipped (and why); Undo, Open deck, Study now', async () => {
    const selectDeck = vi.fn();
    const navigate = vi.fn();
    const onundo = vi.fn(async () => {});
    render(AddedSummary, {
      ctx: ctxWith({ selectDeck, navigate }),
      added: 12,
      updated: 1,
      skipped: 2,
      reasons: ['2 rows already existed', 'Row 7: A required field is empty.'],
      deckId: 'd1',
      onundo,
    });
    expect(screen.getByText('12 cards added')).toBeTruthy();
    expect(screen.getByText('1 card updated')).toBeTruthy();
    expect(screen.getByText('2 rows skipped')).toBeTruthy();
    expect(screen.getByText('Row 7: A required field is empty.')).toBeTruthy();

    await fireEvent.click(screen.getByRole('button', { name: 'Open deck' }));
    expect(selectDeck).toHaveBeenLastCalledWith('d1');
    expect(navigate).toHaveBeenLastCalledWith('/decks');
    await fireEvent.click(screen.getByRole('button', { name: 'Study now' }));
    expect(navigate).toHaveBeenLastCalledWith('/study');

    await fireEvent.click(screen.getByRole('button', { name: 'Undo' }));
    expect(onundo).toHaveBeenCalledOnce();
    expect(await screen.findByText(/Undone/)).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Study now' })).toBeNull();
  });

  it('nothing added: no deck links, no undo', () => {
    render(AddedSummary, { ctx: ctxWith({ navigate: () => {} }), added: 0, skipped: 3, deckId: 'd1', onundo: async () => {} });
    expect(screen.getByText('No cards added')).toBeTruthy();
    expect(screen.queryByRole('button', { name: 'Undo' })).toBeNull();
    expect(screen.queryByRole('button', { name: 'Open deck' })).toBeNull();
  });
});

describe('Paste: text → detected separator → preview → add (dev mock)', () => {
  it('5 lines "word<TAB>meaning<TAB>sentence" land in word / meaning / example; one undo removes them', async () => {
    const deck = (await api.deck.create('Paste test')).id;
    const navigate = vi.fn();
    const destination = { ready: true, deckId: deck, packageParent: deck, ensure: async () => ({ id: deck, created: false }) };
    render(ImportFlow, { ctx: ctxWith({ navigate }), kind: 'paste', nt: vocabulary, notetypes: [vocabulary], onchoosetype: () => {}, destination });
    const lines = [
      ['go', 'gitmek', 'I go to school by bus.'],
      ['book', 'kitap', 'This book is about a small village.'],
      ['eat', 'yemek', 'We eat dinner at seven.'],
      ['happy', 'mutlu', 'The children are happy today.'],
      ['water', 'su', 'Can I have a glass of water, please?'],
    ];
    await fireEvent.input(screen.getByRole('textbox', { name: 'Paste your list here' }), {
      target: { value: lines.map((l) => l.join('\t')).join('\n') },
    });
    expect(await screen.findByText('5 cards found', { exact: false }, { timeout: 3000 })).toBeTruthy();
    expect(screen.getByText(/^Tab between fields, New line between cards/)).toBeTruthy();
    // The columns are pre-filled by role (not "pronunciation" for the third one).
    const heads = screen.getAllByRole('combobox', { name: /^Column \d/ });
    expect(heads.map((h) => h.textContent?.trim())).toEqual(['Word · term', 'Meaning · meaning', 'Example sentence · example sentence']);
    expect(screen.getByText('I go to school by bus.')).toBeTruthy();

    const spy = vi.spyOn(api, 'call');
    await fireEvent.click(screen.getByRole('button', { name: /Add 5 cards/ }));
    expect(await screen.findByText('5 cards added')).toBeTruthy();
    const start = spy.mock.calls.find(([m]) => m === 'io.import.start')![1] as { columns: unknown; notetype_id: string; skip_first_row: boolean };
    expect(start).toMatchObject({
      notetype_id: vocabulary.id,
      skip_first_row: false,
      columns: [
        { kind: 'field', key: 'word' },
        { kind: 'field', key: 'meaning' },
        { kind: 'field', key: 'example' },
      ],
    });
    spy.mockRestore();
    const notes = (await api.note.list(deck)).items;
    expect(notes.map((n) => n.sort_field).sort()).toEqual(['book', 'eat', 'go', 'happy', 'water']);

    // The text box is cleared for the next list; the confirmation stays.
    expect((screen.getByRole('textbox', { name: 'Paste your list here' }) as HTMLTextAreaElement).value).toBe('');
    await fireEvent.click(screen.getByRole('button', { name: 'Study now' }));
    expect(navigate).toHaveBeenCalledWith('/study');
    await fireEvent.click(screen.getByRole('button', { name: 'Undo' }));
    await waitFor(async () => expect((await api.note.list(deck)).items).toHaveLength(0));
  });

  it('a CSV header is recognised and skipped; a column Classic cannot hold is flagged with a way out', async () => {
    const deck = (await api.deck.create('Header test')).id;
    const classic = (await api.call('notetypes.list', {})).notetypes.find((n) => n.id === CLASSIC_NOTETYPE_ID)!;
    const chosen = vi.fn();
    const destination = { ready: true, deckId: deck, packageParent: deck, ensure: async () => ({ id: deck, created: false }) };
    render(ImportFlow, { ctx: ctxWith(), kind: 'paste', nt: classic, notetypes: [classic, vocabulary], onchoosetype: chosen, destination });
    await fireEvent.input(screen.getByRole('textbox', { name: 'Paste your list here' }), {
      target: { value: 'word,meaning,sentence\ngo,gitmek,I go home.' },
    });
    expect(await screen.findByText('1 card found', { exact: false }, { timeout: 3000 })).toBeTruthy();
    expect((screen.getByRole('checkbox', { name: 'First row is a header' }) as HTMLInputElement).checked).toBe(true);
    expect(screen.getByText(/Column 3 goes to no field/)).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: /Switch to “Vocabulary”/ }));
    expect(chosen).toHaveBeenCalledWith(vocabulary.id);
  });
});

describe('<CreateScreen> ways in', () => {
  it('four ways as tabs; a request (Quick Add → Paste) opens that way on the deck', async () => {
    const decks = (await api.deck.tree()).roots;
    render(CreateScreen, { ctx: ctxWith({ decks, navigate: () => {} }) });
    const tabs = await screen.findByRole('tablist', { name: 'How do you want to add' });
    expect(within(tabs).getAllByRole('tab').map((t) => t.textContent?.trim())).toEqual(['One card', 'Bulk', 'Paste', 'From a file']);
    requestAddWay('paste', decks[0]!.deck.id);
    expect(await screen.findByRole('textbox', { name: 'Paste your list here' })).toBeTruthy();
    expect(screen.getByRole('tab', { name: 'Paste' }).getAttribute('aria-selected')).toBe('true');
    expect(takeAddWay()).toBeNull();
    await fireEvent.click(screen.getByRole('tab', { name: 'From a file' }));
    expect(await screen.findByText('Drop the file here')).toBeTruthy();
  });

  it('the request survives until the page opens (Quick Add navigates after asking)', () => {
    requestAddWay('file', 'd9');
    const seq = addWayRequest().seq;
    expect(takeAddWay()).toEqual({ way: 'file', deckId: 'd9' });
    expect(takeAddWay()).toBeNull();
    expect(addWayRequest().seq).toBe(seq);
  });
});
