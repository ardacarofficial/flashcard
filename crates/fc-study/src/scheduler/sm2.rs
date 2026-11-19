//! SM-2 fallback (Anki's variant of SuperMemo 2).
//!
//! Stored state reuses the generic columns: `difficulty` holds the **ease factor** (2.5 at
//! start, never below 1.3) and `stability` the **current interval** in days.
//!
//! | Rating | Interval (review) | Ease |
//! |---|---|---|
//! | Again | 1 day (after relearning) | −0.20 |
//! | Hard | ×1.2 | −0.15 |
//! | Good | ×ease | — |
//! | Easy | ×ease ×1.3 | +0.15 |
//!
//! Graduating from learning: Good → 1 day, Easy → 4 days.

use super::{apply, check_params, info, Candidate, RATINGS, SM2_ID};
use fc_core::card::{CardState, Rating, SchedulingState};
use fc_core::time::MILLIS_PER_DAY;
use fc_core::Timestamp;
use fc_providers::strategy::{ReviewOutcome, Scheduler, SchedulerParams, StrategyResult};
use fc_providers::ProviderInfo;

/// Starting ease.
pub const INITIAL_EASE: f64 = 2.5;
/// Lowest ease.
pub const MIN_EASE: f64 = 1.3;
/// Interval multiplier of "Easy" on top of the ease.
pub const EASY_BONUS: f64 = 1.3;
/// Interval multiplier of "Hard".
pub const HARD_FACTOR: f64 = 1.2;

/// The SM-2 scheduler.
#[derive(Debug, Clone, Copy, Default)]
pub struct Sm2Scheduler;

fn candidates(state: &SchedulingState) -> [Candidate; 4] {
    let ease = state
        .difficulty
        .filter(|e| e.is_finite() && *e >= MIN_EASE)
        .unwrap_or(INITIAL_EASE);
    let c = |interval_days: f64, ease: f64| Candidate {
        stability: interval_days,
        difficulty: ease.max(MIN_EASE),
        interval_days,
    };
    match state.state {
        // Graduating intervals; ease untouched while learning.
        CardState::New | CardState::Learning => {
            [c(1.0, ease), c(1.0, ease), c(1.0, ease), c(4.0, ease)]
        }
        // The lapse already cost ease; relearning restarts at one day.
        CardState::Relearning => [c(1.0, ease), c(1.0, ease), c(1.0, ease), c(2.0, ease)],
        CardState::Review => {
            let ivl = state
                .stability
                .filter(|s| s.is_finite() && *s > 0.0)
                .unwrap_or_else(|| match (state.due, state.last_review) {
                    (Some(d), Some(l)) => {
                        ((d.as_millis() - l.as_millis()) as f64 / MILLIS_PER_DAY as f64).max(1.0)
                    }
                    _ => 1.0,
                });
            [
                c(1.0, ease - 0.2),
                c(ivl * HARD_FACTOR, ease - 0.15),
                c(ivl * ease, ease),
                c(ivl * ease * EASY_BONUS, ease + 0.15),
            ]
        }
    }
}

impl Scheduler for Sm2Scheduler {
    fn info(&self) -> ProviderInfo {
        info(SM2_ID)
    }

    fn schedule(
        &self,
        state: &SchedulingState,
        rating: Rating,
        now: Timestamp,
        params: &SchedulerParams,
    ) -> StrategyResult<ReviewOutcome> {
        check_params(params)?;
        Ok(apply(state, rating, now, params, &candidates(state)))
    }

    fn preview(
        &self,
        state: &SchedulingState,
        now: Timestamp,
        params: &SchedulerParams,
    ) -> StrategyResult<[ReviewOutcome; 4]> {
        check_params(params)?;
        let c = candidates(state);
        Ok(RATINGS.map(|r| apply(state, r, now, params, &c)))
    }
}
