/**
 * F1 (J7a under load): the edit dialog puts the caret in the first field once the note type
 * has arrived — but never moves it out of a field the user is already typing in (the fields
 * can appear late; the caret must not jump).
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor } from '@testing-library/svelte';
import { api, CLASSIC_NOTETYPE_ID } from '$lib/api';
import { i18n } from '$lib/i18n/index.svelte';
import NoteEditor from './NoteEditor.svelte';
import { trackModules } from './modules.svelte';

i18n.locale = 'en';

/** Holds `notetypes.list` until released (a slow bridge). */
function holdNoteTypes() {
  const call = api.call.bind(api);
  const held: (() => void)[] = [];
  vi.spyOn(api, 'call').mockImplementation(((method: string, req: unknown) => {
    const p = call(method as never, req as never);
    if (method !== 'notetypes.list') return p;
    return p.then((v) => new Promise((resolve) => held.push(() => resolve(v as never))));
  }) as typeof api.call);
  return () => held.splice(0).forEach((go) => go());
}

async function aNote() {
  const deck = await api.call('decks.create', { path: `Focus ${Math.random()}`, parent_id: null });
  const note = await api.call('notes.create', {
    notetype_id: CLASSIC_NOTETYPE_ID,
    deck_id: deck.id,
    fields: { front: 'pencil', back: 'kurşun kalem' },
    tags: [],
    attributes: [],
    allow_duplicate: true,
  });
  return { id: note.id, deckId: deck.id };
}

afterEach(() => {
  vi.restoreAllMocks();
  document.body.innerHTML = '';
});

describe('<NoteEditor autofocus>', () => {
  it('puts the caret in the first field when nothing is focused yet', async () => {
    trackModules();
    const note = await aNote();
    const release = holdNoteTypes();
    render(NoteEditor, { noteId: note.id, deckId: note.deckId, deckName: 'D', autofocus: true });
    const first = await waitFor(() => {
      release();
      const f = document.querySelector<HTMLTextAreaElement>('form textarea');
      expect(f).not.toBeNull();
      return f!;
    });
    await waitFor(() => expect(document.activeElement).toBe(first));
  });

  it('a late note type does not pull the caret out of a field the user is typing in', async () => {
    trackModules();
    const note = await aNote();
    const release = holdNoteTypes();
    const { container } = render(NoteEditor, { noteId: note.id, deckId: note.deckId, deckName: 'D', autofocus: true });
    // While the fields are still loading, the user types in the editor's other text box
    // (the card's own note).
    const own = await waitFor(() => {
      const t = container.querySelector<HTMLTextAreaElement>('aside textarea');
      expect(t).not.toBeNull();
      return t!;
    });
    own.focus();
    expect(document.activeElement).toBe(own);
    await waitFor(() => {
      release();
      expect(container.querySelector('form textarea')).not.toBeNull();
    });
    await new Promise((r) => setTimeout(r, 20));
    expect(document.activeElement).toBe(own);
    expect(screen.queryAllByRole('textbox').length).toBeGreaterThan(1);
  });
});
