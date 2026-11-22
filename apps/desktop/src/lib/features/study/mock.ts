/**
 * Dev/test mock of the study module: a tiny queue over the mock notes (every card new,
 * "Again" sends a card to the back, fixed intervals). Not the real engine — that is fc-app's
 * `study` module; this only lets the screen run in a browser and in component tests.
 */
import { err, menu, mockScenario, module, type MockEnv, type MockFeature } from '$lib/api/mock/env';
import type { ApiError } from '$lib/api/types';
import type { StartStudy, StudyEmptyReason, StudyOptions, StudyOptionsRequest, StudyScope, StudyScopeOption } from './contract';
import { PRACTICE_SCOPES, SCOPES } from './contract';
import type { DeckCounts, DeckTreeNode, SettingDef } from '$lib/api/types';
import type {
  MatchRoundDto,
  NoteDto,
  SessionDto,
  SessionModesDto,
  SessionSummary,
  StudyCardDto,
  TestAnswerDto,
  TestPaperDto,
  TestQuestionDto,
  TestVerdict,
} from '$lib/api/types';

const DAY = 86_400_000;
const esc = (s: string) => s.replace(/[&<>"]/g, (c) => `&#${c.charCodeAt(0)};`);
const INTERVALS = [60_000, 6 * 60_000, 10 * 60_000, 4 * DAY];

/**
 * How long an "Again" card waits before it is shown again (a learning step). In the browser
 * mock a minute, so the waiting state can be seen; in tests 0 unless a test sets it.
 */
let learningStepMs = import.meta.env.MODE === 'mock' ? 60_000 : 0;
export function setMockLearningStep(ms: number): void {
  learningStepMs = ms;
}

interface MockCard {
  card_id: string;
  note: NoteDto;
  /** Queue bucket of a Cards session (default `new`). */
  state?: 'new' | 'learning' | 'review';
  /** Learning step: not shown before this wall-clock time (ms). */
  waitUntil?: number;
}

/** Learn: stage per card (`learned` once right at both stages). */
interface MockLearn {
  stage: Record<string, 'choice' | 'written' | 'learned'>;
  mistakes: Record<string, number>;
  total: number;
}

/** Match: the pool and the round on the board (`pair` = index into `cards`). */
interface MockMatch {
  pool: MockCard[];
  rounds: number;
  cards: MockCard[];
  tiles: { pair: number; side: 'term' | 'definition' }[];
  matched: boolean[];
  mistakes: number;
  started_at: number;
  completed_at: number | null;
  finished_ms: number | null;
}

interface MockSession {
  dto: SessionDto;
  queue: MockCard[];
  again: string[];
  time: number;
  hints: number;
  learn?: MockLearn;
  test?: { questions: TestQuestionDto[]; result: TestPaperDto['result']; answers: TestPaperDto['answers']; draft?: TestAnswerDto[] | null };
  match?: MockMatch;
}

const front = (c: MockCard) => c.note.fields.front ?? '';
const back = (c: MockCard) => c.note.fields.back ?? '';
const shuffled = <T>(xs: T[]): T[] => xs.map((x) => [Math.random(), x] as const).sort((a, b) => a[0] - b[0]).map((p) => p[1]);
const plainModes = (over: Partial<SessionModesDto>): SessionModesDto => ({
  rating_scale: 'buttons',
  confidence: [],
  timer: null,
  timed_out: 0,
  learn: null,
  test: null,
  ...over,
});

/** The study modes' settings as fc-app declares them (Settings → "Reading and modes"). */
const def = (key: string, kind: SettingDef['kind'], value: unknown, advanced = false): { module: string; def: SettingDef } => ({
  module: key.split('.')[0]!,
  def: { key, kind, default: value, label_key: `settings.${key}.label`, description_key: `settings.${key}.description`, advanced, scopes: ['app'] },
});
const int = (min: number, max: number): SettingDef['kind'] => ({ type: 'integer', min, max });
const choice = (...options: string[]): SettingDef['kind'] => ({ type: 'choice', options });
const text = (max_len: number): SettingDef['kind'] => ({ type: 'text', max_len });
const MODE_SETTINGS = [
  def('study.answer_buttons', choice('two', 'four'), 'two'),
  def('study.learn_ahead_mins', int(0, 1440), 20),
  def('confidence.rating_scale', choice('buttons', 'confidence'), 'buttons'),
  def('timed.seconds', int(3, 600), 20),
  def('timed.on_timeout', choice('again', 'skip'), 'again'),
  def('focus.fullscreen', { type: 'bool' }, true),
  def('focus.hide_counts', { type: 'bool' }, true),
  def('focus.auto', { type: 'bool' }, false),
  def('learn.max_cards', int(1, 500), 20),
  def('practice_test.questions', int(1, 200), 20),
  def('reading.default_language', text(35), 'en'),
  def('reading.vocabulary_query', text(2000), ''),
  def('reading.ffmpeg_path', text(1000), 'ffmpeg', true),
];

export default {
  modules: [module('study', [menu('study.session', 'nav.study', '/study', 5), menu('study.mode_settings', 'nav.mode_settings', '/mode-settings', 90)])],
  handlers: ({ id, now, notes, decks, ancestors, cardFlags, tree, modules }: MockEnv) => {
    const modulesOn = (m: string) => modules.enabled(m);
    const sessions = new Map<string, MockSession>();
    const undo: Array<{ id: string; snapshot: string }> = [];

    const require = (sessionId: string): MockSession => {
      const s = sessions.get(sessionId);
      if (!s) throw err('NOT_FOUND_SESSION', 'errors.not_found.session', sessionId);
      return s;
    };
    /** Brings the first card whose learning step is over to the front; false when all wait. */
    const promote = (s: MockSession): boolean => {
      const t = Date.now();
      const i = s.queue.findIndex((c) => (c.waitUntil ?? 0) <= t);
      if (i > 0) s.queue.unshift(...s.queue.splice(i, 1));
      return i >= 0;
    };
    const current = (s: MockSession): StudyCardDto | null => {
      if (s.dto.ended_at !== null || !promote(s)) return null;
      const c = s.queue[0]!;
      return {
        card_id: c.card_id,
        note_id: c.note.id,
        deck_id: c.note.home_deck_id,
        state: c.state ?? 'new',
        sides: {
          template_key: 'forward',
          template_name: '1',
          card_type: 'classic',
          question: esc(c.note.fields.front ?? ''),
          answer: esc(c.note.fields.back ?? ''),
          css: '',
          interaction: { kind: 'self_grade' },
          tts: [],
        },
        flags: cardFlags.get(c.card_id) ?? 0,
        type_answer: false,
        intervals: INTERVALS.map((ms, i) => ({
          rating: (['again', 'hard', 'good', 'easy'] as const)[i]!,
          interval_ms: ms,
          scheduled_days: ms >= DAY ? ms / DAY : 0,
        })),
        hints_used: s.hints,
      };
    };
    /** Learn options of a card: its answer and up to three other answers (stable per card). */
    const learnOptions = (s: MockSession, c: MockCard): string[] => {
      const others = [...new Set(s.queue.concat(Object.values(pool(s))).map(back))].filter((b) => b && b !== back(c));
      const opts = others.slice(0, 3).concat(back(c));
      return opts.sort((a, b) => ((a.length * 7 + c.card_id.length) % 5) - ((b.length * 7) % 5) || a.localeCompare(b));
    };
    const pool = (s: MockSession): Record<string, MockCard> => Object.fromEntries(s.queue.map((c) => [c.card_id, c]));
    const modes = (s: MockSession): SessionModesDto | null => {
      if (s.learn) {
        const c = s.queue[0];
        const st = c ? s.learn.stage[c.card_id] : undefined;
        const learned = Object.values(s.learn.stage).filter((x) => x === 'learned').length;
        const written = Object.values(s.learn.stage).filter((x) => x === 'written').length;
        return plainModes({
          learn: {
            stage: c && st !== 'learned' ? (st ?? 'choice') : null,
            options: c && st === 'choice' ? learnOptions(s, c) : [],
            mistakes: c ? (s.learn.mistakes[c.card_id] ?? 0) : 0,
            progress: { total: s.learn.total, learned, choice: s.learn.total - learned - written, written },
          },
        });
      }
      if (s.test) {
        return plainModes({
          test: { questions: s.test.questions.length, submitted: s.test.result !== null, percent: s.test.result?.percent ?? null },
        });
      }
      return null;
    };
    const view = (s: MockSession): SessionDto => ({
      ...s.dto,
      counts: {
        new: s.queue.filter((c) => (c.state ?? 'new') === 'new').length,
        learning: s.queue.filter((c) => c.state === 'learning').length,
        review: s.queue.filter((c) => c.state === 'review').length,
      },
      mistakes: s.again.length,
      current: s.test || s.match ? null : current(s),
      next_due: s.test || s.match || s.dto.ended_at !== null || s.queue.length === 0 || s.queue.some((c) => (c.waitUntil ?? 0) <= Date.now())
        ? null
        : Math.min(...s.queue.map((c) => c.waitUntil ?? 0)),
      modes: modes(s),
    });
    const best = new Map<string, number>();
    const round = (s: MockSession): MatchRoundDto => {
      const m = s.match!;
      return {
        session_id: s.dto.id,
        round: m.rounds,
        tiles: m.tiles.map((t, i) => ({
          id: i,
          side: t.side,
          text: t.side === 'term' ? front(m.cards[t.pair]!) : back(m.cards[t.pair]!),
          matched: m.matched[t.pair] ?? false,
        })),
        pairs: m.cards.length,
        matched: m.matched.filter(Boolean).length,
        mistakes: m.mistakes,
        started_at: m.started_at,
        finished_ms: m.finished_ms,
        best_ms: best.get(s.dto.deck_id ?? '') ?? null,
        cards: m.pool.length,
      };
    };
    const paper = (s: MockSession, key: boolean): TestPaperDto => {
      const t = s.test!;
      return {
        session_id: s.dto.id,
        questions: t.questions.map((q) => ({ ...q, key: key || t.result ? q.key : null })),
        answers: t.answers,
        result: t.result,
        draft: t.result ? null : (t.draft ?? null),
      };
    };
    /** A test paper: true/false, multiple choice, written, in turn. */
    const makePaper = (cards: MockCard[], count: number, kinds: TestQuestionDto['kind'][]): TestQuestionDto[] =>
      cards.slice(0, count).map((c, i) => {
        const kind = kinds[i % kinds.length]!;
        const others = cards.filter((x) => back(x) !== back(c)).map(back);
        const options = kind === 'multiple_choice' ? shuffled([back(c), ...others.slice(0, 3)]) : [];
        const lie = i % 2 === 1 && others.length > 0;
        return {
          index: i,
          card_id: c.card_id,
          kind,
          prompt_html: `<p>${esc(front(c))}</p>`,
          options,
          statement: kind === 'true_false' ? (lie ? others[0]! : back(c)) : null,
          key: {
            option: kind === 'multiple_choice' ? options.indexOf(back(c)) : null,
            truth: kind === 'true_false' ? !lie : null,
            expected: back(c),
          },
        };
      });
    const summary = (s: MockSession): SessionSummary => ({
      session_id: s.dto.id,
      kind: s.dto.kind,
      mode: s.dto.mode,
      answered: s.dto.answered,
      correct: s.dto.correct,
      mistakes: s.again.length,
      time_ms: s.time,
      started_at: s.dto.started_at,
      ended_at: s.dto.ended_at,
    });
    const requireCurrent = (s: MockSession, cardId: string) => {
      if (s.queue[0]?.card_id !== cardId || s.dto.ended_at !== null) {
        throw err('CONFLICT_NOT_CURRENT_CARD', 'errors.conflict.not_current_card', cardId);
      }
    };
    /** Today's counts of a deck (or all decks) as the deck tree shows them (demo seed). */
    const todayOf = (deckId: string | null): Pick<DeckCounts, 'due' | 'learning' | 'new'> => {
      const find = (ns: DeckTreeNode[]): DeckTreeNode | null => {
        for (const n of ns) {
          if (n.deck.id === deckId) return n;
          const hit = find(n.children);
          if (hit) return hit;
        }
        return null;
      };
      const top = tree(null);
      const nodes = deckId ? [find(top)].filter((n): n is DeckTreeNode => n !== null) : top;
      return nodes.reduce((a, n) => ({ due: a.due + n.counts.due, learning: a.learning + n.counts.learning, new: a.new + n.counts.new }), { due: 0, learning: 0, new: 0 });
    };
    /** Every card of a deck (or all decks) as the deck tree counts them (demo seed). */
    const totalOf = (deckId: string | null): number => {
      const find = (ns: DeckTreeNode[]): DeckTreeNode | null => {
        for (const n of ns) {
          if (n.deck.id === deckId) return n;
          const hit = find(n.children);
          if (hit) return hit;
        }
        return null;
      };
      const top = tree(null);
      const nodes = deckId ? [find(top)].filter((n): n is DeckTreeNode => n !== null) : top;
      return nodes.reduce((a, n) => a + n.counts.total, 0);
    };
    /**
     * A Cards session over the demo collection: the few mock notes repeat until the queue
     * matches the deck tree's numbers (due, then learning, then new), so the start panel,
     * the session bar and the summary agree. Without demo counts (tests): every card once, new.
     */
    const cardsQueue = (base: MockCard[], deckId: string | null): MockCard[] => {
      const c = todayOf(deckId);
      if (base.length === 0 || c.due + c.learning + c.new === 0) return base;
      const states = [
        ...Array<'review'>(c.due).fill('review'),
        ...Array<'learning'>(c.learning).fill('learning'),
        ...Array<'new'>(c.new).fill('new'),
      ];
      return states.map((state, i) => ({ ...base[i % base.length]!, state }));
    };
    const remember = (s: MockSession) => undo.push({ id: s.dto.id, snapshot: JSON.stringify(s) });

    /** Cards answered during the mock's "study day" (any mode); `struggled` = Again / wrong. */
    const today = new Map<string, { struggled: boolean; scheduled: boolean }>();
    const answeredToday = (cardId: string, struggled: boolean, scheduled: boolean) => {
      const was = today.get(cardId);
      today.set(cardId, { struggled: struggled || (was?.struggled ?? false), scheduled: scheduled || (was?.scheduled ?? false) });
    };
    /** Every card of a deck (with sub-decks), each once. */
    const deckCards = (deckId: string | null): MockCard[] =>
      notes
        .filter((n) => !deckId || ancestors(n.home_deck_id).includes(deckId))
        .flatMap((n) => n.cards.map((c) => ({ card_id: c.id, note: n })));
    /** The browser demo's decks carry fixed "today" numbers (seed.ts); tests have none. */
    const hasDemoCounts = (deckId: string | null) => {
      if (import.meta.env.MODE !== 'mock') return false;
      const find = (ns: DeckTreeNode[]): DeckTreeNode | null => {
        for (const n of ns) {
          if (n.deck.id === deckId) return n;
          const hit = find(n.children);
          if (hit) return hit;
        }
        return null;
      };
      return deckId === null ? tree(null).some((n) => n.counts.total > 0) : (find(tree(null))?.counts.total ?? 0) > 0;
    };
    const nextMidnight = () => {
      const d = new Date();
      d.setHours(24, 0, 0, 0);
      return d.getTime();
    };
    /**
     * The cards a scope holds right now, or why it is empty. Both `study.options` and
     * `study.start` use this, so the launcher's count is what a session gets (CONTRACT §1).
     */
    const resolve = (scope: StudyScope, deckId: string | null): { cards: MockCard[]; reason: StudyEmptyReason | null; next: number | null } => {
      const base = deckCards(deckId);
      const none = (reason: StudyEmptyReason, next: number | null = null) => ({ cards: [], reason, next });
      if (base.length === 0) return none('no_cards');
      switch (scope) {
        case 'due':
        case 'new': {
          let queue: MockCard[];
          if (hasDemoCounts(deckId)) {
            const c = todayOf(deckId);
            queue = c.due + c.learning + c.new === 0 ? [] : cardsQueue(base, deckId);
          } else {
            // Tests: every card is new until it is answered with the schedule on.
            queue = base.filter((c) => !today.get(c.card_id)?.scheduled);
          }
          if (scope === 'new') queue = queue.filter((c) => (c.state ?? 'new') === 'new');
          if (queue.length > 0) return { cards: queue, reason: null, next: null };
          // The demo's never-studied decks: the daily new-card limit is used up.
          const unseen = base.some((c) => !today.has(c.card_id));
          return unseen && hasDemoCounts(deckId) ? none('daily_limit_reached', nextMidnight()) : none('nothing_due');
        }
        case 'studied_today': {
          const cards = base.filter((c) => today.has(c.card_id));
          return cards.length ? { cards, reason: null, next: null } : none('none_studied_today');
        }
        case 'struggled_today': {
          const cards = base.filter((c) => today.get(c.card_id)?.struggled);
          return cards.length ? { cards, reason: null, next: null } : none('none_struggled_today');
        }
        case 'all': {
          // The demo's decks carry inventory numbers (seed.ts) larger than the few mock notes:
          // "every card" must never be fewer than "today's cards" — repeat the notes up to the
          // deck's total, as the today queue does.
          const total = hasDemoCounts(deckId) ? totalOf(deckId) : 0;
          if (total > base.length) return { cards: Array.from({ length: total }, (_, i) => base[i % base.length]!), reason: null, next: null };
          return { cards: base, reason: null, next: null };
        }
      }
    };
    const scopeOption = (scope: StudyScope, deckId: string | null): StudyScopeOption => {
      const r = resolve(scope, deckId);
      return {
        scope,
        count: r.cards.length,
        mode: PRACTICE_SCOPES.includes(scope) ? 'trial' : 'scheduled',
        empty_reason: r.cards.length ? null : r.reason,
        next_available_at: r.cards.length ? null : r.next,
      };
    };
    const nothingToStudy = (reason: StudyEmptyReason, next: number | null): ApiError => ({
      ...err('CONFLICT_NOTHING_TO_STUDY', 'errors.conflict.nothing_to_study', reason),
      details: { reason, next_available_at: next },
    });
    const deckName = (deckId: string | null) => (deckId ? (decks.find((d) => d.id === deckId)?.name ?? null) : null);
    const newSession = (req: StartStudy, queue: MockCard[]): MockSession => ({
      dto: {
        id: id('session'),
        kind: req.kind,
        mode: req.mode ?? 'scheduled',
        deck_id: req.deck_id ?? null,
        started_at: now(),
        ended_at: null,
        counts: { new: 0, learning: 0, review: 0 },
        answered: 0,
        correct: 0,
        mistakes: 0,
        current: null,
        next_due: null,
        modes: null,
      },
      queue,
      again: [],
      time: 0,
      hints: 0,
    });

    // The browser demo opens with an unfinished session to continue (and, with
    // `?mock=waiting`, one whose cards all wait for their learning step).
    if (import.meta.env.MODE === 'mock' && mockScenario() !== 'fresh') {
      const en = decks.find((d) => d.name === 'İngilizce' && d.parent_id === null) ?? null;
      const cards = deckCards(en?.id ?? null).slice(0, 5);
      if (en && cards.length) {
        const waiting = mockScenario() === 'waiting';
        const s = newSession({ kind: 'normal', deck_id: en.id }, cards.map((c) => ({ ...c, state: 'learning', waitUntil: waiting ? Date.now() + 14 * 60_000 : 0 })));
        s.dto.answered = 7;
        s.dto.correct = 6;
        s.time = 95_000;
        sessions.set(s.dto.id, s);
      }
    }

    return {
      'settings.schema': () => ({ settings: MODE_SETTINGS }),
        'study.options': ({ deck_id }: StudyOptionsRequest): StudyOptions => {
          if (!modulesOn('study')) throw err('CONFLICT_MODULE_DISABLED', 'errors.conflict.module_disabled', 'v1.study.options');
          const open = [...sessions.values()]
            .filter((s) => s.dto.ended_at === null && (!deck_id || (s.dto.deck_id !== null && ancestors(s.dto.deck_id).includes(deck_id))))
            .reverse();
          return {
            deck_id: deck_id ?? null,
            deck_name: deckName(deck_id ?? null),
            resumable: open.map((s) => ({
              session: view(s),
              deck_name: deckName(s.dto.deck_id),
              answered: s.test ? (s.test.draft?.length ?? 0) : s.dto.answered,
              remaining: s.test ? s.test.questions.length - (s.test.draft?.length ?? 0) : s.queue.length,
            })),
            scopes: SCOPES.map((sc) => scopeOption(sc, deck_id ?? null)),
            modes: (
              [
                ['normal', 'study'],
                ['learn', 'learn'],
                ['test', 'practice_test'],
                ['match', 'match'],
              ] as const
            )
              .filter(([, m]) => modulesOn('study') && modulesOn(m))
              .map(([k]) => k),
          };
        },
      'study.start': (req: StartStudy) => {
        const from = req.from_session ? require(req.from_session) : null;
        const deckId = req.deck_id ?? null;
        // v1.3: a scope (Cards default: today's queue) resolves first; an empty one is an
        // error that leaves every open session alone.
        const scope: StudyScope | null = req.scope ?? (req.kind === 'normal' && !from ? 'due' : null);
        let queue: MockCard[];
        if (scope && (req.kind === 'normal' || req.kind === 'learn' || req.kind === 'test' || req.kind === 'match')) {
          const r = resolve(scope, deckId);
          if (r.cards.length === 0) throw nothingToStudy(r.reason ?? 'no_cards', r.next);
          queue = r.cards;
        } else {
          queue = deckCards(deckId).filter((c) => !from || from.again.includes(c.card_id));
        }
        if (req.filter?.limit) queue = queue.slice(0, req.filter.limit);
        // One open session per kind (fc-app `end_open_sessions`): a test paper waits while you study cards.
        for (const s of sessions.values()) if (s.dto.kind === req.kind) s.dto.ended_at ??= now();
        const s = newSession(req, queue);
        if (scope && PRACTICE_SCOPES.includes(scope)) s.dto.mode = 'trial';
        if (req.kind === 'learn') {
          s.queue = queue.slice(0, 30);
          s.learn = { stage: Object.fromEntries(s.queue.map((c) => [c.card_id, 'choice' as const])), mistakes: {}, total: s.queue.length };
        } else if (req.kind === 'test') {
          const opts = req.modes?.test;
          s.test = {
            questions: makePaper(shuffled(queue), opts?.questions ?? 20, opts?.kinds ?? ['true_false', 'multiple_choice', 'written']),
            result: null,
            answers: null,
          };
        } else if (req.kind === 'match') {
          s.match = { pool: queue, rounds: 0, cards: [], tiles: [], matched: [], mistakes: 0, started_at: Date.now(), completed_at: null, finished_ms: null };
        }
        sessions.set(s.dto.id, s);
        return view(s);
      },
      'modes.learn.answer': ({ session_id, card_id, choice, given }) => {
        const s = require(session_id);
        requireCurrent(s, card_id);
        const l = s.learn!;
        const c = s.queue.shift()!;
        const stage = l.stage[card_id] === 'written' ? 'written' : 'choice';
        const picked = choice != null ? (learnOptions({ ...s, queue: [c, ...s.queue] }, c)[choice] ?? '') : (given ?? '');
        const ok = picked.trim().toLowerCase() === back(c).trim().toLowerCase();
        answeredToday(card_id, !ok, s.dto.mode === 'scheduled');
        if (ok) l.stage[card_id] = stage === 'choice' ? 'written' : 'learned';
        else l.mistakes[card_id] = (l.mistakes[card_id] ?? 0) + 1;
        if (l.stage[card_id] !== 'learned') s.queue.splice(Math.min(2, s.queue.length), 0, c);
        s.dto.answered++;
        if (ok) s.dto.correct++;
        return {
          grade: ok ? 'correct' : 'wrong',
          stage,
          learned: l.stage[card_id] === 'learned',
          expected: back(c),
          diff: [],
          session: view(s),
        };
      },
      'modes.test.paper': ({ session_id, with_key }) => paper(require(session_id), with_key ?? false),
      'modes.test.save_draft': ({ session_id, answers }) => {
        const s = require(session_id);
        const t = s.test!;
        if (t.result) throw err('CONFLICT_TEST_SUBMITTED', 'errors.conflict.test_submitted', session_id);
        t.draft = answers;
        return paper(s, false);
      },
      'modes.test.submit': ({ session_id, answers }) => {
        const s = require(session_id);
        const t = s.test!;
        if (t.result) throw err('CONFLICT_TEST_SUBMITTED', 'errors.conflict.test_submitted', session_id);
        const items = t.questions.map((q) => {
          const a = answers.find((x) => x.index === q.index);
          let verdict: TestVerdict = 'blank';
          if (q.kind === 'multiple_choice' && a?.choice != null) verdict = a.choice === q.key!.option ? 'correct' : 'wrong';
          if (q.kind === 'true_false' && a?.truth != null) verdict = a.truth === q.key!.truth ? 'correct' : 'wrong';
          if (q.kind === 'written' && a?.text) verdict = a.text.trim().toLowerCase() === q.key!.expected.toLowerCase() ? 'correct' : 'wrong';
          return { index: q.index, verdict };
        });
        const n = (v: TestVerdict) => items.filter((i) => i.verdict === v).length;
        for (const i of items) {
          const q = t.questions.find((x) => x.index === i.index);
          if (q && i.verdict !== 'blank') answeredToday(q.card_id, i.verdict !== 'correct', false);
        }
        t.answers = answers;
        t.result = {
          total: items.length,
          correct: n('correct'),
          almost: n('almost'),
          wrong: n('wrong'),
          blank: n('blank'),
          points: n('correct'),
          percent: items.length ? Math.round((n('correct') * 100) / items.length) : 0,
          items,
        };
        return paper(s, true);
      },
      'modes.match.start': ({ session_id }) => {
        const s = require(session_id);
        const m = s.match!;
        const seen = new Set<string>();
        const usable = m.pool.filter((c) => front(c) && back(c) && !seen.has(back(c)) && seen.add(back(c)));
        if (usable.length < 2) return { ...round(s), tiles: [], pairs: 0, cards: usable.length };
        m.rounds++;
        m.cards = shuffled(usable).slice(0, 6);
        m.tiles = shuffled(m.cards.flatMap((_, pair) => [{ pair, side: 'term' as const }, { pair, side: 'definition' as const }]));
        m.matched = m.cards.map(() => false);
        m.mistakes = 0;
        m.started_at = Date.now();
        m.completed_at = null;
        m.finished_ms = null;
        return round(s);
      },
      'modes.match.pick': ({ session_id, first, second }) => {
        const s = require(session_id);
        const m = s.match!;
        const a = m.tiles[first];
        const b = m.tiles[second];
        if (!a || !b || a.side === b.side) throw err('VALIDATION_SESSION_MODE', 'errors.validation.session_mode', session_id);
        const correct = a.pair === b.pair;
        if (correct) m.matched[a.pair] = true;
        else m.mistakes++;
        const complete = m.matched.every(Boolean);
        if (complete) m.completed_at = Date.now();
        return { correct, complete, round: round(s) };
      },
      'modes.match.finish': ({ session_id }) => {
        const s = require(session_id);
        const m = s.match!;
        const time = Math.max(0, (m.completed_at ?? Date.now()) - m.started_at);
        m.finished_ms = time;
        const key = s.dto.deck_id ?? '';
        const previous = best.get(key) ?? null;
        const newBest = previous === null || time < previous;
        if (newBest) best.set(key, time);
        return { time_ms: time, mistakes: m.mistakes, pairs: m.cards.length, previous_best_ms: previous, new_best: newBest, round: round(s) };
      },
      'study.session': ({ session_id }) => view(require(session_id)),
      'study.active': () => {
        const open = [...sessions.values()].filter((s) => s.dto.ended_at === null).pop();
        return { session: open ? view(open) : null };
      },
      'study.answer': ({ session_id, card_id, rating, duration_ms }) => {
        const s = require(session_id);
        requireCurrent(s, card_id);
        remember(s);
        const card = s.queue.shift()!;
        answeredToday(card_id, rating === 'again', s.dto.mode === 'scheduled');
        if (rating === 'again') {
          s.queue.push({ ...card, state: 'learning', waitUntil: Date.now() + learningStepMs });
          if (!s.again.includes(card_id)) s.again.push(card_id);
        } else {
          s.dto.correct++;
        }
        s.dto.answered++;
        s.time += duration_ms;
        s.hints = 0;
        return view(s);
      },
      'study.hint': ({ session_id, card_id }) => {
        const s = require(session_id);
        requireCurrent(s, card_id);
        const answer = s.queue[0]!.note.fields.back ?? '';
        if (s.hints + 1 >= answer.length) return { level: s.hints, text: null, kind: 'letters', html: null };
        s.hints++;
        const text = answer.slice(0, s.hints) + '_'.repeat(answer.length - s.hints);
        return { level: s.hints, text, kind: 'letters', html: text };
      },
      'study.check_answer': ({ session_id, card_id, given }) => {
        const s = require(session_id);
        requireCurrent(s, card_id);
        const expected = s.queue[0]!.note.fields.back ?? '';
        const ok = given.trim().toLowerCase() === expected.trim().toLowerCase();
        return {
          grade: ok ? 'correct' : 'wrong',
          score: ok ? 1 : 0,
          diff: [{ kind: ok ? 'same' : 'extra', text: given }],
          expected,
          suggested_rating: ok ? 'good' : 'again',
          auto_grade: false,
        };
      },
      'study.end': ({ session_id }) => {
        const s = require(session_id);
        s.dto.ended_at ??= Date.now();
        return summary(s);
      },
      'study.summary': ({ session_id }) => summary(require(session_id)),
      'study.suspend': ({ card_ids, session_id }) => {
        if (session_id) {
          const s = require(session_id);
          remember(s);
          s.queue = s.queue.filter((c) => !card_ids.includes(c.card_id));
        }
        return { cards: card_ids.length };
      },
      'study.bury': ({ card_ids, session_id }) => {
        if (session_id) {
          const s = require(session_id);
          remember(s);
          s.queue = s.queue.filter((c) => !card_ids.includes(c.card_id));
        }
        return { cards: card_ids.length };
      },
      'study.postpone': () => ({ cards: 0 }),
      'oplog.undo': () => {
        const last = undo.pop();
        if (!last) throw err('CONFLICT_NOTHING_TO_UNDO', 'errors.conflict.nothing_to_undo', '');
        sessions.set(last.id, JSON.parse(last.snapshot) as MockSession);
        return {
          batch_id: id('batch'),
          reverted_batch_id: id('batch'),
          summary: { key: 'oplog.summary.study.answer', params: {} },
        };
      },
    };
  },
} satisfies MockFeature;
