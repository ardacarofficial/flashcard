//! Goals (docs/02 §18, v0.6): "learn 500 words by the end of the month", "study 30 minutes a
//! day". Progress and the pace forecast ("at this pace you'll be 3 days late") are pure
//! functions of the review statistics ([`crate::stats`]).

use crate::stats::{iso_date, DayAggregate, Streak};
use serde::{Deserialize, Serialize};

/// What a goal measures.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum GoalKind {
    /// New cards learned (first answers) since the goal was set.
    CardsLearned,
    /// Minutes studied: per day without a deadline, in total with one.
    StudyMinutes,
    /// Streak length in days.
    Streak,
    /// Share of settled (mature) cards in the scope, 0–100.
    DeckCompletion,
}

impl GoalKind {
    /// Storage string (`goals.kind`).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CardsLearned => "cards_learned",
            Self::StudyMinutes => "study_minutes",
            Self::Streak => "streak",
            Self::DeckCompletion => "deck_completion",
        }
    }

    /// From its storage string.
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "cards_learned" => Self::CardsLearned,
            "study_minutes" => Self::StudyMinutes,
            "streak" => Self::Streak,
            "deck_completion" => Self::DeckCompletion,
            _ => return None,
        })
    }

    /// Whether `target` makes sense for the kind (positive, at most 100 for percentages).
    pub fn valid_target(self, target: f64) -> bool {
        target.is_finite()
            && target > 0.0
            && match self {
                Self::DeckCompletion => target <= 100.0,
                _ => target <= 1_000_000.0,
            }
    }
}

/// Where a goal stands.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct GoalProgress {
    /// Current value (cards, minutes, days or percent).
    pub current: f64,
    /// Target value.
    pub target: f64,
    /// `current / target`, 0–100.
    pub percent: f64,
    /// Target reached.
    pub done: bool,
    /// A per-day goal (study minutes without a deadline): `current` is today's value.
    pub daily: bool,
    /// Day the target is reached at the current pace (`None`: done, no pace yet, or not
    /// forecastable).
    pub projected_day: Option<i64>,
    /// `YYYY-MM-DD` of `projected_day`.
    pub projected_date: Option<String>,
    /// Days after the deadline the target is reached at this pace (negative = early; `None`
    /// without deadline or forecast).
    pub days_late: Option<i64>,
}

/// What a goal's progress is computed from (all within the goal's scope).
#[derive(Debug, Clone, Copy)]
pub struct GoalInputs<'a> {
    /// Day aggregates (any days; the goal picks its own window).
    pub days: &'a [DayAggregate],
    /// Current streak.
    pub streak: Streak,
    /// Settled (mature) share of the scope, 0–100.
    pub settled_pct: f64,
}

/// Progress of a goal of `kind` set on day `start` with optional `deadline` day, on `today`.
pub fn goal_progress(
    kind: GoalKind,
    target: f64,
    start: i64,
    deadline: Option<i64>,
    today: i64,
    inputs: GoalInputs<'_>,
) -> GoalProgress {
    let since = |f: &dyn Fn(&DayAggregate) -> f64| -> f64 {
        inputs
            .days
            .iter()
            .filter(|d| d.day >= start && d.day <= today)
            .map(f)
            .sum()
    };
    let elapsed = (today - start + 1).max(1) as f64;
    // Pace forecast for cumulative goals.
    let paced = |current: f64| -> Option<i64> {
        if current >= target {
            return None;
        }
        let rate = current / elapsed;
        (rate > 0.0).then(|| today + ((target - current) / rate).ceil() as i64)
    };
    let (current, daily, projected) = match kind {
        GoalKind::CardsLearned => {
            let c = since(&|d| f64::from(d.new_cards));
            (c, false, paced(c))
        }
        GoalKind::StudyMinutes if deadline.is_none() => {
            let today_ms: u64 = inputs
                .days
                .iter()
                .filter(|d| d.day == today)
                .map(|d| d.time_ms)
                .sum();
            (today_ms as f64 / 60_000.0, true, None)
        }
        GoalKind::StudyMinutes => {
            let c = since(&|d| d.time_ms as f64 / 60_000.0);
            (c, false, paced(c))
        }
        GoalKind::Streak => {
            let c = f64::from(inputs.streak.current);
            let left = (target - c).ceil().max(0.0) as i64;
            let projected = (left > 0)
                .then(|| today + left - i64::from(!inputs.streak.studied_today))
                .map(|d| d.max(today));
            (c, false, projected)
        }
        GoalKind::DeckCompletion => (inputs.settled_pct, false, None),
    };
    let done = current >= target;
    let projected = if done { None } else { projected };
    GoalProgress {
        current,
        target,
        percent: if target > 0.0 {
            (current / target * 100.0).clamp(0.0, 100.0)
        } else {
            0.0
        },
        done,
        daily,
        projected_day: projected,
        projected_date: projected.map(iso_date),
        days_late: match (projected, deadline) {
            (Some(p), Some(d)) if !daily => Some(p - d),
            _ => None,
        },
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
    use super::*;

    fn day(d: i64, new_cards: u32, minutes: u64) -> DayAggregate {
        DayAggregate {
            new_cards,
            time_ms: minutes * 60_000,
            ..DayAggregate::empty(d)
        }
    }

    fn inputs(days: &[DayAggregate]) -> GoalInputs<'_> {
        GoalInputs {
            days,
            streak: Streak {
                current: 4,
                longest: 9,
                studied_today: false,
            },
            settled_pct: 42.0,
        }
    }

    #[test]
    fn cards_goal_forecasts_lateness() {
        // 500 cards in 30 days; 10 days in, 100 learned → 10/day → 40 more days → 20 days late.
        let days: Vec<_> = (0..10).map(|d| day(d, 10, 5)).collect();
        let p = goal_progress(GoalKind::CardsLearned, 500.0, 0, Some(29), 9, inputs(&days));
        assert_eq!(p.current, 100.0);
        assert!((p.percent - 20.0).abs() < 1e-9);
        assert_eq!(p.projected_day, Some(49));
        assert_eq!(p.days_late, Some(20));
        assert!(!p.done);
        // Reviews before the goal was set do not count.
        let p = goal_progress(GoalKind::CardsLearned, 50.0, 5, None, 9, inputs(&days));
        assert_eq!(p.current, 50.0);
        assert!(p.done && p.projected_day.is_none());
    }

    #[test]
    fn daily_minutes_streak_and_completion() {
        let days = vec![day(8, 0, 40), day(9, 0, 12)];
        let p = goal_progress(GoalKind::StudyMinutes, 30.0, 0, None, 9, inputs(&days));
        assert!(p.daily && !p.done && p.current == 12.0);
        let p = goal_progress(GoalKind::StudyMinutes, 30.0, 0, Some(20), 9, inputs(&days));
        assert!(!p.daily && p.done && p.current == 52.0);
        // Streak 4 (not yet today) → 10 needs today + 5 more days.
        let p = goal_progress(GoalKind::Streak, 10.0, 0, Some(12), 9, inputs(&days));
        assert_eq!(p.projected_day, Some(14));
        assert_eq!(p.days_late, Some(2));
        let p = goal_progress(GoalKind::DeckCompletion, 80.0, 0, None, 9, inputs(&days));
        assert_eq!(p.current, 42.0);
        assert!(p.projected_day.is_none());
        assert!(GoalKind::DeckCompletion.valid_target(80.0));
        assert!(!GoalKind::DeckCompletion.valid_target(180.0));
        assert_eq!(
            GoalKind::parse(GoalKind::Streak.as_str()),
            Some(GoalKind::Streak)
        );
    }
}
