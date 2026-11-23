import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/svelte';
import type { BackupReport, BackupStatus, ImportPreview, JobDto, NoteTypeDto } from '$lib/api/types';
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
// v1.4.1: Settings › Backup with a second backup location.
let backupStatus: BackupStatus;
const backupReport: BackupReport = {
  path: '/home/demo/y.fcbackup',
  bytes: 1,
  notes: 1,
  cards: 1,
  review_log: 0,
  media: 0,
  verified: true,
  missing_media: [],
  mirror: { path: null, error: 'disk full', disk_full: false },
};
const backupDone: JobDto = { ...done, id: 'job-b', kind: 'io.backup', result: backupReport };
const call = vi.fn(async (method: string, _params: unknown) => {
  switch (method) {
    case 'io.backup.status':
      return backupStatus;
    case 'io.backup.create':
      return { ...backupDone, status: 'queued', result: null };
    case 'settings.schema':
      return { settings: [] };
    case 'io.import.preview':
      return preview;
    case 'io.import.start':
      return { ...done, status: 'queued', result: null };
    case 'jobs.get':
      return (_params as { id: string }).id === backupDone.id ? backupDone : done;
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
import BackupPanel from './BackupPanel.svelte';

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

describe('backup settings: second backup location (v1.4.1)', () => {
  const base: BackupStatus = {
    schedule: 'daily',
    keep: 7,
    folder: '/home/demo/backups',
    last_verified: null,
    automatic: [],
    next_due: null,
    last_failure: null,
    stale: null,
    mirror: null,
  };

  it('not set: one muted hint', async () => {
    backupStatus = base;
    render(BackupPanel, {});
    expect(await screen.findByText('No second backup location; you can set another drive below.')).toBeTruthy();
  });

  it('a missing folder and a failed copy are warnings; a failed copy after a backup is said', async () => {
    backupStatus = { ...base, mirror: { folder: 'E:\\Yedek', state: 'missing', copies: [], last_failure: { at: 1, message: 'gone' }, disk_full: false } };
    render(BackupPanel, {});
    const alerts = await screen.findAllByRole('alert');
    expect(alerts.map((a) => a.textContent)).toEqual([
      'The second backup folder was not found: E:\\Yedek. Is the drive connected?',
      expect.stringMatching(/^The last copy to the second location failed \(/),
    ]);

    const prompt = vi.spyOn(window, 'prompt').mockReturnValue('/home/demo/y.fcbackup');
    await fireEvent.click(screen.getByRole('button', { name: 'Create backup…' }));
    expect(await screen.findByText('Backed up, but the copy to the second location failed: disk full')).toBeTruthy();
    expect(screen.getByText('Backup saved to /home/demo/y.fcbackup.')).toBeTruthy();
    prompt.mockRestore();
  });

  it('a full mirror disk is said plainly, the main backup still counts', async () => {
    backupStatus = { ...base, mirror: { folder: 'E:/Yedek', state: 'ok', copies: [], last_failure: { at: 1, message: 'full' }, disk_full: true } };
    render(BackupPanel, {});
    const alerts = await screen.findAllByRole('alert');
    expect(alerts.map((a) => a.textContent)).toEqual([expect.stringMatching(/^The second backup disk is full \(.*\): E:\/Yedek\. Your main backup is fine/)]);
  });

  it('ok: copies and the newest date', async () => {
    const copy = { path: 'E:/Yedek/auto-1.fcbackup', created_at: 1, verified_at: 1, bytes: 1, complete: true, automatic: true };
    backupStatus = { ...base, mirror: { folder: 'E:/Yedek', state: 'ok', copies: [copy, copy], last_failure: null, disk_full: false } };
    render(BackupPanel, {});
    expect(await screen.findByText(/^Second location: ready · 2 copies, last: /)).toBeTruthy();
    expect(screen.getByText('Second folder: E:/Yedek')).toBeTruthy();
    expect(screen.queryByRole('alert')).toBeNull();
  });
});
