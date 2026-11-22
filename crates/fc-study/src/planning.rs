//! Smart planning (docs/02 §5, v0.6): **load balancing**, **easy days**, **exam-date mode**,
//! the **workload estimate** of a desired retention and (v1.3) the day-by-day workload
//! [`forecast`] of a new-cards-per-day pace. Pure: loads, calendars and weights come in
//! as arguments.
//!
//! [`plan_interval`] post-processes the interval a [`Scheduler`](fc_providers::strategy::Scheduler)
//! proposes for a review card:
//!
//! 1. A **fuzz range** around the proposed interval ([`fuzz_range`], Anki's widths: none under
//!    2.5 days, then ±15 % / ±10 % / ±5 % growing with the interval, at least ±1 day) is the set
//!    of acceptable days — all of them keep the card near its target recall.
//! 2. **Exam mode**: when the range would carry the card past an exam it belongs to, the range
//!    moves to the last days before the exam ([`EXAM_WINDOW_DAYS`]), so the card is reviewed
//!    shortly before it (recall at the exam is maximized; the load right before the exam is
//!    spread over the window).
//! 3. Inside the range the day with the lowest **cost** wins: `(load + 1) / weight`, where `load`
//!    is the number of cards already due that day (when load balancing is on, else 0) and
//!    `weight` the easy-day weight of its weekday (1 = normal, [`REDUCED_WEIGHT`],
//!    [`MINIMUM_WEIGHT`]). Ties go to the day closest to the proposed one, then the earlier.
//!
//! Deterministic (no fuzz randomness), so undo/redo and tests replay exactly.

use fc_core::stats::weekday;

pub mod forecast;

/// Easy-day weight of a "reduced" weekday.
pub const REDUCED_WEIGHT: f64 = 0.5;
/// Easy-day weight of a "minimum" weekday (only when every other day is far busier).
pub const MINIMUM_WEIGHT: f64 = 0.1;
/// Days before an exam over which pulled-in reviews are spread.
pub const EXAM_WINDOW_DAYS: u32 = 3;

/// How a weekday is treated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EasyDay {
    /// Normal load.
    #[default]
    Normal,
    /// About half the load.
    Reduced,
    /// As little as possible.
    Minimum,
}

impl EasyDay {
    /// Weight of the day in [`plan_interval`].
    pub fn weight(self) -> f64 {
        match self {
            Self::Normal => 1.0,
            Self::Reduced => REDUCED_WEIGHT,
            Self::Minimum => MINIMUM_WEIGHT,
        }
    }

    /// From its setting value (`normal`, `reduced`, `minimum`; anything else is normal).
    pub fn parse(s: &str) -> Self {
        match s {
            "reduced" => Self::Reduced,
            "minimum" => Self::Minimum,
            _ => Self::Normal,
        }
    }
}

/// Options of [`plan_interval`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlanOptions {
    /// Spread reviews over the fuzz range by load.
    pub load_balance: bool,
    /// Easy-day setting per weekday, Monday first.
    pub easy_days: [EasyDay; 7],
}

impl Default for PlanOptions {
    fn default() -> Self {
        Self {
            load_balance: true,
            easy_days: [EasyDay::Normal; 7],
        }
    }
}

impl PlanOptions {
    /// Whether planning can change anything at all.
    pub fn is_active(&self) -> bool {
        self.load_balance || self.easy_days.iter().any(|d| *d != EasyDay::Normal)
    }
}

/// Acceptable intervals `(lo, hi)` around `interval` (days), within `1..=max`.
pub fn fuzz_range(interval: u32, max: u32) -> (u32, u32) {
    let max = max.max(1);
    let ivl = f64::from(interval.min(max));
    if ivl < 2.5 {
        return (interval.clamp(1, max), interval.clamp(1, max));
    }
    let delta = 1.0
        + 0.15 * (ivl.min(7.0) - 2.5)
        + 0.10 * (ivl.min(20.0) - 7.0).max(0.0)
        + 0.05 * (ivl - 20.0).max(0.0);
    // Both in range after clamping, so the casts are exact.
    let lo = (ivl - delta).round().clamp(2.0, f64::from(max)) as u32;
    let hi = (ivl + delta).round().clamp(f64::from(lo), f64::from(max)) as u32;
    (lo.min(interval.max(1)), hi.max(interval.min(max)))
}

/// An exam the card belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExamDay {
    /// Study-day index of the exam.
    pub day: i64,
}

/// The acceptable intervals `(lo, hi)` (days from `today`) after exam mode.
pub fn candidate_range(today: i64, interval: u32, max: u32, exam: Option<ExamDay>) -> (u32, u32) {
    let (mut lo, mut hi) = fuzz_range(interval, max);
    if let Some(exam) = exam {
        let to_exam = exam.day - today;
        if to_exam >= 1 && today + i64::from(lo) >= exam.day {
            // Would only be seen after the exam: pull it into the last days before it.
            let latest = u32::try_from((to_exam - 1).max(1)).unwrap_or(1);
            let earliest = latest.saturating_sub(EXAM_WINDOW_DAYS - 1).max(1);
            lo = earliest.min(latest);
            hi = latest;
        } else if to_exam >= 1 && today + i64::from(hi) >= exam.day {
            // Straddles the exam: keep the part before it.
            hi = u32::try_from((to_exam - 1).max(i64::from(lo))).unwrap_or(lo);
        }
    }
    (lo, hi)
}

/// Picks the interval (days) for a review card the scheduler gave `interval` days, on study
/// day `today`. `load(d)` = cards already due on day `d`.
pub fn plan_interval(
    today: i64,
    interval: u32,
    max: u32,
    exam: Option<ExamDay>,
    options: &PlanOptions,
    load: &dyn Fn(i64) -> u32,
) -> u32 {
    if interval == 0 {
        return 0;
    }
    let (lo, hi) = candidate_range(today, interval, max, exam);
    if lo == hi || !(options.is_active() || exam.is_some()) {
        return if (lo..=hi).contains(&interval) {
            interval
        } else {
            hi
        };
    }
    let mut best: Option<(f64, u32, u32)> = None; // (cost, distance, interval)
    for ivl in lo..=hi {
        let day = today + i64::from(ivl);
        let l = if options.load_balance { load(day) } else { 0 };
        let w = options.easy_days[weekday(day)].weight();
        let cost = (f64::from(l) + 1.0) / w;
        let dist = ivl.abs_diff(interval);
        let better = match best {
            None => true,
            Some((c, d, _)) => cost < c - 1e-12 || ((cost - c).abs() <= 1e-12 && dist < d),
        };
        if better {
            best = Some((cost, dist, ivl));
        }
    }
    best.map_or(interval, |(_, _, ivl)| ivl)
}

// ------------------------------------------------------------------ workload

/// Days until a memory of `stability` days falls to recall `retention`: the inverse of the one
/// forgetting curve ([`fc_core::recall::Curve::days_to`]) with `decay` (callers pass
/// [`fc_core::recall::decay_of_weights`] of the preset; an invalid one = the default decay).
pub fn interval_at(stability: f64, retention: f64, decay: f64) -> Option<f64> {
    fc_core::recall::Curve::new(decay)
        .unwrap_or_default()
        .days_to(stability, retention)
}

/// Estimated reviews per day of review cards with `stabilities` at `retention`: each card
/// comes back every [`interval_at`] days (at least one), plus `new_per_day` new cards with
/// `learning_reviews` answers each. A steady-state estimate (docs/02 §5 "iş yükü tahmini").
pub fn daily_workload(
    stabilities: &[f64],
    retention: f64,
    decay: f64,
    new_per_day: u32,
    learning_reviews: f64,
) -> f64 {
    let reviews: f64 = stabilities
        .iter()
        .filter_map(|s| interval_at(*s, retention, decay))
        .map(|i| 1.0 / i.max(1.0))
        .sum();
    reviews + f64::from(new_per_day) * learning_reviews
}

#[cfg(test)]
mod tests;
