//! FSRS through the reference implementation (`fsrs-rs`).
//!
//! **Model version: FSRS-6** (21 weights). `fsrs-rs` 7 also ships FSRS-7, whose memory has a
//! second ("fast") stability trace; `cards` stores one stability, so FSRS-7 needs a schema
//! addition first (docs/08 K10). FSRS-6 is exact with the stored state.
//!
//! Custom weights: `SchedulerParams.extra = {"weights": [21 numbers]}` (personal optimization
//! arrives with v0.6).

use super::{apply, check_params, elapsed_days, info, Candidate, FSRS_ID, RATINGS};
use fc_core::card::{CardState, Rating, SchedulingState};
use fc_core::Timestamp;
use fc_providers::strategy::{
    ReviewOutcome, Scheduler, SchedulerParams, StrategyError, StrategyResult,
};
use fc_providers::ProviderInfo;
use fsrs::{MemoryState, FSRS};

/// FSRS-6 default weights (`fsrs::FSRS6_DEFAULT_PARAMETERS`).
pub const FSRS6_DEFAULT_WEIGHTS: [f32; 21] = [
    0.212, 1.2931, 2.3065, 8.2956, 6.4133, 0.8334, 3.0194, 0.001, 1.8722, 0.1666, 0.796, 1.4835,
    0.0614, 0.2629, 1.6483, 0.6014, 1.8729, 0.5425, 0.0912, 0.0658, 0.1542,
];

/// Probability of recall after `elapsed_days` for a memory of `stability` days (FSRS-6
/// forgetting curve with the default decay). `None` for a non-positive/unknown stability.
pub fn retrievability(stability: f64, elapsed_days: f64) -> Option<f64> {
    if !(stability.is_finite() && stability > 0.0) {
        return None;
    }
    let decay = -f64::from(FSRS6_DEFAULT_WEIGHTS[20]);
    let factor = 0.9f64.powf(1.0 / decay) - 1.0;
    Some((1.0 + factor * elapsed_days.max(0.0) / stability).powf(decay))
}

/// The FSRS scheduler (default).
#[derive(Debug, Clone, Copy, Default)]
pub struct FsrsScheduler;

fn weights(params: &SchedulerParams) -> StrategyResult<Vec<f32>> {
    let Some(raw) = params.extra.get("weights") else {
        return Ok(FSRS6_DEFAULT_WEIGHTS.to_vec());
    };
    let list = raw
        .as_array()
        .ok_or_else(|| StrategyError("weights must be an array of numbers".into()))?;
    let w: Vec<f32> = list
        .iter()
        .map(|v| v.as_f64().filter(|x| x.is_finite()).map(|x| x as f32))
        .collect::<Option<_>>()
        .ok_or_else(|| StrategyError("weights must be finite numbers".into()))?;
    if w.len() != FSRS6_DEFAULT_WEIGHTS.len() {
        return Err(StrategyError(format!(
            "FSRS-6 needs {} weights, got {}",
            FSRS6_DEFAULT_WEIGHTS.len(),
            w.len()
        )));
    }
    Ok(w)
}

/// Memory of a card as FSRS sees it (`None` = first review).
fn memory(state: &SchedulingState) -> Option<MemoryState> {
    if state.state == CardState::New {
        return None;
    }
    match (state.stability, state.difficulty) {
        (Some(s), Some(d)) if s.is_finite() && d.is_finite() && s > 0.0 => Some(MemoryState {
            stability: s as f32,
            difficulty: d as f32,
            stability_fast: s as f32,
        }),
        _ => None,
    }
}

impl FsrsScheduler {
    fn candidates(
        &self,
        state: &SchedulingState,
        now: Timestamp,
        params: &SchedulerParams,
    ) -> StrategyResult<[Candidate; 4]> {
        check_params(params)?;
        let model = FSRS::new(&weights(params)?).map_err(|e| StrategyError(e.to_string()))?;
        let next = model
            .next_states(
                memory(state),
                params.desired_retention as f32,
                elapsed_days(state, now),
            )
            .map_err(|e| StrategyError(e.to_string()))?;
        let c = |s: &fsrs::ItemState| Candidate {
            stability: f64::from(s.memory.stability),
            difficulty: f64::from(s.memory.difficulty),
            interval_days: f64::from(s.interval),
        };
        Ok([c(&next.again), c(&next.hard), c(&next.good), c(&next.easy)])
    }
}

impl Scheduler for FsrsScheduler {
    fn info(&self) -> ProviderInfo {
        info(FSRS_ID)
    }

    fn schedule(
        &self,
        state: &SchedulingState,
        rating: Rating,
        now: Timestamp,
        params: &SchedulerParams,
    ) -> StrategyResult<ReviewOutcome> {
        let candidates = self.candidates(state, now, params)?;
        Ok(apply(state, rating, now, params, &candidates))
    }

    fn preview(
        &self,
        state: &SchedulingState,
        now: Timestamp,
        params: &SchedulerParams,
    ) -> StrategyResult<[ReviewOutcome; 4]> {
        let candidates = self.candidates(state, now, params)?;
        Ok(RATINGS.map(|r| apply(state, r, now, params, &candidates)))
    }
}
