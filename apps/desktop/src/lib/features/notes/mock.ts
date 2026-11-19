import { err, menu, module, settingsPage, type MockEnv, type MockFeature } from '$lib/api/mock/env';
import type { NoteDto } from '$lib/api/types';

export default {
  modules: [
    module('notes', [
      menu('notes.add', 'nav.add_note', '/notes/new', 20),
      menu('notes.list', 'nav.notes', '/notes', 30),
    ]),
  ],
  handlers: ({ now, id, emit, notes, requireDeck }: MockEnv) => ({
  'notes.create': (req) => {
    requireDeck(req.deck_id);
    if (!req.fields.front?.trim()) {
      throw err('VALIDATION_FIELD_REQUIRED', 'errors.validation.field_required', 'front');
    }
    const t = now();
    const n: NoteDto = {
      id: id('note'),
      notetype_id: req.notetype_id,
      home_deck_id: req.deck_id,
      fields: req.fields,
      tags: req.tags,
      attributes: req.attributes,
      cards: [
        {
          id: id('card'),
          deck_id: req.deck_id,
          template_key: 'forward',
          state: 'new',
          due: null,
          suspended: false,
        },
      ],
      sort_field: req.fields.front,
      created_at: t,
      updated_at: t,
    };
    notes.push(n);
    emit({ type: 'note_created', data: { note_id: n.id, deck_id: req.deck_id } });
    return n;
  },
  'notes.list': ({ deck_id }) => ({
    items: notes
      .filter((n) => !deck_id || n.home_deck_id === deck_id)
      .map((n) => ({
        id: n.id,
        notetype_id: n.notetype_id,
        sort_field: n.sort_field,
        tags: n.tags,
        card_count: n.cards.length,
        updated_at: n.updated_at,
      })),
    next_cursor: null,
  }),
  'notes.delete': ({ ids }) => {
    const gone = ids.filter((noteId) => {
      const i = notes.findIndex((n) => n.id === noteId);
      if (i >= 0) notes.splice(i, 1);
      return i >= 0;
    });
    if (gone.length === 0) {
      throw err('NOT_FOUND_NOTE', 'errors.not_found.note', ids.join(','));
    }
    emit({ type: 'notes_deleted', data: { ids: gone } });
    return { decks: 0, notes: gone.length, cards: gone.length };
  },
  }),
} satisfies MockFeature;
