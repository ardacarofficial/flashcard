import { describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor, within } from '@testing-library/svelte';
import { i18n } from '$lib/i18n/index.svelte';
import { api } from '$lib/api';
import NoteTypesScreen from './NoteTypesScreen.svelte';
import type { ScreenCtx } from '../types';

i18n.locale = 'en';

const ctx: ScreenCtx = {
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
};

const typeButton = (name: string) => screen.findByRole('button', { name: new RegExp(`^${name}`) }, { timeout: 5000 });

// Whole flows on the mock (lazy card renderer, debounced preview): generous time on a busy machine.
describe('<NoteTypesScreen> on the dev mock', { timeout: 30000 }, () => {
  it('shows the fields next to a live preview; template code stays closed', async () => {
    render(NoteTypesScreen, { ctx });
    expect(await screen.findByText('Ready-made types', {}, { timeout: 5000 })).toBeTruthy();
    const rows = await screen.findAllByTestId('field-row');
    expect(rows).toHaveLength(2);
    expect(within(rows[0]!).getByText('Formatted text (bold, lists, pictures)')).toBeTruthy();
    expect(screen.getByRole('checkbox', { name: 'Required: Ön' })).toBeTruthy();
    expect(screen.getByRole('button', { name: 'Move “Ön” down' })).toBeTruthy();

    // Live preview with a flip button (sample content = the field names).
    const preview = screen.getByTestId('notetype-preview');
    expect(await within(preview).findByText('Front (question)')).toBeTruthy();
    await fireEvent.click(within(preview).getByRole('button', { name: 'Show the back' }));
    expect(within(preview).getByText('Back (answer)')).toBeTruthy();

    // Advanced: template code is closed; the HTML editors are not on screen.
    const toggle = screen.getByRole('button', { name: /Advanced: template code/ });
    expect(toggle.getAttribute('aria-expanded')).toBe('false');
    expect(screen.queryByLabelText('Front (HTML)')).toBeNull();
    // Reordering edits the draft (and enables Save).
    await fireEvent.click(screen.getByRole('button', { name: 'Move “Ön” down' }));
    const names = screen.getAllByTestId('field-row').map((r) => (within(r).getByRole('textbox') as HTMLInputElement).value);
    expect(names).toEqual(['Arka', 'Ön']);
    expect(screen.getByText('You have unsaved changes.')).toBeTruthy();
  });

  it('creates a note type from a starting point (definition) and from a built-in copy', async () => {
    const call = vi.spyOn(api, 'call');
    render(NoteTypesScreen, { ctx });
    await screen.findByText('Ready-made types', {}, { timeout: 5000 });

    await fireEvent.click(screen.getByRole('button', { name: 'New note type' }));
    expect(await screen.findByText('Word + example sentence')).toBeTruthy();
    expect(screen.getByText(/Each note becomes 2 cards, front→back and back→front/)).toBeTruthy();
    await fireEvent.click(screen.getByTestId('starter-word_example'));
    await fireEvent.input(within(screen.getByRole('dialog')).getByLabelText('Name'), { target: { value: 'Phrasal verbs' } });
    await fireEvent.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Create' }));

    await typeButton('Phrasal verbs');
    const req = call.mock.calls.find(([m]) => m === 'notetypes.create')![1] as Record<string, unknown> & {
      definition: { fields: { key: string; name: string; required: boolean }[]; templates: { front: string; back: string }[] };
    };
    expect(req.name).toBe('Phrasal verbs');
    expect(req.copy_from).toBeNull();
    expect(req.definition.fields.map((f) => [f.key, f.name, f.required])).toEqual([
      ['word', 'Word', true],
      ['meaning', 'Meaning', true],
      ['example', 'Example sentence', false],
    ]);
    expect(req.definition.templates[0]!.front).toBe('{{word}}');
    expect(req.definition.templates[0]!.back).toContain('{{example}}');
    // The new type is selected and shown with its three fields.
    await waitFor(() => expect(screen.getAllByTestId('field-row')).toHaveLength(3));

    call.mockClear();
    await fireEvent.click(screen.getByRole('button', { name: 'New note type' }));
    await fireEvent.click(await screen.findByTestId('starter-reverse'));
    await fireEvent.click(within(screen.getByRole('dialog')).getByRole('button', { name: 'Create' }));
    await typeButton('Both ways');
    const reverse = (await api.call('notetypes.list', {})).notetypes.find((n) => n.builtin === 'classic_reverse')!;
    expect(call).toHaveBeenCalledWith('notetypes.create', { name: 'Both ways', copy_from: reverse.id, definition: null, id: null });
  });

  it('says what a destructive change does to existing notes and cards before it happens', async () => {
    const nt = await api.call('notetypes.create', {
      name: 'Idioms',
      copy_from: null,
      id: null,
      definition: {
        name: 'Idioms',
        kind: 'standard',
        fields: [
          { key: 'front', name: 'Idiom', kind: 'rich_text', required: true, unique: true, searchable: true, default_value: null },
          { key: 'back', name: 'Meaning', kind: 'rich_text', required: false, unique: false, searchable: true, default_value: null },
          { key: 'note', name: 'Note', kind: 'text', required: false, unique: false, searchable: true, default_value: null },
        ],
        templates: [
          { key: 'forward', name: 'Forward', front: '{{front}}', back: '{{FrontSide}}<hr>{{back}}', card_type: 'classic' },
          { key: 'reverse', name: 'Reverse', front: '{{back}}', back: '{{FrontSide}}<hr>{{front}}', card_type: 'classic' },
        ],
        css: '',
        sort_field: 'front',
      },
    });
    const deck = (await api.deck.tree()).roots[0]!.deck.id;
    for (const front of ['a piece of cake', 'break the ice', 'hit the sack']) {
      await api.call('notes.create', { notetype_id: nt.id, deck_id: deck, fields: { front, back: 'x', note: 'y' }, tags: [], attributes: [] });
    }

    render(NoteTypesScreen, { ctx });
    await fireEvent.click(await typeButton('Idioms'));
    expect(await screen.findByText('3 notes use this type. Each note becomes up to 2 cards.')).toBeTruthy();

    // Deleting the type: refused while notes use it, and the page says why.
    expect(screen.getByText(/3 notes use this type\. A type with notes can't be deleted/)).toBeTruthy();
    expect((screen.getByRole('button', { name: 'Delete “Idioms”' }) as HTMLButtonElement).disabled).toBe(true);

    // A field shown on the cards cannot just go: the dialog says which template uses it.
    await fireEvent.click(screen.getByRole('button', { name: 'Remove “Meaning”' }));
    expect(await screen.findByText(/This field is shown by the “Forward”, “Reverse” template\. First take \{\{back\}\} out/)).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Got it' }));

    // A field with content in 3 notes: the count and the field are named before confirming.
    await fireEvent.click(screen.getByRole('button', { name: 'Remove “Note”' }));
    expect(await screen.findByText(/When you save, the “Note” content of 3 notes is deleted\./)).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Remove field' }));
    expect(screen.getAllByTestId('field-row')).toHaveLength(2);
    expect(screen.getByText('“Note” will be removed: its content in 3 notes is lost.')).toBeTruthy();

    // A template that has cards cannot be removed (template code is under Advanced).
    await fireEvent.click(screen.getByRole('button', { name: /Advanced: template code/ }));
    expect(screen.getAllByLabelText('Front (HTML)')).toHaveLength(2);
    await fireEvent.click(screen.getByRole('button', { name: 'Remove template “Forward”' }));
    expect(await screen.findByText(/3 cards were made from this template \(counting the trash\)/)).toBeTruthy();
    await fireEvent.click(screen.getByRole('button', { name: 'Got it' }));

    // Saving applies the removal.
    await fireEvent.click(screen.getByRole('button', { name: 'Save note type' }));
    await screen.findByText('Saved.');
    const saved = (await api.call('notetypes.list', {})).notetypes.find((n) => n.id === nt.id)!;
    expect(saved.fields.map((f) => f.key)).toEqual(['front', 'back']);
  });
});
