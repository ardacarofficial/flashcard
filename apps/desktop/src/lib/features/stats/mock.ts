/**
 * Dev-only mock of the v0.6 modules (stats, planning, goals, optimizer). Returns fixed,
 * plausible numbers so the screens can be demoed and tested without Rust; every real number
 * is computed in fc-core / fc-app.
 */
import { err, menu, module, type MockEnv, type MockFeature } from '$lib/api/mock/env';
import type { CardRow, CardStats, ExamDto, GoalDto, LeechRepairDto, LeechRepairPlan, ReviewStats, StatsOverview, StruggleItem, WorkloadForecast } from '$lib/api/types';

const rate = (count: number, pass: number) => ({ count, pass, rate: count ? pass / count : null });

function reviews(): ReviewStats {
  return {
    streak: { current: 4, longest: 12, studied_today: true },
    totals: { reviews: 120, time_ms: 1_800_000, days_studied: 9, days_in_range: 30, new_cards: 40, imported: 0 },
    success: { learn: rate(50, 45), young: rate(40, 34), mature: rate(30, 28), relearn: rate(0, 0), all: rate(120, 107) },
    true_retention: 62 / 70,
    heatmap: Array.from({ length: 90 }, (_, d) => {
      const n = (d * 37) % 11 === 0 ? 0 : 8 + ((d * 53) % 60);
      const date = new Date(Date.UTC(2025, 7, 7 + d)).toISOString().slice(0, 10);
      return { day: 20311 + d, date, reviews: n, time_ms: n * 15_000 };
    }),
    weekly: [{ start_day: 20398, start_date: '2025-10-27', reviews: 120, pass: 107, rate: 107 / 120, new_cards: 40, time_ms: 1_800_000 }],
    monthly: [{ start_day: 20392, start_date: '2025-11-01', reviews: 120, pass: 107, rate: 107 / 120, new_cards: 40, time_ms: 1_800_000 }],
    hours: Array.from({ length: 24 }, (_, hour) => ({ hour, stat: hour === 9 ? rate(120, 107) : rate(0, 0) })),
    answer_times: {
      mean_ms: 15_000,
      buckets: [{ up_to_ms: 2000, count: 20 }, { up_to_ms: null, count: 100 }],
      slowest: [],
    },
  };
}

function cards(): CardStats {
  return {
    states: { new: 140, learning: 18, relearning: 6, young: 210, mature: 310, suspended: 9, buried: 2, total: 695 },
    forecast: Array.from({ length: 30 }, (_, i) => ({
      day: 20404 + i,
      date: new Date(Date.UTC(2025, 10, 4 + i)).toISOString().slice(0, 10),
      due: 12 + ((i * 7) % 29),
    })),
    overdue: 0,
    stability: [{ from: null, to: 1, count: 30 }, { from: 1, to: 7, count: 120 }, { from: 7, to: 30, count: 210 }, { from: 30, to: null, count: 180 }],
    difficulty: [{ from: null, to: 3, count: 90 }, { from: 3, to: 6, count: 260 }, { from: 6, to: null, count: 120 }],
    report: { cards: 695, studied: 545, mature: 310, settled_pct: 44.6, studied_pct: 78.4, mean_recall: 0.91 },
  };
}

const iso = (offset: number) => new Date(Date.UTC(2025, 10, 4 + offset)).toISOString().slice(0, 10);

/**
 * A plausible forecast shape for the demo: steady reviews plus each day's new cards coming
 * back on a few fixed intervals (the real model is `fc_study::planning::forecast`).
 */
function mockForecast(deckId: string | null, pace: number, current: number, presetName: string | null, exam: string | null, days = 28): WorkloadForecast {
  const unseen = 140;
  const intro: number[] = [];
  let left = unseen;
  for (let d = 0; d < days; d++) {
    const n = Math.min(pace, left);
    intro.push(n);
    left -= n;
  }
  const secs = 9.5;
  const pts = Array.from({ length: days }, (_, d) => {
    let reviews = 18 + ((d * 7) % 11);
    for (const back of [1, 3, 7, 16]) if (d - back >= 0) reviews += intro[d - back]! * 0.9;
    const cards = reviews + intro[d]! * 3;
    return { offset: d, date: iso(d), cards, reviews, new_cards: intro[d]!, minutes: (cards * secs) / 60 };
  });
  const week = (end: number) => {
    const w = pts.slice(Math.max(0, end - 6), end + 1);
    return {
      offset: end,
      date: iso(end),
      cards_per_day: w.reduce((a, p) => a + p.cards, 0) / w.length,
      minutes_per_day: w.reduce((a, p) => a + p.minutes, 0) / w.length,
    };
  };
  const weeks = Array.from({ length: Math.ceil(days / 7) }, (_, i) => {
    const w = pts.slice(i * 7, i * 7 + 7);
    return {
      week: i + 1,
      start_date: w[0]!.date,
      end_date: w.at(-1)!.date,
      cards_per_day: w.reduce((a, p) => a + p.cards, 0) / w.length,
      minutes_per_day: w.reduce((a, p) => a + p.minutes, 0) / w.length,
      new_cards: w.reduce((a, p) => a + p.new_cards, 0),
    };
  });
  const peak = pts.reduce((b, p) => (p.cards > b.cards ? p : b), pts[0]!);
  const done = intro.findIndex((_, d) => intro.slice(0, d + 1).reduce((a, b) => a + b, 0) >= unseen);
  const examLeft = exam ? Math.round((Date.parse(exam) - Date.parse(iso(0))) / 86_400_000) : null;
  return {
    deck_id: deckId,
    new_per_day: pace,
    current_new_per_day: current,
    new_done_today: 0,
    unseen,
    started: 555,
    secs_per_answer: secs,
    measured_time: true,
    retention: 0.9,
    days: pts,
    weeks,
    peak,
    outlook: week(Math.min(21, days - 1)),
    new_finished_on: pace > 0 && done >= 0 ? iso(done) : null,
    exam:
      exam && examLeft !== null
        ? {
            date: exam,
            days_left: examLeft,
            new_left: Math.max(0, unseen - pace * Math.max(0, examLeft)),
            new_per_day_needed: examLeft >= 1 ? Math.ceil(unseen / examLeft) : null,
            last_week: week(Math.max(0, Math.min(days - 1, examLeft - 1))),
          }
        : null,
    target: deckId
      ? {
          deck: { id: deckId, path: 'İngilizce' },
          preset_id: presetName ? 'preset-mock' : null,
          preset_name: presetName,
          current_new_per_day: current,
          decks: [{ id: deckId, path: 'İngilizce' }],
        }
      : null,
  };
}

const STRUGGLES: StruggleItem[] = [
  ['went', 'gitmek (geçmiş)', 4, 6],
  ['although', 'rağmen', 3, 5],
  ['thorough', 'titiz, eksiksiz', 3, 4],
  ['bring up', 'gündeme getirmek', 2, 4],
  ['reluctant', 'isteksiz', 2, 3],
  ['eventually', 'sonunda', 1, 3],
].map(([term, meaning, misses, answers], i) => ({
  card_id: `card-struggle-${i}`,
  note_id: `note-struggle-${i}`,
  deck_id: 'deck-1',
  deck_path: 'İngilizce',
  term: term as string,
  meaning: meaning as string,
  misses: misses as number,
  answers: answers as number,
  last_miss_at: Date.UTC(2025, 10, 4, 9) - i * 3_600_000 * 7,
  last_seen_at: Date.UTC(2025, 10, 4, 9) - i * 3_600_000 * 5,
  lapses: misses as number,
  suspended: false,
}));

/** The first leech rows (the rest exist only as a count). */
const LEECH_ROWS: CardRow[] = [
  ['give up', 11],
  ['although', 9],
  ['thorough', 8],
].map(([term, lapses], i) => ({
  card_id: `card-leech-${i}`,
  note_id: `note-leech-${i}`,
  deck_id: 'deck-1',
  deck_path: 'İngilizce',
  notetype_id: 'nt-vocab',
  sort_field: term as string,
  template_key: 'recognition',
  card_type: 'Tanıma',
  state: 'review',
  due: null,
  stability: 3.2,
  difficulty: 8.1,
  reps: 20,
  lapses: lapses as number,
  flags: 0,
  suspended: false,
  buried: false,
  tags: [],
  created_at: Date.UTC(2025, 8, 1),
  note_updated_at: Date.UTC(2025, 8, 1),
}));

export default {
  modules: [
    module('stats', [menu('stats.overview', 'nav.stats', '/stats', 40)]),
    module('planning', [menu('planning.screen', 'nav.planning', '/planning', 41)]),
    module('goals', [menu('goals.screen', 'nav.goals', '/goals', 42)]),
    module('optimizer', []),
  ],
  handlers: ({ now, id, emit }: MockEnv) => {
    const exams: ExamDto[] = [];
    const goals: GoalDto[] = [];
    const settings = new Map<string, unknown>();
    // Leeches exist only as a count here (rows come from the real search in Rust).
    let leeches = 60;
    const repairs: LeechRepairDto[] = [];
    let suspended = 0;
    // Pace planner: the preset's new cards a day (null preset until applied once).
    let presetPace = 20;
    let presetName: string | null = null;
    const overview = (): StatsOverview => ({
      today: 20404,
      today_date: '2025-11-04',
      from_day: null,
      reviews: reviews(),
      cards: cards(),
      target_retention: 0.9,
      from_cache: true,
    });
    return {
      'settings.get': ({ keys }) => ({
        values: (keys ?? []).map((key) => ({ key, value: settings.get(key) ?? null, source: null })),
      }),
      'settings.set': ({ scope, key, value }) => {
        settings.set(key, value);
        return { key, value, source: scope };
      },
      'stats.refresh_cache': () => ({ rebuilt: false, days: 3, folded: 0 }),
      'stats.overview': () => overview(),
      'stats.breakdown': ({ attr_id }) => ({ attr_id, name: 'Seviye', rows: [] }),
      'stats.leeches': () => ({ threshold: 8, total: leeches, items: LEECH_ROWS }),
      'stats.leech_repairs.plan': ({ card_id }): LeechRepairPlan => {
        const row = LEECH_ROWS.find((r) => r.card_id === card_id) ?? LEECH_ROWS[0]!;
        const done = repairs.filter((r) => r.card_id === row.card_id);
        const accepted = done.length ? ['pes etmek', 'bırakmak'] : [];
        return {
          card_id: row.card_id,
          note_id: row.note_id,
          lapses: row.lapses,
          threshold: 8,
          is_leech: true,
          failed_reviews: [
            { at: Date.UTC(2025, 10, 4, 9), mode: 'learn', duration_ms: 9000, answer_duration_ms: 7000, hints_used: 0, typed_answer: 'pes etmek' },
            { at: Date.UTC(2025, 10, 2, 20), mode: 'normal', duration_ms: 6000, answer_duration_ms: null, hints_used: 1, typed_answer: null },
          ],
          typed_answers_recorded: true,
          answer_field: 'meaning',
          answer: 'vazgeçmek; pes etmek, bırakmak',
          senses: ['vazgeçmek', 'pes etmek', 'bırakmak'],
          accepted_answers: accepted,
          examples: null,
          confusables: [],
          proposals: done.length
            ? []
            : [
                {
                  repair: { kind: 'add_accepted_answers', field: null, answers: ['pes etmek', 'bırakmak'] },
                  method: 'accepted_answers.add',
                  why: 'senses_not_accepted',
                  preview: [{ field: 'accepted_answers', before: '', after: 'pes etmek, bırakmak' }],
                  ai_preview: null,
                },
                { repair: { kind: 'ai_rewrite', fields: {} }, method: 'ai.notes.update', why: 'fresh_angle', preview: [], ai_preview: null },
              ],
          repairs: done,
        };
      },
      'stats.leech_repairs.apply': ({ card_id, repair }): LeechRepairDto => {
        const row = LEECH_ROWS.find((r) => r.card_id === card_id) ?? LEECH_ROWS[0]!;
        const rec: LeechRepairDto = { id: repairs.length + 1, card_id, note_id: row.note_id, kind: repair.kind, repair, lapses: row.lapses, applied_at: now() };
        repairs.push(rec);
        return rec;
      },
      'stats.leech_repairs.list': ({ card_id }) => ({
        items: repairs
          .filter((r) => !card_id || r.card_id === card_id)
          .map((repair) => ({
            repair,
            before: { reviews: 10, passed: 4, rate: 0.4 },
            after: { reviews: 3, passed: 3, rate: 1 },
            next_reviews: [],
            small_sample: true,
          })),
      }),
      'words.stats': () => ({
        threshold_days: 21,
        languages: [
          {
            language: 'en',
            counts: { new: 140, learning: 234, known: 312, ignored: 4 },
            bands: [],
          },
        ],
        unassigned_notes: 12,
      }),
      'stats.apply_to_leeches': ({ threshold, action }) => {
        const changed = action === 'suspend' ? leeches - suspended : leeches;
        if (action === 'suspend') suspended = leeches;
        return { threshold: threshold ?? 8, cards: leeches, changed, skipped: leeches - changed };
      },
      'stats.deck_reports': () => ({ items: [] }),
      'stats.today': () => ({
        today_date: '2025-11-04',
        today: { answered: 42, correct: 37, rate: 37 / 42, time_ms: 9 * 60_000, new_cards: 8 },
        yesterday: { answered: 45, correct: 39, rate: 39 / 45, time_ms: 10 * 60_000, new_cards: 10 },
        week_mean_answered: 38.5,
        pace: 'same' as const,
        reference_answered: 45,
        reference_is_yesterday: true,
        streak: { current: 4, longest: 12, studied_today: true },
      }),
      'stats.struggles': ({ days }) => {
        const items = (days === 1 ? STRUGGLES.slice(0, 2) : STRUGGLES).map((i) => ({ ...i }));
        const studyable = items.filter((i) => !i.suspended);
        return {
          days: days ?? 7,
          total: items.length,
          today: 2,
          items,
          study_query: studyable.length ? `cid:${studyable.map((i) => i.card_id).join(',')}` : null,
        };
      },
      'planning.forecast': ({ deck_id, new_per_day, exam_date, days }) =>
        mockForecast(deck_id ?? null, new_per_day ?? presetPace, presetPace, presetName, exam_date ?? null, Math.max(days ?? 28, 1)),
      'planning.apply_pace': ({ deck_id, new_per_day, exam_date, exam_name }) => {
        const previous = presetPace;
        const created = presetName === null;
        presetName ??= 'İngilizce';
        presetPace = new_per_day;
        emit({ type: 'presets_changed' });
        let exam: ExamDto | null = null;
        if (exam_date) {
          const t = now();
          exam = { id: id('exam'), name: exam_name ?? 'İngilizce', date: exam_date, deck_id, query: null, created_at: t, updated_at: t };
          exams.push(exam);
          emit({ type: 'exams_changed' });
        }
        return {
          target: mockForecast(deck_id, presetPace, presetPace, presetName, null).target!,
          previous_new_per_day: previous,
          created_preset: created,
          exam,
        };
      },
      'planning.exams.list': () => ({
        items: exams.map((exam) => ({ exam, days_left: 30, cards: 0 })),
      }),
      'planning.exams.create': ({ name, date, deck_id, query }) => {
        if (!name.trim()) throw err('VALIDATION_EXAM_NAME', 'errors.validation.exam_name', name);
        const t = now();
        const e: ExamDto = { id: id('exam'), name: name.trim(), date, deck_id, query, created_at: t, updated_at: t };
        exams.push(e);
        emit({ type: 'exams_changed' });
        return e;
      },
      'planning.exams.update': (req) => {
        const e = exams.find((x) => x.id === req.id);
        if (!e) throw err('NOT_FOUND_EXAM', 'errors.not_found.exam', req.id);
        Object.assign(e, { ...req, updated_at: now() });
        emit({ type: 'exams_changed' });
        return e;
      },
      'planning.exams.delete': ({ id: eid }) => {
        exams.splice(exams.findIndex((x) => x.id === eid), 1);
        emit({ type: 'exams_changed' });
        return {};
      },
      'goals.list': () => ({
        items: goals.map((goal) => ({
          goal,
          progress: {
            current: 30,
            target: goal.target,
            percent: Math.min(100, (30 / goal.target) * 100),
            done: goal.target <= 30,
            daily: (goal.kind === 'study_minutes' || goal.kind === 'daily_cards') && goal.deadline === null,
            projected_day: null,
            projected_date: goal.deadline,
            days_late: goal.deadline ? 3 : null,
          },
        })),
      }),
      'goals.create': ({ kind, target, deadline, query }) => {
        const t = now();
        const g: GoalDto = { id: id('goal'), kind, target, deadline, query, created_at: t, updated_at: t };
        goals.push(g);
        emit({ type: 'goals_changed' });
        return g;
      },
      'goals.update': (req) => {
        const g = goals.find((x) => x.id === req.id);
        if (!g) throw err('NOT_FOUND_GOAL', 'errors.not_found.goal', req.id);
        Object.assign(g, { ...req, updated_at: now() });
        emit({ type: 'goals_changed' });
        return g;
      },
      'goals.delete': ({ id: gid }) => {
        goals.splice(goals.findIndex((x) => x.id === gid), 1);
        emit({ type: 'goals_changed' });
        return {};
      },
      'optimizer.workload': () => ({
        current_retention: 0.9,
        review_cards: 27,
        new_per_day: 20,
        points: [0.8, 0.9, 0.95].map((r) => ({ retention: r, reviews_per_day: 40 * r, relative: r / 0.9 })),
      }),
      'optimizer.run': ({ preset_id }) => {
        const t = now();
        const job = {
          id: id('job'),
          kind: 'optimizer.run',
          status: 'succeeded' as const,
          progress: 1,
          message: null,
          error: null,
          result: { preset_id, items: 100, cards: 20, weights: Array(21).fill(1), previous: [] },
          created_at: t,
          updated_at: t,
        };
        return { job };
      },
    };
  },
} satisfies MockFeature;
