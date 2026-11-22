import { err, menu, module, settingsPage, type MockEnv, type MockFeature } from '$lib/api/mock/env';
import type { ApiError, CreateNote, NoteDto } from '$lib/api/types';

export default {
  modules: [
    module('notes', [
      menu('notes.add', 'nav.add_note', '/notes/new', 20),
      menu('notes.list', 'nav.notes', '/notes', 30),
    ]),
  ],
  handlers: ({ now, id, emit, notes, decks, requireDeck, requireName, addDeck, sameName, pathOf, undoLog }: MockEnv) => {
  /** v1.3: one undo step removes these notes (and a deck made for them). */
  const undoable = (ids: string[], deckId: string | null = null) =>
    undoLog.push({
      summary: 'add.summary.undone',
      revert: () => {
        for (const noteId of ids) {
          const i = notes.findIndex((n) => n.id === noteId);
          if (i >= 0) notes.splice(i, 1);
        }
        if (ids.length) emit({ type: 'notes_deleted', data: { ids } });
        const d = deckId ? decks.findIndex((x) => x.id === deckId) : -1;
        if (d >= 0) {
          decks.splice(d, 1);
          emit({ type: 'deck_deleted', data: { id: deckId!, affected: { decks: 1, notes: 0, cards: 0 } } });
        }
      },
    });
  const create = (req: CreateNote): NoteDto => {
    requireDeck(req.deck_id);
    if (!req.fields.front?.trim()) {
      throw err('VALIDATION_FIELD_REQUIRED', 'errors.validation.field_required', 'front');
    }
    const front = req.fields.front.trim().toLocaleLowerCase();
    if (!req.allow_duplicate && notes.some((n) => n.notetype_id === req.notetype_id && n.fields.front?.trim().toLocaleLowerCase() === front)) {
      throw err('CONFLICT_NOTE_DUPLICATE', 'errors.conflict.note_duplicate', front);
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
  };
  return {
  'notes.create': (req) => {
    const n = create(req);
    undoable([n.id]);
    return n;
  },

  // v1.1: a set from the Create screen (the real backend makes it one undo step).
  'notes.create_many': (req) => {
    const blank = (f: Record<string, string>) => Object.values(f).every((v) => !v.trim());
    if (req.notes.length === 0 || req.notes.length > 1000) {
      throw err('VALIDATION_INVALID_PARAMS', 'errors.validation.invalid_params', 'notes');
    }
    if (req.notes.every((d) => blank(d.fields))) {
      throw err('VALIDATION_FIELD_REQUIRED', 'errors.validation.field_required', 'notes');
    }
    let deckId: string;
    let made: string | null = null;
    if (req.target.kind === 'deck') {
      deckId = requireDeck(req.target.id).id;
    } else {
      const name = requireName(req.target.path);
      const parent = req.target.parent_id ? requireDeck(req.target.parent_id) : null;
      if (decks.some((d) => d.parent_id === (parent?.id ?? null) && sameName(d.name, name))) {
        throw err('CONFLICT_DECK_EXISTS', 'errors.conflict.deck_exists', name);
      }
      const d = addDeck(name, parent);
      d.description = req.target.description ?? '';
      emit({ type: 'deck_created', data: { deck: { ...d, path: pathOf(d) } } });
      deckId = d.id;
      made = d.id;
    }
    const out = { deck_id: deckId, note_ids: [] as string[], empty_rows: [] as number[], duplicate_rows: [] as number[] };
    req.notes.forEach((draft, row) => {
      if (blank(draft.fields)) {
        out.empty_rows.push(row);
        return;
      }
      try {
        const n = create({ notetype_id: req.notetype_id, deck_id: deckId, fields: draft.fields, tags: req.tags, attributes: req.attributes, allow_duplicate: req.allow_duplicate ?? null });
        out.note_ids.push(n.id);
      } catch (e) {
        if ((e as ApiError).code === 'CONFLICT_NOTE_DUPLICATE') out.duplicate_rows.push(row);
        else throw { ...(e as ApiError), details: { ...(e as ApiError).details, row } };
      }
    });
    undoable(out.note_ids, made);
    return out;
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
  };
  },
} satisfies MockFeature;
