//! FSRS personal optimization (docs/02 §5 "Kişisel optimizasyon", v0.6): turns review
//! histories into `fsrs-rs` training items and fits FSRS-6 weights to them.
//!
//! Pure and deterministic: training uses `fsrs-rs`'s seeded shuffles, no clock or I/O. One
//! outside input exists: `fsrs-rs` turns its outlier filter off when the `FSRS_NO_OUTLIER`
//! environment variable is set; the transports remove it at process start
//! (`fc_app::system::pin_process_env`), so the filter is always on.
//! Cancellation is cooperative through the shared [`Progress`] (`want_abort`); the app's
//! background job sets it when the job is cancelled.

use fc_core::card::{CardState, Rating};
use fsrs::{ComputeParametersInput, ComputeParametersVersion, FSRSItem, FSRSReview};
use std::sync::{Arc, Mutex};

pub use fsrs::CombinedProgressState as Progress;

/// Fewer training items than this: the optimizer would only return (near) defaults, so the
/// app reports "not enough data" instead of saving them.
pub const MIN_TRAINING_ITEMS: usize = 64;

/// One answer in a card's history, oldest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HistoryReview {
    /// Button.
    pub rating: Rating,
    /// State before the answer.
    pub state_before: CardState,
    /// Study-day index of the answer.
    pub day: i64,
}

/// Why training stopped without weights.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OptimizeError {
    /// Cancelled through [`Progress::want_abort`].
    Interrupted,
    /// Not enough reviews to learn from.
    NotEnoughData {
        /// Items found.
        items: usize,
        /// Items needed.
        min: usize,
    },
    /// The optimizer failed.
    Failed(String),
}

impl std::fmt::Display for OptimizeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Interrupted => f.write_str("optimization interrupted"),
            Self::NotEnoughData { items, min } => {
                write!(
                    f,
                    "not enough reviews: {items} training items, {min} needed"
                )
            }
            Self::Failed(e) => write!(f, "optimizer failed: {e}"),
        }
    }
}

impl std::error::Error for OptimizeError {}

fn rating_number(r: Rating) -> u32 {
    match r {
        Rating::Again => 1,
        Rating::Hard => 2,
        Rating::Good => 3,
        Rating::Easy => 4,
    }
}

/// Training items of one card's history: one item per answer that has at least one earlier
/// long-term (≥ 1 day) step, each the history up to that answer. A history restarts where the
/// card was new again (reset).
pub fn card_items(history: &[HistoryReview]) -> Vec<FSRSItem> {
    let mut items = Vec::new();
    let mut reviews: Vec<FSRSReview> = Vec::new();
    let mut last_day: Option<i64> = None;
    for r in history {
        if r.state_before == CardState::New {
            reviews.clear();
            last_day = None;
        }
        let delta = last_day.map_or(0, |d| (r.day - d).max(0));
        reviews.push(FSRSReview {
            rating: rating_number(r.rating),
            // Small whole numbers of days: exact in f32.
            delta_t: delta as f32,
        });
        last_day = Some(r.day);
        let item = FSRSItem {
            reviews: reviews.clone(),
        };
        if item.long_term_review_cnt() > 0 {
            items.push(item);
        }
    }
    items
}

/// Training items of many cards.
pub fn training_items(histories: &[Vec<HistoryReview>]) -> Vec<FSRSItem> {
    histories.iter().flat_map(|h| card_items(h)).collect()
}

/// A fresh shared progress state (pass to [`train`], poll from another thread).
pub fn new_progress() -> Arc<Mutex<Progress>> {
    Progress::new_shared()
}

/// Fits FSRS-6 weights (21 numbers) to `items`. Fails with [`OptimizeError::NotEnoughData`]
/// below [`MIN_TRAINING_ITEMS`] and with [`OptimizeError::Interrupted`] once `progress` has
/// `want_abort` set.
pub fn train(
    items: Vec<FSRSItem>,
    progress: Option<Arc<Mutex<Progress>>>,
) -> Result<Vec<f64>, OptimizeError> {
    train_with(items, progress, || {})
}

/// [`train`], calling `started` once the inputs are checked and training is about to run
/// (tests abort there, deterministically "while running").
fn train_with(
    items: Vec<FSRSItem>,
    progress: Option<Arc<Mutex<Progress>>>,
    started: impl FnOnce(),
) -> Result<Vec<f64>, OptimizeError> {
    if items.len() < MIN_TRAINING_ITEMS {
        return Err(OptimizeError::NotEnoughData {
            items: items.len(),
            min: MIN_TRAINING_ITEMS,
        });
    }
    let aborted = |p: &Option<Arc<Mutex<Progress>>>| {
        p.as_ref()
            .is_some_and(|p| p.lock().map(|s| s.want_abort).unwrap_or(true))
    };
    if aborted(&progress) {
        return Err(OptimizeError::Interrupted);
    }
    started();
    let result = fsrs::compute_parameters(ComputeParametersInput {
        train_set: items,
        progress: progress.clone(),
        model_version: ComputeParametersVersion::Fsrs6,
        ..Default::default()
    });
    match result {
        Ok(w) => {
            if aborted(&progress) {
                return Err(OptimizeError::Interrupted);
            }
            let w: Vec<f64> = w.into_iter().map(f64::from).collect();
            if w.len() == crate::scheduler::FSRS6_DEFAULT_WEIGHTS.len()
                && w.iter().all(|x| x.is_finite())
            {
                Ok(w)
            } else {
                Err(OptimizeError::Failed(format!("unexpected weights: {w:?}")))
            }
        }
        Err(fsrs::FSRSError::Interrupted) => Err(OptimizeError::Interrupted),
        Err(e) => Err(OptimizeError::Failed(e.to_string())),
    }
}

/// Training progress 0–1 (`None` before training reported anything).
pub fn progress_fraction(p: &Progress) -> Option<f32> {
    let total = p.total();
    (total > 0).then(|| (p.current() as f32 / total as f32).clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;
    use fc_core::rng::{Rng, SplitMix64};

    fn h(rating: Rating, state: CardState, day: i64) -> HistoryReview {
        HistoryReview {
            rating,
            state_before: state,
            day,
        }
    }

    /// Synthetic histories: recall depends on the gap, like a real memory would.
    pub(crate) fn synthetic(cards: usize, seed: u64) -> Vec<Vec<HistoryReview>> {
        let mut rng = SplitMix64::new(seed);
        (0..cards)
            .map(|_| {
                let mut day = 0i64;
                let mut gap = 1i64;
                let mut out = vec![h(Rating::Good, CardState::New, 0)];
                for _ in 0..8 {
                    day += gap;
                    let p = 0.97f64.powf(gap as f64 / 3.0);
                    let roll = (rng.next_u64() >> 11) as f64 / (1u64 << 53) as f64;
                    if roll < p {
                        out.push(h(Rating::Good, CardState::Review, day));
                        gap = (gap as f64 * 2.5).ceil() as i64;
                    } else {
                        out.push(h(Rating::Again, CardState::Review, day));
                        gap = 1;
                    }
                }
                out
            })
            .collect()
    }

    #[test]
    fn items_follow_the_history() {
        let items = card_items(&[
            h(Rating::Good, CardState::New, 10),
            h(Rating::Good, CardState::Learning, 10),
            h(Rating::Good, CardState::Review, 13),
            h(Rating::Again, CardState::Review, 20),
            // Reset: history starts over.
            h(Rating::Good, CardState::New, 30),
            h(Rating::Good, CardState::Review, 32),
        ]);
        assert_eq!(items.len(), 3);
        let deltas: Vec<f32> = items[1].reviews.iter().map(|r| r.delta_t).collect();
        assert_eq!(deltas, vec![0.0, 0.0, 3.0, 7.0]);
        assert_eq!(items[2].reviews.len(), 2);
        assert_eq!(items[2].reviews[1].rating, 3);
    }

    #[test]
    fn too_little_data_is_reported() {
        let items = training_items(&synthetic(3, 1));
        assert!(matches!(
            train(items, None),
            Err(OptimizeError::NotEnoughData { .. })
        ));
    }

    #[test]
    fn training_fits_weights_and_is_deterministic() {
        let items = training_items(&synthetic(150, 7));
        assert!(items.len() >= MIN_TRAINING_ITEMS);
        let a = train(items.clone(), Some(new_progress())).unwrap();
        let b = train(items, None).unwrap();
        assert_eq!(a.len(), 21);
        assert_eq!(a, b, "seeded: same data, same weights");
        let defaults: Vec<f64> = crate::scheduler::FSRS6_DEFAULT_WEIGHTS
            .iter()
            .map(|w| f64::from(*w))
            .collect();
        assert_ne!(a, defaults, "learned from the data");
    }

    #[test]
    fn abort_flag_interrupts_training() {
        let items = training_items(&synthetic(150, 7));
        let p = new_progress();
        p.lock().unwrap().want_abort = true;
        assert_eq!(
            train(items.clone(), Some(p)),
            Err(OptimizeError::Interrupted)
        );
        // Aborted while running (from another thread, as the app's job does): the request
        // lands after the up-front check, so training itself has to notice it. Set from a
        // thread joined before training starts — no race with how fast training is.
        let p = new_progress();
        let aborter = p.clone();
        let started = move || {
            std::thread::spawn(move || aborter.lock().unwrap().want_abort = true)
                .join()
                .unwrap();
        };
        assert_eq!(
            train_with(items, Some(p.clone()), started),
            Err(OptimizeError::Interrupted)
        );
        assert!(p.lock().unwrap().want_abort);
    }
}
