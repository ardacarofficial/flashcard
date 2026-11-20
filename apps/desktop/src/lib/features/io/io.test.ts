import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/svelte';
import type { ImportPreview, JobDto } from '$lib/api/types';
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

import ImportPanel from './ImportPanel.svelte';

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

describe('<ImportPanel>', () => {
  const ctx = {
    decks: [],
    selectedDeck: 'd1',
    selectedPath: 'Words',
    selectDeck: () => {},
    guarded: async () => true,
  };

  it('previews pasted text, imports it as a job and offers undo', async () => {
    render(ImportPanel, { ctx });
    await fireEvent.input(screen.getByRole('textbox', { name: 'Paste text' }), {
      target: { value: 'Arda: porsuk' },
    });
    await fireEvent.click(screen.getByRole('button', { name: 'Preview' }));
    expect(await screen.findByText('porsuk')).toBeTruthy();
    expect(screen.getByText(/Detected automatically \(93%\)/)).toBeTruthy();
    expect(call).toHaveBeenCalledWith(
      'io.import.preview',
      expect.objectContaining({ source: { kind: 'text', text: 'Arda: porsuk' }, deck_id: 'd1' }),
    );

    await fireEvent.click(screen.getByRole('button', { name: 'Import into Words' }));
    expect(await screen.findByText('1 added, 0 updated, 0 skipped as duplicates.')).toBeTruthy();
    expect(call).toHaveBeenCalledWith(
      'io.import.start',
      expect.objectContaining({
        deck_id: 'd1',
        text_options: preview.text_options,
        columns: preview.suggested_columns,
        duplicates: 'skip',
      }),
    );
    expect(screen.getByRole('listitem').textContent).toContain('Row 2: No deck given.');

    await fireEvent.click(screen.getByRole('button', { name: 'Undo import' }));
    expect(undo).toHaveBeenCalled();
  });
});
