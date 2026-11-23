//! FSRS through the reference implementation (`fsrs-rs`).
//!
//! **Model version: FSRS-6** (21 weights). `fsrs-rs` 7 also ships FSRS-7, whose memory has a
//! second ("fast") stability trace; `cards` stores one stability, so FSRS-7 needs a schema
//! addition first (docs/08 K10). FSRS-6 is exact with the stored state.
//!
//! Custom weights: `SchedulerParams.extra = {"weights": [numbers]}` — 17 (FSRS-4.5), 19
//! (FSRS-5) or 21; every list goes through [`fc_core::preset::schedulable_fsrs_weights`] (the
//! reference upgrade to FSRS-6, then the model's ranges), so a preset that could be saved
//! always schedules (v1.4.1).

use super::{apply, check_params, elapsed_days, info, Candidate, FSRS_ID, RATINGS};
use crate::day::StudyDay;
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

/// Probability of recall after `elapsed_days` for a memory of `stability` days: the one
/// formula, [`fc_core::recall::retrievability`], with the [`decay`] of `params`. `None` for a
/// non-positive/unknown stability or invalid weights.
pub fn retrievability(stability: f64, elapsed_days: f64, params: &SchedulerParams) -> Option<f64> {
    fc_core::recall::retrievability(stability, elapsed_days, decay(params)?)
}

/// Forgetting-curve decay of `params`' weights (`extra.weights`, else the defaults) by the one
/// rule, [`fc_core::recall::decay_of_weights`]. `None` when `extra.weights` is not a list of
/// numbers.
pub fn decay(params: &SchedulerParams) -> Option<f64> {
    let Some(raw) = params.extra.get("weights") else {
        return Some(fc_core::recall::DEFAULT_DECAY);
    };
    let w: Vec<f64> = raw
        .as_array()?
        .iter()
        .map(serde_json::Value::as_f64)
        .collect::<Option<_>>()?;
    Some(fc_core::recall::decay_of_weights(&w))
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
    let w: Vec<f64> = list
        .iter()
        .map(|v| v.as_f64().filter(|x| x.is_finite()))
        .collect::<Option<_>>()
        .ok_or_else(|| StrategyError("weights must be finite numbers".into()))?;
    let w = fc_core::preset::schedulable_fsrs_weights(&w).ok_or_else(|| {
        StrategyError(format!(
            "FSRS needs 17, 19 or 21 weights, got {}",
            list.len()
        ))
    })?;
    if w.is_empty() {
        return Ok(FSRS6_DEFAULT_WEIGHTS.to_vec());
    }
    #[allow(clippy::cast_possible_truncation)]
    Ok(w.into_iter().map(|x| x as f32).collect())
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
    ) -> StrategyResult<(StudyDay, [Candidate; 4])> {
        let day = check_params(params)?;
        let model = FSRS::new(&weights(params)?).map_err(|e| StrategyError(e.to_string()))?;
        let next = model
            .next_states(
                memory(state),
                params.desired_retention as f32,
                elapsed_days(state, now, day),
            )
            .map_err(|e| StrategyError(e.to_string()))?;
        let c = |s: &fsrs::ItemState| Candidate {
            stability: f64::from(s.memory.stability),
            difficulty: f64::from(s.memory.difficulty),
            interval_days: f64::from(s.interval),
        };
        Ok((
            day,
            [c(&next.again), c(&next.hard), c(&next.good), c(&next.easy)],
        ))
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
        let (day, candidates) = self.candidates(state, now, params)?;
        Ok(apply(state, rating, now, params, day, &candidates))
    }

    fn preview(
        &self,
        state: &SchedulingState,
        now: Timestamp,
        params: &SchedulerParams,
    ) -> StrategyResult<[ReviewOutcome; 4]> {
        let (day, candidates) = self.candidates(state, now, params)?;
        Ok(RATINGS.map(|r| apply(state, r, now, params, day, &candidates)))
    }
}
