#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::day::StudyDay;
use crate::scheduler::{builtin, retrievability, FSRS_ID};
use fc_core::card::{CardState, Rating, SchedulingState};
use fc_core::rng::{Rng, SplitMix64};
use fc_providers::strategy::SchedulerParams;
use std::collections::HashMap;

#[test]
fn fuzz_range_widths() {
    assert_eq!(fuzz_range(1, 36_500), (1, 1));
    assert_eq!(fuzz_range(2, 36_500), (2, 2));
    let (lo, hi) = fuzz_range(10, 36_500);
    assert!(lo < 10 && hi > 10, "{lo}..{hi}");
    let (lo, hi) = fuzz_range(100, 36_500);
    assert!(
        (93..=96).contains(&lo) && (104..=107).contains(&hi),
        "{lo}..{hi}"
    );
    // Never past the maximum, always containing the interval.
    assert_eq!(fuzz_range(100, 100).1, 100);
    for i in 1..400 {
        let (lo, hi) = fuzz_range(i, 365);
        assert!(lo <= i.min(365) && i.min(365) <= hi && hi <= 365 && lo >= 1);
    }
}

#[test]
fn picks_the_least_loaded_day_and_respects_easy_days() {
    let today = fc_core::stats::day_of_date(2026, 11, 16); // a Monday
    let opts = PlanOptions::default();
    let busy = |d: i64| {
        if d == today + 10 {
            50
        } else {
            10 + u32::try_from(d - today).unwrap()
        }
    };
    let ivl = plan_interval(today, 10, 36_500, None, &opts, &busy);
    let (lo, _) = fuzz_range(10, 36_500);
    assert_eq!(ivl, lo, "lowest load is the earliest day here");
    // Flat load: the proposed day stays.
    assert_eq!(plan_interval(today, 10, 36_500, None, &opts, &|_| 5), 10);
    // Balancing off and no easy days: untouched even under load.
    let off = PlanOptions {
        load_balance: false,
        ..PlanOptions::default()
    };
    assert_eq!(plan_interval(today, 10, 36_500, None, &off, &busy), 10);
    // today + 12 is a Saturday: a minimum day is avoided when another day is free enough.
    let mut easy = off;
    easy.easy_days[5] = EasyDay::Minimum;
    easy.easy_days[6] = EasyDay::Minimum;
    let ivl = plan_interval(today, 12, 36_500, None, &easy, &|_| 0);
    assert!(
        fc_core::stats::weekday(today + i64::from(ivl)) < 5,
        "moved off the weekend"
    );
}

#[test]
fn exam_pulls_reviews_before_the_exam() {
    let today = 1000;
    let exam = ExamDay { day: 1020 };
    let opts = PlanOptions::default();
    // A 60-day interval would skip the exam: reviewed in the 3 days before it.
    let ivl = plan_interval(today, 60, 36_500, Some(exam), &opts, &|_| 0);
    assert!((17..=19).contains(&ivl), "{ivl}");
    // Short intervals before the exam are untouched.
    assert_eq!(
        plan_interval(today, 5, 36_500, Some(exam), &opts, &|_| 0),
        5
    );
    // Exam tomorrow: seen on exam day.
    assert_eq!(
        plan_interval(1019, 30, 36_500, Some(exam), &opts, &|_| 0),
        1
    );
    // After the exam: no effect.
    let (lo, hi) = candidate_range(1021, 30, 36_500, Some(exam));
    assert_eq!((lo, hi), fuzz_range(30, 36_500));
}

#[test]
fn workload_grows_with_retention() {
    let s = [5.0, 20.0, 80.0, 300.0];
    let w80 = daily_workload(&s, 0.80, 0.1542, 0, 0.0);
    let w90 = daily_workload(&s, 0.90, 0.1542, 0, 0.0);
    let w97 = daily_workload(&s, 0.97, 0.1542, 0, 0.0);
    assert!(w80 < w90 && w90 < w97, "{w80} {w90} {w97}");
    // At 90 % recall the interval equals the stability (definition of S).
    assert!((interval_at(20.0, 0.9, 0.1542).unwrap() - 20.0).abs() < 1e-9);
    assert!(daily_workload(&[], 0.9, 0.15, 10, 3.0) > 29.0);
}

// ------------------------------------------------------------------ simulation

struct Sim {
    /// Reviews answered each day.
    daily: Vec<u32>,
    /// Final states.
    cards: Vec<SchedulingState>,
}

/// `days` of study: `new_per_day` new cards on each of the first `intro_days` days, every due
/// card answered (recalled with its FSRS probability, deterministic seed), intervals
/// post-processed by [`plan_interval`] when `plan` is set.
fn simulate(
    days: i64,
    intro_days: i64,
    new_per_day: usize,
    plan: Option<PlanOptions>,
    exam: Option<ExamDay>,
) -> Sim {
    let cal = StudyDay::default();
    let sched = builtin(FSRS_ID).unwrap();
    let params = SchedulerParams {
        desired_retention: 0.9,
        maximum_interval_days: 36_500,
        learning_steps_mins: Vec::new(),
        relearning_steps_mins: Vec::new(),
        extra: serde_json::Value::Null,
    };
    let mut cards: Vec<SchedulingState> = Vec::new();
    let mut due_day: Vec<i64> = Vec::new();
    let mut load: HashMap<i64, u32> = HashMap::new();
    let mut rng = SplitMix64::new(42);
    let mut daily = Vec::new();
    for today in 0..days {
        if today < intro_days {
            for _ in 0..new_per_day {
                cards.push(SchedulingState::NEW);
                due_day.push(today);
            }
        }
        let now = cal.start(today).plus_millis(10 * 3_600_000);
        let mut answered = 0;
        for i in 0..cards.len() {
            if due_day[i] > today {
                continue;
            }
            answered += 1;
            let state = &cards[i];
            let recall = match (state.state, state.stability, state.last_review) {
                (CardState::New, _, _) => 1.0,
                (_, Some(s), Some(last)) => {
                    let elapsed = f64::from(cal.days_between(last, now));
                    retrievability(s, elapsed, &params).unwrap()
                }
                _ => 1.0,
            };
            let roll = (rng.next_u64() >> 11) as f64 / (1u64 << 53) as f64;
            let rating = if roll < recall {
                Rating::Good
            } else {
                Rating::Again
            };
            let out = sched.schedule(state, rating, now, &params).unwrap();
            let mut ivl = out.scheduled_days.max(1);
            if let Some(opts) = plan {
                if let Some(c) = load.get_mut(&due_day[i]) {
                    *c = c.saturating_sub(1);
                }
                ivl = plan_interval(today, ivl, 36_500, exam, &opts, &|d| {
                    load.get(&d).copied().unwrap_or(0)
                });
            }
            let mut next = out.state;
            next.due = Some(cal.start(today + i64::from(ivl)));
            due_day[i] = today + i64::from(ivl);
            *load.entry(due_day[i]).or_insert(0) += 1;
            cards[i] = next;
        }
        daily.push(answered);
    }
    Sim { daily, cards }
}

/// Variance of the daily load around its centered 15-day moving average: how lumpy the days
/// are, without the slow rise and fall of the total workload (which balancing must not
/// change).
fn variance(xs: &[u32]) -> f64 {
    const HALF: usize = 7;
    let residuals: Vec<f64> = (HALF..xs.len() - HALF)
        .map(|i| {
            let window = &xs[i - HALF..=i + HALF];
            let mean = window.iter().map(|x| f64::from(*x)).sum::<f64>() / window.len() as f64;
            f64::from(xs[i]) - mean
        })
        .collect();
    residuals.iter().map(|r| r * r).sum::<f64>() / residuals.len() as f64
}

/// docs/04 v0.6 acceptance: load balancing lowers the variance of the daily load.
#[test]
fn load_balancing_lowers_daily_load_variance() {
    let plain = simulate(365, 120, 20, None, None);
    let balanced = simulate(365, 120, 20, Some(PlanOptions::default()), None);
    // After the first weeks: same workload, flatter days.
    let window = 30..365;
    let v_plain = variance(&plain.daily[window.clone()]);
    let v_bal = variance(&balanced.daily[window.clone()]);
    let total = |s: &Sim| s.daily[window.clone()].iter().sum::<u32>();
    eprintln!("daily-load variance: balanced {v_bal:.1}, plain {v_plain:.1}");
    assert!(
        v_bal < v_plain * 0.5,
        "variance balanced {v_bal:.1} vs plain {v_plain:.1}"
    );
    let (tp, tb) = (f64::from(total(&plain)), f64::from(total(&balanced)));
    assert!(
        (tb - tp).abs() / tp < 0.1,
        "same total work (±10 %): {tp} vs {tb}"
    );
    let peak = |s: &Sim| *s.daily[window.clone()].iter().max().unwrap();
    assert!(peak(&balanced) < peak(&plain));
}

/// Easy days get noticeably fewer reviews.
#[test]
fn easy_days_get_fewer_reviews() {
    let mut opts = PlanOptions::default();
    opts.easy_days[5] = EasyDay::Reduced;
    opts.easy_days[6] = EasyDay::Minimum;
    let sim = simulate(365, 120, 20, Some(opts), None);
    let (mut weekend, mut weekday_sum, mut nw, mut nd) = (0u32, 0u32, 0u32, 0u32);
    for (d, n) in sim.daily.iter().enumerate().skip(60) {
        if fc_core::stats::weekday(i64::try_from(d).unwrap()) >= 5 {
            weekend += n;
            nw += 1;
        } else {
            weekday_sum += n;
            nd += 1;
        }
    }
    let (we, wd) = (
        f64::from(weekend) / f64::from(nw),
        f64::from(weekday_sum) / f64::from(nd),
    );
    assert!(we < wd * 0.75, "weekend {we:.1} vs weekday {wd:.1}");
}

/// Exam mode: recall on exam day is higher than without it.
#[test]
fn exam_mode_maximizes_recall_on_exam_day() {
    let exam = ExamDay { day: 90 };
    let base = simulate(90, 40, 10, Some(PlanOptions::default()), None);
    let exam_sim = simulate(90, 40, 10, Some(PlanOptions::default()), Some(exam));
    let cal = StudyDay::default();
    let at_exam = cal.start(exam.day).plus_millis(10 * 3_600_000);
    let params = SchedulerParams {
        desired_retention: 0.9,
        maximum_interval_days: 36_500,
        learning_steps_mins: Vec::new(),
        relearning_steps_mins: Vec::new(),
        extra: serde_json::Value::Null,
    };
    let mean_recall = |s: &Sim| {
        let rs: Vec<f64> = s
            .cards
            .iter()
            .map(|c| {
                let elapsed = f64::from(cal.days_between(c.last_review.unwrap(), at_exam));
                retrievability(c.stability.unwrap(), elapsed, &params).unwrap()
            })
            .collect();
        rs.iter().sum::<f64>() / rs.len() as f64
    };
    let (rb, re) = (mean_recall(&base), mean_recall(&exam_sim));
    assert!(re > rb + 0.02 && re > 0.95, "exam mode {re:.3} vs {rb:.3}");
    // Every card was seen in the window right before the exam.
    for c in &exam_sim.cards {
        let last = cal.index(c.last_review.unwrap());
        assert!(
            last >= exam.day - i64::from(EXAM_WINDOW_DAYS)
                || c.due.is_some_and(|d| cal.index(d) <= exam.day)
        );
    }
}
