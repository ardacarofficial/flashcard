//! Recall probability ("retrievability", `R`): **the one formula** every surface uses — card
//! info, statistics, the study history panel and the query language (`retrievability<0.9`,
//! in memory and in SQL through the `fc_recall` function `fc-storage` registers).
//!
//! The FSRS-6 forgetting curve `R(t, S) = (1 + F·t/S)^-d` with `F = 0.9^(-1/d) − 1`, so that
//! `R(S, S) = 0.9`. `d` is the decay (FSRS-6 weight `w20`) of the card's preset: personalised
//! weights change it, so callers pass it in ([`DEFAULT_DECAY`] for the default weights).
//!
//! `t` is counted in **whole study days** ([`elapsed_days`]), exactly as the scheduler counts
//! the elapsed time when a card is graded: a card reviewed yesterday evening is one day old
//! all of today. Pure: "today" arrives as the start of the current study day.

use crate::card::{CardState, SchedulingState};
use crate::time::{Timestamp, MILLIS_PER_DAY};

/// Decay of the FSRS-6 default weights (`w20`). Weights are `f32`; this is the widened `f32`
/// value, so a preset with the default weights compares bit-equal.
#[allow(clippy::cast_lossless)]
pub const DEFAULT_DECAY: f64 = 0.1542_f32 as f64;

/// Whether `decay` describes a usable curve (finite and positive).
pub fn valid_decay(decay: f64) -> bool {
    decay.is_finite() && decay > 0.0
}

/// `F` of the curve with `decay` (`R(S, S) = 0.9`).
fn factor(decay: f64) -> f64 {
    0.9f64.powf(-1.0 / decay) - 1.0
}

/// `R` after `elapsed_days` for a memory of `stability` days. `None` for a non-positive or
/// non-finite stability or an invalid decay.
pub fn retrievability(stability: f64, elapsed_days: f64, decay: f64) -> Option<f64> {
    if !(stability.is_finite() && stability > 0.0 && valid_decay(decay)) {
        return None;
    }
    let t = if elapsed_days.is_finite() {
        elapsed_days.max(0.0)
    } else {
        0.0
    };
    Some((1.0 + factor(decay) * t / stability).powf(-decay))
}

/// Whole study days from the study day of `last_review` to the current one, which began at
/// `day_start` (0 for a review made today or later). Days are 24 h long (the study calendar
/// uses a fixed UTC offset), so this equals `StudyDay::days_between(last_review, now)`.
pub fn elapsed_days(last_review: Timestamp, day_start: Timestamp) -> u32 {
    if last_review >= day_start {
        return 0;
    }
    let gap = day_start.0.saturating_sub(last_review.0);
    let days = gap.saturating_add(MILLIS_PER_DAY - 1) / MILLIS_PER_DAY;
    u32::try_from(days).unwrap_or(u32::MAX)
}

/// `R` of a card reviewed at `last_review` with `stability`, on the study day that began at
/// `day_start`. The building block of [`card_retrievability`] and of `fc_recall` in SQL.
pub fn retrievability_at(
    stability: f64,
    last_review: Timestamp,
    day_start: Timestamp,
    decay: f64,
) -> Option<f64> {
    retrievability(
        stability,
        f64::from(elapsed_days(last_review, day_start)),
        decay,
    )
}

/// `R` of a card now: `None` for a new card or one without a review or a positive stability.
pub fn card_retrievability(s: &SchedulingState, day_start: Timestamp, decay: f64) -> Option<f64> {
    if s.state == CardState::New {
        return None;
    }
    retrievability_at(s.stability?, s.last_review?, day_start, decay)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-12
    }

    #[test]
    fn the_curve_passes_ninety_percent_at_one_stability() {
        for d in [DEFAULT_DECAY, 0.1, 0.5, 0.8] {
            assert!(close(retrievability(7.0, 7.0, d).unwrap(), 0.9));
            assert!(close(retrievability(7.0, 0.0, d).unwrap(), 1.0));
        }
        // FSRS-4.5 (decay 0.5) is the old fixed curve `(1 + 19/81·x)^-0.5`.
        let old = (1.0 + 19.0 / 81.0 * 3.0f64).powf(-0.5);
        assert!((retrievability(2.0, 6.0, 0.5).unwrap() - old).abs() < 1e-12);
        // A larger decay forgets faster after S.
        assert!(retrievability(2.0, 6.0, 0.5).unwrap() < retrievability(2.0, 6.0, 0.1).unwrap());
        assert_eq!(retrievability(0.0, 1.0, 0.5), None);
        assert_eq!(retrievability(f64::NAN, 1.0, 0.5), None);
        assert_eq!(retrievability(1.0, 1.0, 0.0), None);
        assert_eq!(retrievability(1.0, 1.0, f64::NAN), None);
    }

    #[test]
    fn elapsed_days_count_study_day_boundaries() {
        let start = Timestamp(100 * MILLIS_PER_DAY + 3_600_000);
        assert_eq!(elapsed_days(start, start), 0);
        assert_eq!(elapsed_days(start.plus_millis(5), start), 0);
        assert_eq!(elapsed_days(start.plus_millis(-1), start), 1);
        assert_eq!(elapsed_days(start.plus_millis(-MILLIS_PER_DAY), start), 1);
        assert_eq!(
            elapsed_days(start.plus_millis(-MILLIS_PER_DAY - 1), start),
            2
        );
        assert_eq!(elapsed_days(Timestamp(i64::MIN), start), u32::MAX);
    }

    #[test]
    fn new_and_unreviewed_cards_have_no_retrievability() {
        let mut s = SchedulingState::NEW;
        s.stability = Some(3.0);
        s.last_review = Some(Timestamp(0));
        let day = Timestamp(5 * MILLIS_PER_DAY);
        assert_eq!(card_retrievability(&s, day, DEFAULT_DECAY), None);
        s.state = CardState::Review;
        assert!(card_retrievability(&s, day, DEFAULT_DECAY).is_some());
        s.last_review = None;
        assert_eq!(card_retrievability(&s, day, DEFAULT_DECAY), None);
    }
}
