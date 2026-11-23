/**
 * Dev-only mock of the `subtitle` module (see `api/mock/transport.ts`). Validation is minimal:
 * the real rules live in fc-app `modules/subtitle/` (keys hashed, captures as notes). Unlike
 * the real module (off by default) it starts on, with one used key, one revoked key and a few
 * captures, so the screenshot run shows the settings page with every kind of row.
 */
import { menu, module, err, type MockEnv, type MockFeature } from '$lib/api/mock/env';
import type { NoteDto, RankedExample, SubtitleCaptureRow, SubtitleKeyDto, SubtitleStatus } from '$lib/api/types';

const PORT = 28735;

/**
 * `subtitle.encounters.list` by `note_id` of the newest mock backend. Another feature's mock
 * that also answers `subtitle.encounters.list` (words/mock.ts, by lemma) hands `note_id`
 * requests to this.
 */
export let mockEncountersOfNote: (noteId: string) => SubtitleCaptureRow[] = () => [];

export default {
  modules: [module('subtitle', [menu('subtitle.screen', 'nav.subtitle', '/subtitle', 83)]), module('examples', [])],
  handlers: ({ now, id, emit, decks, notes }: MockEnv) => {
    const t0 = now();
    const keys: SubtitleKeyDto[] = [
      { id: id('subkey'), name: 'Firefox (laptop)', prefix: 'sk_fc_7q2m', created_at: t0 - 20 * 86_400_000, last_used_at: null, revoked_at: t0 - 6 * 86_400_000 },
      { id: id('subkey'), name: 'Chrome · ev', prefix: 'sk_fc_k3vd', created_at: t0 - 5 * 86_400_000, last_used_at: t0 - 3_600_000, revoked_at: null },
    ];
    let defaultDeck: string | null = null;
    const captures = 12;
    const lastCapture = t0 - 3_600_000;
    const status = (): SubtitleStatus => {
      const deck = decks.find((d) => d.id === defaultDeck) ?? null;
      return {
        address: `http://127.0.0.1:${PORT}/subtitle`,
        port: PORT,
        default_deck_id: deck?.id ?? null,
        default_deck_path: deck?.path ?? null,
        fallback_deck: 'Subtitle',
        captures,
        last_capture_at: lastCapture,
        active_keys: keys.filter((k) => k.revoked_at === null).length,
        share_known_words: false,
      };
    };
    // v1.4 encounters (where a word was seen). A note whose word is go/went gets two demo
    // encounters the first time it is asked for; every other note has none.
    const encounters: SubtitleCaptureRow[] = [];
    const seeded = new Set<string>();
    const wordOf = (n: NoteDto) => (n.fields.word ?? n.fields.surface ?? n.fields.front ?? '').replace(/<[^>]*>/g, '').trim().toLowerCase();
    const requireNote = (noteId: string) => {
      const n = notes.find((x) => x.id === noteId);
      if (!n) throw err('NOT_FOUND_NOTE', 'errors.not_found.note', noteId);
      return n;
    };
    const encounter = (n: NoteDto, sentence: string, title: string, start: number, primary: boolean): SubtitleCaptureRow => {
      const at = sentence.indexOf('went');
      return {
        capture_id: id('capture'),
        dedup_key: `en|go|${sentence.length}`,
        lang: 'en',
        lemma: 'go',
        note_id: n.id,
        card_id: n.cards[0]?.id ?? id('card'),
        key_id: null,
        created_at: now() - (primary ? 3 : 1) * 86_400_000,
        facts: {
          surface: 'went',
          pos: 'verb',
          sentence,
          translation: null,
          term_offset: at >= 0 ? [at, at + 4] : null,
          video_id: title,
          title,
          site: 'netflix',
          t_start_ms: start,
          t_end_ms: start + 2400,
          audio_hash: primary ? 'a'.repeat(64) : null,
          audio_origin: primary ? 'original' : null,
          example_fields: {},
        },
        is_primary: primary,
        review_note_id: null,
      };
    };
    const ofNote = (noteId: string): SubtitleCaptureRow[] => {
      const n = requireNote(noteId);
      if (!seeded.has(noteId)) {
        seeded.add(noteId);
        if (['go', 'went'].includes(wordOf(n))) {
          encounters.push(
            encounter(n, 'I went home after the party.', 'Breaking Bad S1E3', 12 * 60_000 + 41_000, true),
            encounter(n, 'We went to the beach.', 'Breaking Bad S1E1', 31 * 60_000 + 5_000, false),
          );
        }
      }
      return encounters.filter((e) => e.note_id === noteId).sort((a, b) => Number(b.is_primary) - Number(a.is_primary) || a.created_at - b.created_at);
    };
    const requireEncounter = (captureId: string) => {
      const e = encounters.find((x) => x.capture_id === captureId);
      if (!e) throw err('NOT_FOUND_SUBTITLE_ENCOUNTER', 'errors.not_found.subtitle_encounter', captureId);
      return e;
    };
    const setPrimary = (e: SubtitleCaptureRow) => {
      for (const x of encounters) if (x.note_id === e.note_id) x.is_primary = x === e;
    };
    const ranking = (noteId: string) => {
      const list = ofNote(noteId);
      // Shorter lines rank higher (the real ranking: fc-app `examples`).
      const items: RankedExample[] = [...list]
        .sort((a, b) => (a.facts.sentence ?? '').length - (b.facts.sentence ?? '').length)
        .map((e, i) => ({
          id: e.capture_id,
          rank: i + 1,
          score: 10 - i * 3,
          is_primary: e.is_primary,
          sentence: e.facts.sentence ?? '',
          target_found: true,
          unknown_words: [],
          reasons: [],
          duplicates: [],
        }));
      return { note_id: noteId, language: 'en', target: 'go', frequency_pack: false, items };
    };
    mockEncountersOfNote = ofNote;
    return {
      'subtitle.encounters.list': ({ note_id }) => ({ encounters: note_id ? ofNote(note_id) : [] }),
      'subtitle.encounters.set_primary': ({ capture_id }) => {
        const e = requireEncounter(capture_id);
        setPrimary(e);
        emit({ type: 'note_updated', data: { note_id: e.note_id } });
        return { encounters: ofNote(e.note_id) };
      },
      'subtitle.encounters.move': ({ capture_id, to_note_id }) => {
        const e = requireEncounter(capture_id);
        const from = e.note_id;
        const to = requireNote(to_note_id ?? from);
        ofNote(to.id);
        e.note_id = to.id;
        e.card_id = to.cards[0]?.id ?? e.card_id;
        e.is_primary = !encounters.some((x) => x !== e && x.note_id === to.id && x.is_primary);
        const left = encounters.filter((x) => x.note_id === from);
        if (left.length && !left.some((x) => x.is_primary)) left[0]!.is_primary = true;
        return { encounters: ofNote(to.id) };
      },
      'subtitle.encounters.merge_notes': ({ a, b, force }) => {
        const na = requireNote(a);
        const nb = requireNote(b);
        if (!force && wordOf(na) !== wordOf(nb)) {
          throw err('CONFLICT_SUBTITLE_MERGE_WORDS', 'errors.conflict.subtitle_merge_words', `${a} ${b}`);
        }
        ofNote(a);
        ofNote(b);
        const moving = encounters.filter((x) => x.note_id === b);
        for (const e of moving) {
          e.note_id = a;
          e.is_primary = false;
        }
        const i = notes.indexOf(nb);
        if (i >= 0) notes.splice(i, 1);
        emit({ type: 'notes_deleted', data: { ids: [b] } });
        return { kept_note_id: a, archived_note_id: b, moved: moving.length, archived_reviews: 0, encounters: ofNote(a) };
      },
      'subtitle.encounters.rank': ({ note_id }) => ranking(note_id),
      'subtitle.encounters.suggest_primary': ({ note_id }) => {
        const r = ranking(note_id);
        const current = r.items.find((x) => x.is_primary)?.id ?? null;
        const best = r.items[0]?.id ?? null;
        const suggested = best && best !== current ? best : null;
        return { note_id, current, suggested, gain: suggested ? 3 : 0, ranking: r };
      },
      'subtitle.keys.list': () => ({ keys: [...keys].reverse() }),
      'subtitle.keys.create': ({ name }) => {
        const n = name.trim();
        if (!n || n.length > 60) throw err('VALIDATION_SUBTITLE_KEY', 'errors.validation.subtitle_key', name);
        const secret = `sk_fc_${id('mock').replace(/[^a-z0-9]/g, '')}${'x'.repeat(30)}`;
        const key: SubtitleKeyDto = {
          id: id('subkey'),
          name: n,
          prefix: secret.slice(0, 10),
          created_at: now(),
          last_used_at: null,
          revoked_at: null,
        };
        keys.push(key);
        emit({ type: 'subtitle_changed' });
        return { key, secret };
      },
      'subtitle.keys.revoke': ({ id: kid }) => {
        const k = keys.find((x) => x.id === kid);
        if (!k) throw err('NOT_FOUND_SUBTITLE_KEY', 'errors.not_found.subtitle_key', kid);
        if (k.revoked_at !== null) throw err('CONFLICT_SUBTITLE_KEY_STATE', 'errors.conflict.subtitle_key_state', kid);
        k.revoked_at = now();
        emit({ type: 'subtitle_changed' });
        return k;
      },
      'subtitle.status': () => status(),
      'subtitle.default_deck.set': ({ deck_id }) => {
        if (deck_id && !decks.some((d) => d.id === deck_id)) throw err('NOT_FOUND_DECK', 'errors.not_found.deck', deck_id);
        defaultDeck = deck_id ?? null;
        emit({ type: 'subtitle_changed' });
        return status();
      },
    };
  },
} satisfies MockFeature;
