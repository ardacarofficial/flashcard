import { err, menu, module, settingsPage, type Handlers, type MockEnv, type MockFeature } from '$lib/api/mock/env';
import type { AudioFillItem, AudioFillPreview, AudioFillScope, ApiError, CreateNote, JobDto, NoteAcceptedAnswers, NoteDto } from '$lib/api/types';

export default {
  modules: [
    module('notes', [
      menu('notes.add', 'nav.add_note', '/notes/new', 20),
      menu('notes.list', 'nav.notes', '/notes', 30),
    ]),
    module('accepted_answers', []),
    { ...module('audio_fill', []), methods: ['media.audio.fill_preview', 'media.audio.fill', 'media.origins'] },
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
  ...extras({ now, id, emit, notes }),
  };
  },
} satisfies MockFeature;

/**
 * v1.4 note extras (dev mock; the real rules: fc-app `accepted_answers`, `audio_fill`):
 * accepted answers per note (the meaning field, `back` here) and filling a missing sound
 * (a note "has" a sound field when its fields carry an `audio` key; TTS reads the word).
 */
function extras({ now, id, emit, notes }: Pick<MockEnv, 'now' | 'id' | 'emit' | 'notes'>): Handlers {
  const answers = new Map<string, Map<string, string[]>>();
  const requireNote = (noteId: string) => {
    const n = notes.find((x) => x.id === noteId);
    if (!n) throw err('NOT_FOUND_NOTE', 'errors.not_found.note', noteId);
    return n;
  };
  const state = (noteId: string): NoteAcceptedAnswers => ({
    note_id: noteId,
    fields: [...(answers.get(noteId) ?? new Map<string, string[]>())].filter(([, a]) => a.length).map(([field, a]) => ({ field, answers: [...a] })),
  });
  const listOf = (noteId: string, field: string | null | undefined) => {
    requireNote(noteId);
    const byField = answers.get(noteId) ?? new Map<string, string[]>();
    answers.set(noteId, byField);
    const key = field ?? 'back';
    const list = byField.get(key) ?? [];
    byField.set(key, list);
    return list;
  };
  const fold = (s: string) => s.trim().toLocaleLowerCase();
  const scopeNotes = (scope: AudioFillScope) => (scope.note_ids ?? []).map(requireNote);
  const plan = (n: NoteDto): AudioFillItem => {
    const word = (n.fields.word ?? n.fields.front ?? '').replace(/<[^>]*>/g, '').trim();
    if (!('audio' in n.fields)) return { note_id: n.id, source: 'none', reason: 'no_audio_field', text: null, language: null };
    if (!word) return { note_id: n.id, source: 'none', reason: 'no_text', text: null, language: null };
    return { note_id: n.id, source: 'tts', reason: null, text: word, language: 'en' };
  };
  const preview = (scope: AudioFillScope): AudioFillPreview => {
    const ns = scopeNotes(scope);
    const todo = ns.filter((n) => !(n.fields.audio ?? '').trim());
    const items = todo.map(plan);
    const count = (s: AudioFillItem['source']) => items.filter((i) => i.source === s).length;
    return {
      notes: ns.length,
      with_audio: ns.length - todo.length,
      original: count('original'),
      clip: count('clip'),
      tts: count('tts'),
      tts_cached: 0,
      none: count('none'),
      other_video: 0,
      no_times: 0,
      tts_problems: [],
      items,
    };
  };
  const ttsHash = 'b'.repeat(64);
  return {
    'accepted_answers.list': ({ note_id }) => {
      requireNote(note_id);
      return state(note_id);
    },
    'accepted_answers.add': ({ note_id, field, answers: add }) => {
      const list = listOf(note_id, field);
      for (const a of add) if (a.trim() && !list.some((x) => fold(x) === fold(a))) list.push(a.trim());
      return state(note_id);
    },
    'accepted_answers.remove': ({ note_id, field, answers: drop }) => {
      const list = listOf(note_id, field);
      const keep = list.filter((x) => !drop.some((d) => fold(d) === fold(x)));
      list.splice(0, list.length, ...keep);
      return state(note_id);
    },
    'media.audio.fill_preview': ({ scope }) => preview(scope),
    'media.audio.fill': ({ scope }) => {
      const result = preview(scope);
      for (const item of result.items) {
        if (item.source === 'none') continue;
        const n = requireNote(item.note_id);
        n.fields = { ...n.fields, audio: `[sound:media:${ttsHash}]` };
        n.updated_at = now();
        emit({ type: 'note_updated', data: { note_id: n.id } });
      }
      const t = now();
      const job: JobDto = { id: id('job'), kind: 'audio_fill', status: 'succeeded', progress: 1, message: null, error: null, result, created_at: t, updated_at: t };
      return job;
    },
    'media.origins': ({ hashes, note_id }) => {
      const all = [...hashes];
      if (note_id) {
        for (const v of Object.values(requireNote(note_id).fields)) for (const m of v.matchAll(/media:([0-9a-f]{64})/g)) all.push(m[1]!);
      }
      return { origins: [...new Set(all)].map((hash) => ({ hash, origin: hash === ttsHash ? ('tts' as const) : ('user' as const) })) };
    },
  };
}
