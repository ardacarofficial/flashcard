/**
 * Dev-only mock of the word index (`words.*`, v1.4) and of `subtitle.encounters.list`, with a
 * small film vocabulary so "Kelimelerim" can be demoed and tested without Rust. The real index
 * (status rules, counts, paging, history) lives in fc-app's `words` module; this only answers in
 * shape. The one place for every `words.*` handler (Library, Home, Progress and Kelimelerim read
 * these). `words.stats` counts the listed sample plus a fixed background (the rest of a
 * plausible collection: 312 known, 234 learning); `words.mark` changes the sample's rows.
 */
import { menu, module, type MockEnv, type MockFeature } from '$lib/api/mock/env';
// Namespace import: `mockEncountersOfNote` is a live binding set when the subtitle mock starts.
import * as sub from '../subtitle/mock';
import type {
  CefrBand,
  WordHistoryPoint,
  SubtitleCaptureRow,
  WordCounts,
  WordEntryDto,
  WordLevelDto,
  WordStatus,
} from '$lib/api/types';

interface Seed {
  key: string;
  status: WordStatus;
  rank: number | null;
  band: CefrBand | null;
  notes: number;
  cards: number;
  interval?: number;
  manual?: boolean;
}

const WORDS: Seed[] = [
  { key: 'give up', status: 'learning', rank: null, band: 'B1', notes: 1, cards: 2, interval: 4 },
  { key: 'deadline', status: 'new', rank: 3120, band: 'B2', notes: 1, cards: 1 },
  { key: 'figure out', status: 'known', rank: null, band: 'B1', notes: 1, cards: 2, interval: 34 },
  { key: 'reckon', status: 'learning', rank: 4210, band: 'B2', notes: 1, cards: 1, interval: 2 },
  { key: 'cook', status: 'known', rank: 1240, band: 'A1', notes: 1, cards: 1, interval: 48 },
  { key: 'chemistry', status: 'new', rank: 5480, band: 'B1', notes: 1, cards: 1 },
  { key: 'partner', status: 'known', rank: 980, band: 'A2', notes: 0, cards: 0, manual: true },
  { key: 'cartel', status: 'new', rank: 18_900, band: null, notes: 1, cards: 1 },
  { key: 'launder', status: 'learning', rank: 14_200, band: 'C1', notes: 1, cards: 1, interval: 1 },
  { key: 'lawyer', status: 'known', rank: 2050, band: 'A2', notes: 1, cards: 1, interval: 25 },
  { key: 'yo', status: 'ignored', rank: 640, band: null, notes: 0, cards: 0, manual: true },
  { key: 'whatever', status: 'learning', rank: 870, band: 'A2', notes: 1, cards: 1, interval: 6 },
];

/** Meanings of the sample (`words.lookup` → `word.meanings`). */
const MEANINGS: Record<string, string[]> = {
  'give up': ['vazgeçmek', 'bırakmak'],
  deadline: ['son teslim tarihi'],
  'figure out': ['çözmek, anlamak'],
  reckon: ['sanmak, tahmin etmek'],
  cook: ['pişirmek', 'aşçı'],
  chemistry: ['kimya'],
  cartel: ['kartel'],
  launder: ['(para) aklamak'],
  lawyer: ['avukat'],
  whatever: ['her neyse'],
};

/** Words outside the listed sample, so the counts look like a real collection. */
const BACKGROUND: WordCounts = { new: 137, learning: 230, known: 308, ignored: 3 };

const SENTENCES: Record<string, [string, string, number]> = {
  'give up': ['I will never give up on this family.', 'Breaking Bad · S1E3', 1_302_000],
  deadline: ['The deadline is Friday, no exceptions.', 'The Office · S2E4', 412_000],
  reckon: ['I reckon we should head back.', 'Peaky Blinders · S1E1', 2_015_000],
  chemistry: ['Chemistry is the study of change.', 'Breaking Bad · S1E1', 640_000],
  cartel: ['The cartel does not forgive.', 'Breaking Bad · S2E7', 1_822_000],
  launder: ['We need a way to launder the money.', 'Breaking Bad · S1E3', 2_410_000],
};

function level(w: Seed): WordLevelDto | null {
  if (w.rank === null && w.band === null) return null;
  const expression = w.key.includes(' ');
  return {
    lemma: expression ? null : w.key,
    rank: w.rank,
    band: w.band,
    band_origin: expression ? 'expression' : w.band ? 'profile' : null,
    estimated: expression,
  };
}

export default {
  modules: [
    module('words', [{ ...menu('words.screen', 'nav.words', '/words', 46), icon: 'book-open' }]),
    module('word_data', []),
  ],
  handlers: ({ now, id, modules }: MockEnv) => {
    const at = now();
    // Own copy: `words.mark` changes it.
    const words: Seed[] = WORDS.map((w) => ({ ...w }));
    const noteIds = new Map(WORDS.map((w) => [w.key, w.notes ? id('note') : null]));
    const withData = () => modules.enabled('word_data');
    const entry = (w: Seed, full = false): WordEntryDto => {
      const note = noteIds.get(w.key) ?? null;
      return {
        language: 'en',
        key: w.key,
        status: w.status,
        mark: w.manual ? (w.status === 'ignored' ? 'ignored' : 'known') : null,
        mark_at: w.manual ? at : null,
        notes: w.notes,
        cards: w.cards,
        max_interval_days: w.interval ?? null,
        known_by_maturity: (w.interval ?? 0) >= 21,
        last_review: w.interval ? at : null,
        first_seen: w.notes ? at : null,
        note_ids: full && note ? [note] : [],
        card_ids: [],
        meanings: full ? (MEANINGS[w.key] ?? []) : [],
        level: withData() ? level(w) : null,
      };
    };
    const counts = (list: Seed[], base: WordCounts = { new: 0, learning: 0, known: 0, ignored: 0 }): WordCounts => ({
      new: base.new + list.filter((w) => w.status === 'new').length,
      learning: base.learning + list.filter((w) => w.status === 'learning').length,
      known: base.known + list.filter((w) => w.status === 'known').length,
      ignored: base.ignored + list.filter((w) => w.status === 'ignored').length,
    });
    /** Status by the cards alone (what a cleared mark falls back to): the seed's own unless
     * the seed was a manual mark. */
    const byCards = (w: Seed): WordStatus => {
      const seed = WORDS.find((x) => x.key === w.key);
      if (seed && !seed.manual) return seed.status;
      return w.cards === 0 ? 'new' : (w.interval ?? 0) >= 21 ? 'known' : 'learning';
    };
    const BANDS: (CefrBand | null)[] = ['A1', 'A2', 'B1', 'B2', 'C1', 'C2', null];
    return {
      'words.stats': () => ({
        threshold_days: 21,
        total: counts(words, BACKGROUND),
        languages: [
          {
            language: 'en',
            counts: counts(words, BACKGROUND),
            bands: withData() ? BANDS.map((band) => ({ band, counts: counts(words.filter((w) => w.band === band)) })) : [],
          },
        ],
        unassigned_notes: 12,
      }),
      'words.languages': () => ({ languages: [{ code: 'en', notes: 655, words: Object.values(counts(words, BACKGROUND)).reduce((a, b) => a + b, 0) }] }),
      'words.history': ({ days }) => {
        // A plausible climb to today's known count: slow at first, a few words a day lately.
        const n = days ?? 90;
        const known = counts(words, BACKGROUND).known;
        const today = new Date(at);
        const points: WordHistoryPoint[] = Array.from({ length: n }, (_, i) => {
          const d = new Date(today.getTime() - (n - 1 - i) * 86_400_000);
          const share = n === 1 ? 1 : i / (n - 1);
          return { day: d.toISOString().slice(0, 10), known: Math.round(known * (0.45 + 0.55 * share * share)) };
        });
        return { threshold_days: 21, points };
      },
      'words.mark': ({ lemmas, mark }) => {
        let changed = 0;
        for (const lemma of lemmas) {
          const w = words.find((x) => x.key === lemma.trim().toLowerCase());
          if (!w || (w.manual ? (w.status === 'ignored' ? 'ignored' : 'known') : null) === mark) continue;
          w.manual = mark !== null;
          w.status = mark ?? byCards(w);
          changed++;
        }
        return { changed };
      },
      'words.list': ({ status, cursor, limit, sort }) => {
        let list = words.filter((w) => !status || w.status === status);
        const rankOf = (w: Seed) => w.rank ?? Number.MAX_SAFE_INTEGER;
        const bandOf = (w: Seed) => (w.band ? BANDS.indexOf(w.band) : 99);
        if (sort === 'rank') list = [...list].sort((a, b) => rankOf(a) - rankOf(b));
        else if (sort === 'band') list = [...list].sort((a, b) => bandOf(a) - bandOf(b) || rankOf(a) - rankOf(b));
        else list = [...list].sort((a, b) => a.key.localeCompare(b.key));
        const start = cursor ? Number(cursor) : 0;
        const size = limit ?? 200;
        const page = list.slice(start, start + size);
        return { words: page.map((w) => entry(w)), next_cursor: start + size < list.length ? String(start + size) : null };
      },
      'words.lookup': ({ forms }) => ({
        language: 'en',
        lemmatizer: 'en-simple',
        threshold_days: 21,
        forms: forms.map((form) => {
          const w = words.find((x) => x.key === form.toLowerCase());
          return {
            form,
            key: w?.key ?? form.toLowerCase(),
            status: w?.status ?? 'new',
            word: w ? entry(w, true) : null,
            level: w && withData() ? level(w) : null,
            family: [],
          };
        }),
      }),
      'subtitle.encounters.list': ({ lemma, note_id }) => {
        // A note's encounters (note editor) belong to the subtitle mock; this one answers by word.
        if (note_id) return { encounters: sub.mockEncountersOfNote(note_id) };
        const s = lemma ? SENTENCES[lemma] : undefined;
        const note = lemma ? noteIds.get(lemma) : null;
        if (!s || !note || !lemma) return { encounters: [] };
        const [sentence, title, t] = s;
        const row: SubtitleCaptureRow = {
          capture_id: id('cap'),
          dedup_key: `${lemma}:${title}`,
          lang: 'en',
          lemma,
          note_id: note,
          card_id: id('card'),
          key_id: null,
          created_at: at,
          facts: { sentence, title, t_start_ms: t, t_end_ms: t + 2500, example_fields: {} },
          is_primary: true,
          review_note_id: null,
        };
        return { encounters: [row] };
      },
    };
  },
} satisfies MockFeature;
