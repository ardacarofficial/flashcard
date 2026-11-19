use super::StrategyResult;
use crate::provider::ProviderInfo;
use fc_core::card::{Rating, SchedulingState};
use fc_core::Timestamp;
use serde::{Deserialize, Serialize};

/// Scheduler parameters, from the deck's preset. Algorithm-specific values (FSRS weights…)
/// live in `extra`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SchedulerParams {
    /// Target probability of recall.
    pub desired_retention: f64,
    /// Upper bound on intervals, days.
    pub maximum_interval_days: u32,
    /// Learning steps, minutes.
    pub learning_steps_mins: Vec<u32>,
    /// Relearning steps, minutes.
    pub relearning_steps_mins: Vec<u32>,
    /// Algorithm-specific parameters.
    pub extra: serde_json::Value,
}

/// Result of answering a card.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReviewOutcome {
    /// New scheduling state (written to `cards`).
    pub state: SchedulingState,
    /// Interval assigned, days (0 for intraday steps).
    pub scheduled_days: u32,
    /// Days since the previous review.
    pub elapsed_days: u32,
}

/// Spaced-repetition algorithm (FSRS default, SM-2 fallback — v0.2). Pure: `schedule(state,
/// rating, now, params) → outcome` (docs/03 §3).
pub trait Scheduler: Send + Sync {
    /// Metadata.
    fn info(&self) -> ProviderInfo;
    /// Applies `rating` at `now`.
    fn schedule(
        &self,
        state: &SchedulingState,
        rating: Rating,
        now: Timestamp,
        params: &SchedulerParams,
    ) -> StrategyResult<ReviewOutcome>;
    /// Outcomes for all four ratings (for showing next intervals on the buttons).
    fn preview(
        &self,
        state: &SchedulingState,
        now: Timestamp,
        params: &SchedulerParams,
    ) -> StrategyResult<[ReviewOutcome; 4]>;
}
