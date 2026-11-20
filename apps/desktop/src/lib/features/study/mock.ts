/**
 * Dev/test mock of the study module: a tiny queue over the mock notes (every card new,
 * "Again" sends a card to the back, fixed intervals). Not the real engine — that is fc-app's
 * `study` module; this only lets the screen run in a browser and in component tests.
 */
import { err, menu, module, type MockEnv, type MockFeature } from '$lib/api/mock/env';
import type {
  MatchRoundDto,
  NoteDto,
  SessionDto,
  SessionModesDto,
  SessionSummary,
  StudyCardDto,
  TestPaperDto,
  TestQuestionDto,
  TestVerdict,
} from '$lib/api/types';

const DAY = 86_400_000;
const esc = (s: string) => s.replace(/[&<>"]/g, (c) => `&#${c.charCodeAt(0)};`);
const INTERVALS = [60_000, 6 * 60_000, 10 * 60_000, 4 * DAY];

interface MockCard {
  card_id: string;
  note: NoteDto;
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
  test?: { questions: TestQuestionDto[]; result: TestPaperDto['result']; answers: TestPaperDto['answers'] };
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

export default {
  modules: [module('study', [menu('study.session', 'nav.study', '/study', 5)])],
  handlers: ({ id, now, notes, ancestors, cardFlags }: MockEnv) => {
    const sessions = new Map<string, MockSession>();
    const undo: Array<{ id: string; snapshot: string }> = [];

    const require = (sessionId: string): MockSession => {
      const s = sessions.get(sessionId);
      if (!s) throw err('NOT_FOUND_SESSION', 'errors.not_found.session', sessionId);
      return s;
    };
    const current = (s: MockSession): StudyCardDto | null => {
      const c = s.queue[0];
      if (!c || s.dto.ended_at !== null) return null;
      return {
        card_id: c.card_id,
        note_id: c.note.id,
        deck_id: c.note.home_deck_id,
        state: 'new',
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
      counts: { new: s.queue.length, learning: 0, review: 0 },
      mistakes: s.again.length,
      current: s.test || s.match ? null : current(s),
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
    const remember = (s: MockSession) => undo.push({ id: s.dto.id, snapshot: JSON.stringify(s) });

    return {
      'study.start': (req) => {
        for (const s of sessions.values()) s.dto.ended_at ??= now();
        const from = req.from_session ? require(req.from_session) : null;
        const queue: MockCard[] = notes
          .filter((n) => !req.deck_id || ancestors(n.home_deck_id).includes(req.deck_id))
          .flatMap((n) => n.cards.map((c) => ({ card_id: c.id, note: n })))
          .filter((c) => !from || from.again.includes(c.card_id));
        const s: MockSession = {
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
        };
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
        if (rating === 'again') {
          s.queue.push(card);
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
        if (s.hints + 1 >= answer.length) return { level: s.hints, text: null };
        s.hints++;
        return { level: s.hints, text: answer.slice(0, s.hints) + '_'.repeat(answer.length - s.hints) };
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
