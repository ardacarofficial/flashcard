/**
 * Dev/test mock of the study module: a tiny queue over the mock notes (every card new,
 * "Again" sends a card to the back, fixed intervals). Not the real engine — that is fc-app's
 * `study` module; this only lets the screen run in a browser and in component tests.
 */
import { err, menu, module, type MockEnv, type MockFeature } from '$lib/api/mock/env';
import type { NoteDto, SessionDto, SessionSummary, StudyCardDto } from '$lib/api/types';

const DAY = 86_400_000;
const esc = (s: string) => s.replace(/[&<>"]/g, (c) => `&#${c.charCodeAt(0)};`);
const INTERVALS = [60_000, 6 * 60_000, 10 * 60_000, 4 * DAY];

interface MockCard {
  card_id: string;
  note: NoteDto;
}

interface MockSession {
  dto: SessionDto;
  queue: MockCard[];
  again: string[];
  time: number;
  hints: number;
}

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
    const view = (s: MockSession): SessionDto => ({
      ...s.dto,
      counts: { new: s.queue.length, learning: 0, review: 0 },
      mistakes: s.again.length,
      current: current(s),
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
        sessions.set(s.dto.id, s);
        return view(s);
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
        s.dto.ended_at ??= now();
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
