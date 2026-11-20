/**
 * Dev-only stand-in for the reading module (the real lemmatizers, FSRS-based word status,
 * queue intervals and jobs live in fc-core / fc-app): words are lower-cased letter runs, a
 * word is "learning" once a card was made for it, jobs finish at once.
 */
import { err, menu, module, type MockEnv, type MockFeature } from '$lib/api/mock/env';
import type {
  JobDto,
  JsonValue,
  NoteDto,
  ReadingItemDto,
  TextAnalysis,
  WordMark,
  WordStatus,
} from '$lib/api/types';

interface Item {
  dto: ReadingItemDto;
  body: string;
}

export default {
  modules: [module('reading', [menu('reading.screen', 'nav.reading', '/reading', 45)])],
  handlers: ({ now, id, emit }: MockEnv) => {
    const items: Item[] = [];
    const marks = new Map<string, WordMark>();
    const carded = new Set<string>();
    const changed = () => emit({ type: 'reading_items_changed' });
    const find = (iid: string): Item => {
      const it = items.find((x) => x.dto.id === iid);
      if (!it) throw err('NOT_FOUND_READING_ITEM', 'errors.not_found.reading_item', iid);
      return it;
    };
    const status = (lemma: string): WordStatus =>
      carded.has(lemma) ? 'learning' : marks.get(lemma) === 'known' ? 'known' : marks.get(lemma) === 'ignored' ? 'ignored' : 'new';
    const add = (title: string, language: string, body: string, kind: 'text' | 'extract', queued: boolean, parent: string | null) => {
      const t = now();
      const dto: ReadingItemDto = {
        id: id('reading'),
        kind,
        parent_id: parent,
        title: title.trim() || body.trim().slice(0, 60),
        language: language.trim() || 'en',
        source: null,
        media_path: null,
        length: body.length,
        position: 0,
        progress_pct: 0,
        cue_count: 0,
        queued,
        priority: kind === 'extract' ? 'high' : 'normal',
        due: queued ? t : null,
        is_due: queued,
        interval_days: 0,
        read_count: 0,
        last_read: null,
        created_at: t,
        updated_at: t,
      };
      items.unshift({ dto, body });
      changed();
      return dto;
    };
    const job = (kind: string, result: unknown): JobDto => {
      const t = now();
      return { id: id('job'), kind, status: 'succeeded', progress: 1, message: null, error: null, result: result as JsonValue, created_at: t, updated_at: t };
    };
    const analyze = (text: string, language: string): TextAnalysis => {
      const tokens = [...text.matchAll(/\p{L}+(?:['’]\p{L}+)*/gu)].map((m) => {
        const lemma = m[0].toLowerCase();
        return { start: m.index, end: m.index + m[0].length, lemma, status: status(lemma) };
      });
      const count = (pick: (s: WordStatus) => boolean, unique: boolean) =>
        (unique ? [...new Map(tokens.map((x) => [x.lemma, x])).values()] : tokens).filter((x) => pick(x.status)).length;
      const counts = (unique: boolean) => ({
        new: count((s) => s === 'new', unique),
        learning: count((s) => s === 'learning', unique),
        known: count((s) => s === 'known', unique),
        ignored: count((s) => s === 'ignored', unique),
      });
      const running = counts(false);
      const counted = tokens.length - running.ignored;
      const sentences: [number, number][] = [...text.matchAll(/[^.!?\n]+[.!?]*/g)]
        .filter((m) => m[0].trim())
        .map((m) => [m.index, m.index + m[0].length]);
      return {
        text,
        language,
        lemmatizer: 'builtin.plain',
        tokens,
        sentences,
        cues: [],
        summary: { running, unique: counts(true), known_pct: counted ? Math.round((running.known * 1000) / counted) / 10 : 0 },
      };
    };

    return {
      'reading.items.list': ({ queued_only }) => {
        const list = items.map((i) => i.dto).filter((d) => !queued_only || d.queued);
        return { items: list, due_count: list.filter((d) => d.is_due).length };
      },
      'reading.items.get': ({ id: iid }) => find(iid).dto,
      'reading.items.create': (req) => {
        if (!req.body.trim()) throw err('VALIDATION_READING_TEXT', 'errors.validation.reading_text', 'empty');
        return add(req.title, req.language, req.body, 'text', req.queue, null);
      },
      'reading.items.import': (req) =>
        job('reading.import', add(req.title || 'Imported', req.language, 'Imported text from a file.', 'text', req.queue, null)),
      'reading.items.update': (req) => {
        const it = find(req.id);
        Object.assign(it.dto, { title: req.title, language: req.language, media_path: req.media_path, updated_at: now() });
        changed();
        return it.dto;
      },
      'reading.items.set_position': ({ id: iid, position }) => {
        const it = find(iid);
        it.dto.position = Math.min(position, it.dto.length);
        changed();
        return it.dto;
      },
      'reading.items.delete': ({ id: iid }) => {
        const it = find(iid);
        items.splice(items.indexOf(it), 1);
        changed();
        return it.dto;
      },
      'reading.queue.set': ({ id: iid, queued }) => {
        const it = find(iid);
        Object.assign(it.dto, { queued, is_due: queued, due: queued ? now() : null });
        changed();
        return it.dto;
      },
      'reading.queue.read': ({ id: iid }) => {
        const it = find(iid);
        const days = it.dto.interval_days ? it.dto.interval_days * 2 : 1;
        Object.assign(it.dto, { queued: true, is_due: false, interval_days: days, read_count: it.dto.read_count + 1 });
        changed();
        return it.dto;
      },
      'reading.queue.postpone': ({ id: iid }) => {
        const it = find(iid);
        it.dto.is_due = false;
        changed();
        return it.dto;
      },
      'reading.extracts.create': ({ item_id, start, end, title }) => {
        const parent = find(item_id);
        return add(title, parent.dto.language, parent.body.slice(start, end), 'extract', true, item_id);
      },
      'reading.extracts.cloze': ({ item_id, deck_id }) => {
        find(item_id);
        return { id: id('note'), home_deck_id: deck_id, cards: [] } as unknown as NoteDto;
      },
      'reading.analyze': ({ item_id, text, language }) => {
        if (item_id) {
          const it = find(item_id);
          return analyze(it.body, it.dto.language);
        }
        return analyze(text ?? '', language ?? 'en');
      },
      'reading.words.mark': ({ language, lemmas, mark }) => {
        let n = 0;
        for (const l of lemmas) {
          if ((marks.get(l) ?? null) === mark) continue;
          if (mark) marks.set(l, mark);
          else marks.delete(l);
          n++;
        }
        emit({ type: 'word_marks_changed', data: { language } });
        return { changed: n };
      },
      'reading.words.lookup': ({ lemma }) => ({ lemma, status: status(lemma), mark: marks.get(lemma) ?? null, cards: [] }),
      'reading.cards.make': (req) => {
        if (req.kind === 'word' && !req.meaning.trim()) {
          throw err('VALIDATION_FIELD_REQUIRED', 'errors.validation.field_required', 'meaning');
        }
        carded.add(req.word.toLowerCase());
        return { id: id('note'), home_deck_id: req.deck_id, cards: [] } as unknown as NoteDto;
      },
      'reading.clips.extract': () => {
        throw err('PROVIDER_FFMPEG', 'errors.provider.ffmpeg', 'no ffmpeg in the browser mock');
      },
    };
  },
} satisfies MockFeature;
