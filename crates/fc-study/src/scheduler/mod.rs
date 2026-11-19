//! Built-in schedulers. Both implement [`Scheduler`] and share one state machine for the
//! learning/relearning steps ([`apply`]); they differ only in their **memory model** — how a
//! rating changes stability/difficulty and which interval each button gets.
//!
//! State machine (Anki-compatible, docs/02 §5):
//!
//! | From | Again | Hard | Good | Easy |
//! |---|---|---|---|---|
//! | new / learning | first step | repeat step | next step, graduate after the last | graduate |
//! | review | lapse → first relearning step (or review if none) | review | review | review |
//! | relearning | first step | repeat step | next step, graduate after the last | graduate |
//!
//! The current step of a (re)learning card is not stored; it is recovered from the length of
//! the step it is waiting on (`due - last_review`), so `cards` needs no extra column.
//! Graduation and review intervals are whole days, capped by `maximum_interval_days`; review
//! intervals keep `hard < good < easy` (as far as the cap allows). No fuzz: the result depends only on the inputs (load balancing comes
//! with v0.6).

mod fsrs;
mod sm2;
#[cfg(test)]
mod tests;

pub use self::fsrs::{retrievability, FsrsScheduler, FSRS6_DEFAULT_WEIGHTS};
pub use self::sm2::Sm2Scheduler;

use fc_core::card::{CardState, Rating, SchedulingState};
use fc_core::time::MILLIS_PER_DAY;
use fc_core::Timestamp;
use fc_providers::strategy::{ReviewOutcome, Scheduler, SchedulerParams, StrategyError};
use fc_providers::ProviderInfo;

/// Id of the FSRS scheduler (default).
pub const FSRS_ID: &str = "fsrs";
/// Id of the SM-2 scheduler (fallback).
pub const SM2_ID: &str = "sm2";

/// Default upper bound on intervals, days (100 years).
pub const DEFAULT_MAXIMUM_INTERVAL_DAYS: u32 = 36_500;

/// The built-in scheduler with this id (`fsrs`, `sm2`).
pub fn builtin(id: &str) -> Option<&'static dyn Scheduler> {
    static FSRS: FsrsScheduler = FsrsScheduler;
    static SM2: Sm2Scheduler = Sm2Scheduler;
    match id {
        FSRS_ID => Some(&FSRS),
        SM2_ID => Some(&SM2),
        _ => None,
    }
}

/// Ids of the built-in schedulers, default first.
pub const BUILTIN_IDS: [&str; 2] = [FSRS_ID, SM2_ID];

fn info(id: &str) -> ProviderInfo {
    ProviderInfo {
        id: id.to_owned(),
        name_key: format!("study.scheduler.{id}"),
        capabilities: Vec::new(),
        settings: Vec::new(),
        secret_keys: Vec::new(),
        local: true,
    }
}

/// Memory after a rating plus the interval (days, fractional) the model proposes for it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Candidate {
    pub stability: f64,
    pub difficulty: f64,
    pub interval_days: f64,
}

/// Index of a rating in `[again, hard, good, easy]`.
pub(crate) const fn rating_index(rating: Rating) -> usize {
    match rating {
        Rating::Again => 0,
        Rating::Hard => 1,
        Rating::Good => 2,
        Rating::Easy => 3,
    }
}

/// The four ratings in button order.
pub const RATINGS: [Rating; 4] = [Rating::Again, Rating::Hard, Rating::Good, Rating::Easy];

/// Whole days between `last_review` and `now` (0 for a never-reviewed card).
pub fn elapsed_days(state: &SchedulingState, now: Timestamp) -> u32 {
    state.last_review.map_or(0, |last| {
        let days = (now.as_millis() - last.as_millis()).max(0) / MILLIS_PER_DAY;
        u32::try_from(days).unwrap_or(u32::MAX)
    })
}

fn invalid(msg: impl Into<String>) -> StrategyError {
    StrategyError(msg.into())
}

/// Validates the generic parameters.
pub(crate) fn check_params(params: &SchedulerParams) -> Result<(), StrategyError> {
    let r = params.desired_retention;
    if !(r.is_finite() && r > 0.0 && r < 1.0) {
        return Err(invalid(format!(
            "desired_retention must be in (0, 1), got {r}"
        )));
    }
    if params.learning_steps_mins.contains(&0) || params.relearning_steps_mins.contains(&0) {
        return Err(invalid("learning steps must be at least 1 minute"));
    }
    Ok(())
}

/// Rounds the four proposed intervals to whole days in `1..=max`; for review cards
/// (`ordered`) also `hard < good < easy` as far as the cap allows.
fn day_intervals(c: &[Candidate; 4], max: u32, ordered: bool) -> [u32; 4] {
    let max = max.max(1);
    let round = |d: f64| -> u32 {
        if d.is_finite() {
            // In range after the clamp, so the cast is exact.
            d.round().clamp(1.0, f64::from(max)) as u32
        } else {
            max
        }
    };
    let again = round(c[0].interval_days);
    let mut hard = round(c[1].interval_days);
    let mut good = round(c[2].interval_days);
    let mut easy = round(c[3].interval_days);
    if ordered {
        hard = hard.min(good);
        good = good.max(hard.saturating_add(1)).min(max);
        easy = easy.max(good.saturating_add(1)).min(max);
    }
    [again, hard, good, easy]
}

/// Step the card is waiting on, recovered from `due - last_review` (largest step not longer
/// than that delay; 0 when unknown).
fn current_step(state: &SchedulingState, steps: &[u32]) -> usize {
    let (Some(due), Some(last)) = (state.due, state.last_review) else {
        return 0;
    };
    // One second of slack for rounding.
    let delay = due.as_millis() - last.as_millis() + 1_000;
    steps
        .iter()
        .rposition(|s| i64::from(*s) * 60_000 <= delay)
        .unwrap_or(0)
}

/// Delay of "Hard" while on step `i`: the average of the first two steps on the first step,
/// 1.5× (at most +1 day) with a single step, else the current step again.
fn hard_delay_mins(steps: &[u32], i: usize) -> u32 {
    match (steps.get(i), steps.get(1)) {
        (Some(&s0), Some(&s1)) if i == 0 => s0.saturating_add(s1) / 2,
        (Some(&s0), None) if steps.len() == 1 => {
            s0.saturating_add(s0 / 2).min(s0.saturating_add(24 * 60))
        }
        (Some(&s), _) => s,
        (None, _) => steps.first().copied().unwrap_or(1),
    }
}

/// The shared state machine: applies `rating` given the memory model's four candidates.
pub(crate) fn apply(
    state: &SchedulingState,
    rating: Rating,
    now: Timestamp,
    params: &SchedulerParams,
    candidates: &[Candidate; 4],
) -> ReviewOutcome {
    let days = day_intervals(
        candidates,
        params.maximum_interval_days,
        state.state == CardState::Review,
    );
    let i = rating_index(rating);
    let chosen = candidates[i];
    let mut next = SchedulingState {
        stability: Some(chosen.stability),
        difficulty: Some(chosen.difficulty),
        last_review: Some(now),
        reps: if rating == Rating::Again {
            state.reps
        } else {
            state.reps.saturating_add(1)
        },
        ..state.clone()
    };
    // Intraday step: `minutes` from now, scheduled_days 0.
    let step = |next: &mut SchedulingState, learning: CardState, minutes: u32| {
        next.state = learning;
        next.due = Some(now.plus_millis(i64::from(minutes) * 60_000));
        0
    };
    let graduate = |next: &mut SchedulingState, d: u32| {
        next.state = CardState::Review;
        next.due = Some(now.plus_days(i64::from(d)));
        d
    };
    let learn = |next: &mut SchedulingState, steps: &[u32], learning: CardState, cur: usize| {
        if steps.is_empty() {
            return graduate(next, days[i]);
        }
        match rating {
            Rating::Again => step(next, learning, steps[0]),
            Rating::Hard => step(next, learning, hard_delay_mins(steps, cur)),
            Rating::Good => match steps.get(cur + 1) {
                Some(&m) => step(next, learning, m),
                None => graduate(next, days[i]),
            },
            Rating::Easy => graduate(next, days[i]),
        }
    };
    let scheduled_days = match state.state {
        CardState::New => learn(
            &mut next,
            &params.learning_steps_mins,
            CardState::Learning,
            0,
        ),
        CardState::Learning => {
            let cur = current_step(state, &params.learning_steps_mins);
            learn(
                &mut next,
                &params.learning_steps_mins,
                CardState::Learning,
                cur,
            )
        }
        CardState::Relearning => {
            let cur = current_step(state, &params.relearning_steps_mins);
            learn(
                &mut next,
                &params.relearning_steps_mins,
                CardState::Relearning,
                cur,
            )
        }
        CardState::Review => match rating {
            Rating::Again => {
                next.lapses = state.lapses.saturating_add(1);
                match params.relearning_steps_mins.first() {
                    Some(&m) => step(&mut next, CardState::Relearning, m),
                    None => graduate(&mut next, days[0]),
                }
            }
            _ => graduate(&mut next, days[i]),
        },
    };
    ReviewOutcome {
        state: next,
        scheduled_days,
        elapsed_days: elapsed_days(state, now),
    }
}

/// Default parameters (the default preset's values).
pub fn default_params() -> SchedulerParams {
    let preset = fc_core::preset::PresetConfig::default();
    SchedulerParams {
        desired_retention: preset.desired_retention,
        maximum_interval_days: DEFAULT_MAXIMUM_INTERVAL_DAYS,
        learning_steps_mins: preset.learning_steps_mins,
        relearning_steps_mins: preset.relearning_steps_mins,
        extra: serde_json::Value::Null,
    }
}
