//! `optimizer` module (v0.6, docs/02 §5): FSRS **personal optimization** per preset and the
//! **workload estimate** of a desired retention.
//!
//! `optimizer.run` enqueues a background job (v0.5 job runtime). The job reads one snapshot,
//! turns the review history of every card whose deck uses the preset into `fsrs-rs` training
//! items (`fc_study::optimize`), trains on a worker thread while polling the job's cancel
//! token (cancel → `want_abort` → training stops → the preset is untouched), then saves the
//! weights with `presets.update` — the one preset write path, one undo step.

use crate::error::{AppError, AppResult};
use crate::handler::{CommandCtx, CommandOutcome, QueryCtx};
use crate::jobs::{cancelled, enqueue, JobContext, JobHandler};
use crate::module::ModuleDescriptor;
use crate::modules::stats::facts::{calendar, scope_card_rows, Presets};
use crate::registry::Registry;
use crate::support::decks::DeckIndex;
use crate::support::{err_id, summary};
use fc_api::v1::methods::*;
use fc_api::v1::planning::*;
use fc_api::v1::presets::UpdatePreset;
use fc_api::{ApiError, ApiMethod, ErrorCode};
use fc_core::card::CardState;
use fc_core::preset::{PresetConfig, RETENTION_RANGE};
use fc_core::{CardId, DeckId, PresetId};
use fc_storage::Reads;
use fc_study::optimize::{self, HistoryReview, OptimizeError};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

/// Module id.
pub const ID: &str = "optimizer";

/// Job kind.
pub const JOB_KIND: &str = OptimizerRun::NAME;

/// FSRS-6 forgetting-curve decay of the default weights (`w20`).
const DEFAULT_DECAY: f64 = 0.1542;

/// Descriptor (UI: a section of the planning screen and the preset editor).
pub fn descriptor() -> ModuleDescriptor {
    ModuleDescriptor {
        id: ID,
        name_key: "modules.optimizer.name",
        description_key: "modules.optimizer.description",
        enabled_by_default: true,
        can_disable: true,
        settings: vec![],
        ui: vec![],
    }
}

/// Registers the module, its methods and its job.
pub fn register(reg: &mut Registry) {
    reg.module(descriptor());
    reg.command::<OptimizerRun>(run);
    reg.query::<OptimizerWorkload>(workload);
    reg.job(Arc::new(OptimizeJob));
}

// ------------------------------------------------------------------ run

fn require_preset(reads: &dyn Reads, id: PresetId) -> AppResult<fc_core::preset::Preset> {
    reads
        .get_preset(id)?
        .ok_or_else(|| err_id(ErrorCode::NotFoundPreset, format!("preset {id} not found"), id))
}

fn run(req: RunOptimizer, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<OptimizerStarted>> {
    require_preset(ctx.reads(), req.preset_id)?;
    calendar(ctx.reads(), req.tz_offset_mins)?;
    let payload = serde_json::to_value(&req)
        .map_err(|e| AppError::api(ErrorCode::Internal, e.to_string()))?;
    let job = enqueue(ctx, JOB_KIND, payload)?;
    Ok(CommandOutcome {
        response: OptimizerStarted { job },
        inverse: Vec::new(),
        summary: summary::<OptimizerRun>(),
        events: Vec::new(),
    })
}

/// Live decks whose effective preset is `preset`.
fn decks_using(reads: &dyn Reads, preset: PresetId) -> AppResult<Vec<DeckId>> {
    let index = DeckIndex::load(reads)?;
    let mut out = Vec::new();
    for d in index.live() {
        if crate::modules::presets::effective(reads, d.id)?.preset_id == Some(preset) {
            out.push(d.id);
        }
    }
    Ok(out)
}

/// Review histories (study days, oldest first) of the cards of `decks`.
pub(crate) fn histories(
    reads: &dyn Reads,
    decks: &[DeckId],
    cal: fc_study::day::StudyDay,
) -> AppResult<Vec<Vec<HistoryReview>>> {
    let cards = reads.study_cards(&fc_storage::repo::StudyCardFilter {
        deck_ids: Some(decks.to_vec()),
        include_suspended: true,
        ..Default::default()
    })?;
    let ids: Vec<CardId> = cards.iter().map(|c| c.id).collect();
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut by_card: BTreeMap<CardId, Vec<(fc_core::Timestamp, HistoryReview)>> = BTreeMap::new();
    for (_, r) in reads.stats_reviews(0, Some(&ids))? {
        by_card.entry(r.card_id).or_default().push((
            r.created_at,
            HistoryReview {
                rating: r.rating,
                state_before: r.state_before,
                day: cal.index(r.created_at),
            },
        ));
    }
    Ok(by_card
        .into_values()
        .map(|mut v| {
            v.sort_by_key(|(t, _)| *t);
            v.into_iter().map(|(_, h)| h).collect()
        })
        .collect())
}

/// Job `optimizer.run`: payload [`RunOptimizer`], result [`OptimizerResult`].
#[derive(Debug, Clone, Copy, Default)]
pub struct OptimizeJob;

impl JobHandler for OptimizeJob {
    fn kind(&self) -> &'static str {
        JOB_KIND
    }

    fn run(&self, payload: serde_json::Value, ctx: &JobContext<'_>) -> AppResult<serde_json::Value> {
        let req: RunOptimizer = serde_json::from_value(payload)
            .map_err(|e| AppError::api(ErrorCode::ValidationInvalidParams, e.to_string()))?;
        ctx.cancel.check()?;
        ctx.report(None, "optimizer.progress.collecting");
        let (histories, previous) = {
            let snap = ctx.read()?;
            let reads: &dyn Reads = &*snap;
            let preset = require_preset(reads, req.preset_id)?;
            let cal = calendar(reads, req.tz_offset_mins)?;
            let decks = decks_using(reads, req.preset_id)?;
            (histories(reads, &decks, cal)?, preset.config.fsrs_weights)
        };
        let cards = u32::try_from(histories.len()).unwrap_or(u32::MAX);
        let items = optimize::training_items(&histories);
        let n_items = u32::try_from(items.len()).unwrap_or(u32::MAX);
        ctx.cancel.check()?;
        let progress = optimize::new_progress();
        let trained = std::thread::scope(|s| {
            let p = progress.clone();
            let worker = s.spawn(move || optimize::train(items, Some(p)));
            while !worker.is_finished() {
                if ctx.is_cancelled() {
                    if let Ok(mut st) = progress.lock() {
                        st.want_abort = true;
                    }
                }
                let fraction = progress
                    .lock()
                    .ok()
                    .and_then(|st| optimize::progress_fraction(&st));
                ctx.report(fraction, "optimizer.progress.training");
                std::thread::sleep(Duration::from_millis(20));
            }
            worker
                .join()
                .unwrap_or_else(|_| Err(OptimizeError::Failed("optimizer thread panicked".into())))
        });
        let weights = match trained {
            Ok(w) => w,
            Err(OptimizeError::Interrupted) => return Err(cancelled()),
            Err(OptimizeError::NotEnoughData { items, min }) => {
                return Err(AppError::Api(
                    ApiError::new(
                        ErrorCode::ConflictNotEnoughReviews,
                        format!("{items} training items, {min} needed"),
                    )
                    .with_detail("items", items)
                    .with_detail("min", min),
                ))
            }
            Err(e) => return Err(AppError::api(ErrorCode::Internal, e.to_string())),
        };
        // Last chance to cancel before writing.
        ctx.cancel.check()?;
        // Re-read the preset so edits made meanwhile are kept; only the weights change.
        let current = {
            let snap = ctx.read()?;
            require_preset(&*snap, req.preset_id)?
        };
        ctx.invoke::<PresetsUpdate>(&UpdatePreset {
            id: current.id,
            name: current.name,
            config: PresetConfig {
                fsrs_weights: weights.clone(),
                ..current.config
            },
        })?;
        serde_json::to_value(OptimizerResult {
            preset_id: req.preset_id,
            items: n_items,
            cards,
            weights,
            previous,
        })
        .map_err(|e| AppError::api(ErrorCode::Internal, e.to_string()))
    }
}

// ------------------------------------------------------------------ workload

fn workload(req: WorkloadRequest, ctx: &QueryCtx<'_>) -> AppResult<WorkloadEstimate> {
    let reads = ctx.tx;
    let (preset, cards) = match req.deck_id {
        Some(d) => {
            let index = DeckIndex::load(reads)?;
            index.require(d)?;
            let cards = reads.study_cards(&fc_storage::repo::StudyCardFilter {
                deck_ids: Some(index.live_subtree(d)),
                ..Default::default()
            })?;
            (Presets::new(reads).of(d)?.clone(), cards)
        }
        None => (PresetConfig::default(), scope_card_rows(reads, None)?),
    };
    let current = preset.desired_retention;
    let mut retentions = match req.retentions {
        Some(r) => r,
        None => (0..6).map(|i| 0.70 + 0.05 * f64::from(i)).chain([0.99]).collect(),
    };
    retentions.push(current);
    for r in &retentions {
        if !RETENTION_RANGE.contains(r) {
            return Err(crate::support::invalid("retentions must be within 0.70–0.99"));
        }
    }
    retentions.sort_by(f64::total_cmp);
    retentions.dedup_by(|a, b| (*a - *b).abs() < 1e-9);
    let stabilities: Vec<f64> = cards
        .iter()
        .filter(|c| !c.suspended && c.scheduling.state != CardState::New)
        .filter_map(|c| c.scheduling.stability)
        .collect();
    let decay = match preset.fsrs_weights.len() {
        21 => preset.fsrs_weights[20],
        _ => DEFAULT_DECAY,
    };
    let learning = preset.learning_steps_mins.len() as f64 + 1.0;
    let at = |r: f64| {
        fc_study::planning::daily_workload(&stabilities, r, decay, preset.new_per_day, learning)
    };
    let base = at(current);
    Ok(WorkloadEstimate {
        current_retention: current,
        review_cards: u32::try_from(stabilities.len()).unwrap_or(u32::MAX),
        new_per_day: preset.new_per_day,
        points: retentions
            .into_iter()
            .map(|r| {
                let w = at(r);
                WorkloadPoint {
                    retention: r,
                    reviews_per_day: w,
                    relative: if base > 0.0 { w / base } else { 1.0 },
                }
            })
            .collect(),
    })
}
