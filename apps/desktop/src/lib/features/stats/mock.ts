/**
 * Dev-only mock of the v0.6 modules (stats, planning, goals, optimizer). Returns fixed,
 * plausible numbers so the screens can be demoed and tested without Rust; every real number
 * is computed in fc-core / fc-app.
 */
import { err, menu, module, type MockEnv, type MockFeature } from '$lib/api/mock/env';
import type { CardStats, ExamDto, GoalDto, ReviewStats, StatsOverview } from '$lib/api/types';

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
    let suspended = 0;
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
      'stats.leeches': () => ({ threshold: 8, total: leeches, items: [] }),
      'stats.apply_to_leeches': ({ threshold, action }) => {
        const changed = action === 'suspend' ? leeches - suspended : leeches;
        if (action === 'suspend') suspended = leeches;
        return { threshold: threshold ?? 8, cards: leeches, changed, skipped: leeches - changed };
      },
      'stats.deck_reports': () => ({ items: [] }),
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
            daily: goal.kind === 'study_minutes' && goal.deadline === null,
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
