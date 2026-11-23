/**
 * v1.4 "This word" in the note editor, on the dev mock: a went/go note's encounters (main one
 * marked, the better example proposed and applied), accepted answers added with Enter and
 * removed with ×, read back from the api.
 */
import { beforeAll, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { api } from '$lib/api';
import { i18n } from '$lib/i18n/index.svelte';
import NoteEditorAnswers from './NoteEditorAnswers.svelte';
import NoteEditorEncounters from './NoteEditorEncounters.svelte';
import { trackModules } from './modules.svelte';

// The words mock also answers `subtitle.encounters.list` (by lemma) and is merged after the
// subtitle mock: send `note_id` reads to the subtitle mock's encounters.
vi.mock('$lib/api/mock/transport', async (original) => {
  const real = await original<typeof import('$lib/api/mock/transport')>();
  const sub = await import('../subtitle/mock');
  return {
    createMockTransport: () => {
      const t = real.createMockTransport();
      return {
        ...t,
        invoke: (command: string, payload: unknown) => {
          const noteId = (payload as { note_id?: string } | null)?.note_id;
          if (command === 'v1.subtitle.encounters.list' && noteId) return Promise.resolve(structuredClone({ encounters: sub.mockEncountersOfNote(noteId) }));
          return t.invoke(command, payload);
        },
      };
    },
  };
});

i18n.locale = 'en';

let noteId = '';
let otherId = '';

beforeAll(async () => {
  trackModules();
  await api.call('modules.set_enabled', { id: 'subtitle', enabled: true });
  const deck = await api.call('decks.create', { path: 'Encounters test', parent_id: null });
  const make = (front: string, back: string) =>
    api.call('notes.create', { notetype_id: '01900000-0000-7000-8000-000000000001', deck_id: deck.id, fields: { front, back }, tags: [], attributes: [], allow_duplicate: true });
  noteId = (await make('went', 'gitmek')).id;
  otherId = (await make('take', 'almak')).id;
});

describe('<NoteEditorEncounters> on the dev mock', { timeout: 20000 }, () => {
  it('lists where the word was seen, marks the main one and applies the better example', async () => {
    render(NoteEditorEncounters, { noteId });
    const rows = await screen.findAllByTestId('encounter-row', {}, { timeout: 5000 });
    expect(rows).toHaveLength(2);
    expect(within(rows[0]!).getByText('Breaking Bad S1E3')).toBeTruthy();
    expect(within(rows[0]!).getByText('12:41')).toBeTruthy();
    expect(within(rows[0]!).getByText('Main example')).toBeTruthy();
    expect(within(rows[0]!).getByText('went').tagName).toBe('MARK');
    expect(within(rows[0]!).getByText('Sound: original')).toBeTruthy();

    // The ranking prefers the other line: "There is a better example: …" → make it the main one.
    const better = await screen.findByTestId('better-example');
    expect(better.textContent).toContain('We went to the beach.');
    await fireEvent.click(within(better).getByRole('button', { name: 'Make it the main example' }));
    await waitFor(async () => {
      const list = (await api.call('subtitle.encounters.list', { note_id: noteId })).encounters;
      expect(list.find((e) => e.is_primary)?.facts.sentence).toBe('We went to the beach.');
    });
    await waitFor(() => expect(screen.queryByTestId('better-example')).toBeNull());
  });

  it('merging with a different word asks first, then merges with force', async () => {
    render(NoteEditorEncounters, { noteId });
    await screen.findAllByTestId('encounter-row', {}, { timeout: 5000 });
    await fireEvent.click(screen.getByRole('button', { name: 'Merge with a card' }));
    const dialog = await screen.findByRole('dialog');
    await fireEvent.input(within(dialog).getByRole('searchbox'), { target: { value: 'take' } });
    const results = await screen.findByTestId('note-picker-results', {}, { timeout: 3000 });
    await fireEvent.click(within(results).getByRole('button', { name: 'Merge with this card' }));
    const confirm = await screen.findByRole('alertdialog');
    expect(confirm.textContent).toContain('These two cards do not look like the same word');
    await fireEvent.click(within(confirm).getByRole('button', { name: 'Merge anyway' }));
    await waitFor(async () => {
      await expect(api.call('notes.get', { id: otherId })).rejects.toMatchObject({ code: 'NOT_FOUND_NOTE' });
    });
  });
});

describe('<NoteEditorAnswers> on the dev mock', { timeout: 20000 }, () => {
  it('adds an answer with Enter and removes it with ×', async () => {
    render(NoteEditorAnswers, { noteId });
    expect(await screen.findByText('Count these as right in tests too')).toBeTruthy();
    const box = screen.getByRole('textbox', { name: 'Add an answer' });
    await fireEvent.input(box, { target: { value: 'gitti' } });
    await fireEvent.keyDown(box, { key: 'Enter' });
    await waitFor(async () => {
      const r = await api.call('accepted_answers.list', { note_id: noteId });
      expect(r.fields.flatMap((f) => f.answers)).toEqual(['gitti']);
    });
    const chip = await screen.findByText('gitti');
    expect(chip).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: /gitti/ }));
    await waitFor(async () => {
      const r = await api.call('accepted_answers.list', { note_id: noteId });
      expect(r.fields.flatMap((f) => f.answers)).toEqual([]);
    });
    await waitFor(() => expect(screen.queryByText('gitti')).toBeNull());
  });
});
