import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/svelte';
import type { ImportPreview, JobDto, NoteTypeDto } from '$lib/api/types';
import { i18n } from '$lib/i18n/index.svelte';
import { decodeColumn, encodeColumn, escapeSeparator, unescapeSeparator } from './mapping';

i18n.locale = 'en';

const preview: ImportPreview = {
  format: 'text',
  text_options: { field_separator: ':', record_separator: '\n', quotes: true, max_fields: null },
  confidence: 0.93,
  columns: 2,
  rows: [['Arda', 'porsuk']],
  total_rows: 1,
  suggested_columns: [
    { kind: 'field', key: 'front' },
    { kind: 'field', key: 'back' },
  ],
  package: null,
};
const done: JobDto = {
  id: 'job1',
  kind: 'io.import',
  status: 'succeeded',
  progress: 1,
  message: null,
  error: null,
  result: {
    added_notes: 1,
    updated_notes: 0,
    skipped_duplicates: 0,
    added_cards: 1,
    added_decks: 0,
    added_notetypes: 0,
    added_attributes: 0,
    added_options: 0,
    added_media: 0,
    added_reviews: 0,
    issues: [{ row: 2, message: { key: 'io.issue.no_deck', params: {} } }],
  },
  created_at: 0,
  updated_at: 0,
};
const call = vi.fn(async (method: string, _params: unknown) => {
  switch (method) {
    case 'io.import.preview':
      return preview;
    case 'io.import.start':
      return { ...done, status: 'queued', result: null };
    case 'jobs.get':
      return done;
    case 'cards.preview':
      return { cards: [] };
    case 'oplog.undo':
      return undo();
  }
  throw new Error(method);
});
const undo = vi.fn().mockResolvedValue({});

vi.mock('$lib/api', () => ({
  api: {
    call: (m: string, p: unknown) => call(m, p),
    on: async () => () => {},
    attribute: { effective: async () => ({ attributes: [] }) },
    oplog: { undo: () => undo() },
  },
}));

import ImportFlow from './ImportFlow.svelte';

describe('io mapping helpers', () => {
  it('round-trips column targets and separators', () => {
    for (const c of [
      { kind: 'ignore' },
      { kind: 'tags' },
      { kind: 'field', key: 'front' },
      { kind: 'attribute', attr_id: 'a-1' },
    ] as const) {
      expect(decodeColumn(encodeColumn(c))).toEqual(c);
    }
    expect(escapeSeparator('\t')).toBe('\\t');
    expect(unescapeSeparator('\\t')).toBe('\t');
  });
});

// v1.3: the import lives on the add page (Paste / From a file); same flow, plainer words.
describe('<ImportFlow> (was <ImportPanel>)', () => {
  const ctx = {
    decks: [],
    selectedDeck: 'd1',
    selectedPath: 'Words',
    selectDeck: () => {},
    guarded: async () => true,
  };
  const nt = {
    id: 'classic',
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
  } as NoteTypeDto;
  const destination = { ready: true, deckId: 'd1', packageParent: 'd1', ensure: async () => ({ id: 'd1', created: false }) };

  it('previews pasted text, imports it as a job and offers undo', async () => {
    render(ImportFlow, { ctx, kind: 'paste', nt, notetypes: [nt], onchoosetype: () => {}, destination });
    await fireEvent.input(screen.getByRole('textbox', { name: 'Paste your list here' }), {
      target: { value: 'Arda: porsuk' },
    });
    expect(await screen.findByText('porsuk', {}, { timeout: 3000 })).toBeTruthy();
    expect(screen.getByText(/detected, 93% sure/)).toBeTruthy();
    expect(call).toHaveBeenCalledWith(
      'io.import.preview',
      expect.objectContaining({ source: { kind: 'text', text: 'Arda: porsuk' }, deck_id: 'd1', notetype_id: 'classic' }),
    );

    await fireEvent.click(screen.getByRole('button', { name: /Add 1 card/ }));
    expect(await screen.findByText('1 card added')).toBeTruthy();
    expect(call).toHaveBeenCalledWith(
      'io.import.start',
      expect.objectContaining({
        deck_id: 'd1',
        text_options: preview.text_options,
        columns: preview.suggested_columns,
        duplicates: 'skip',
      }),
    );
    // Why rows were not imported: the shared confirmation lists the backend's reasons.
    expect(screen.getByText('Row 2: No deck given.')).toBeTruthy();

    await fireEvent.click(screen.getByRole('button', { name: 'Undo' }));
    expect(undo).toHaveBeenCalled();
  });

  it('a file is chosen, mapped and imported in three steps (browser: read as text)', async () => {
    render(ImportFlow, { ctx, kind: 'file', nt, notetypes: [nt], onchoosetype: () => {}, destination });
    expect(screen.getByText('Drop the file here')).toBeTruthy();
    const file = new File(['Arda: porsuk'], 'words.csv', { type: 'text/csv' });
    await fireEvent.change(screen.getByTestId('file-input'), { target: { files: [file] } });
    expect(await screen.findByText('words.csv')).toBeTruthy();
    expect(await screen.findByText('porsuk', {}, { timeout: 3000 })).toBeTruthy();
    expect(screen.getByRole('button', { name: /Import 1 card/ })).toBeTruthy();
    // The duplicate rule is explained in plain words (the unique field named).
    expect(screen.getByText(/same Front already exists/)).toBeTruthy();
  });

  it('an Anki package picked in a browser says it needs the app', async () => {
    render(ImportFlow, { ctx, kind: 'file', nt, notetypes: [nt], onchoosetype: () => {}, destination });
    const file = new File(['x'], 'deck.apkg');
    await fireEvent.change(screen.getByTestId('file-input'), { target: { files: [file] } });
    expect(await screen.findByText(/only be opened in the app/)).toBeTruthy();
  });
});
