//! Automation rules (docs/02 §23, v0.9): "**when** (trigger) + **if** (query) + **do**
//! (commands)". This module holds the pure parts: what can start a rule ([`RuleTrigger`]), what
//! the app observed after a commit ([`SignalKind`]), whether a trigger accepts a signal, and the
//! loop guard that keeps rules from triggering each other forever ([`LoopGuard`]), and when a
//! scheduled rule is due ([`schedule_state`]).
//!
//! Everything that touches storage (evaluating the condition query, running the actions as
//! commands, the run log) lives in `fc-app`'s `automation` module.

use crate::card::Rating;
use crate::{AutomationRuleId, Timestamp};
use serde::{Deserialize, Serialize};

/// Shortest schedule interval, minutes.
pub const MIN_SCHEDULE_MINS: u32 = 15;
/// Longest schedule interval, minutes (30 days).
pub const MAX_SCHEDULE_MINS: u32 = 43_200;
/// Most actions in one rule.
pub const MAX_ACTIONS: usize = 10;
/// Longest rule name, characters.
pub const MAX_NAME_CHARS: usize = 100;

/// What starts a rule. JSON: `{"kind": "card_answered", "ratings": ["again"]}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum RuleTrigger {
    /// A note was added (its cards are checked).
    NoteAdded,
    /// A note's content, tags or classifications were edited.
    NoteUpdated,
    /// A card was answered in a study session.
    CardAnswered {
        /// Only these ratings (empty = any).
        #[serde(default)]
        ratings: Vec<Rating>,
    },
    /// A flag was set on a card.
    CardFlagged {
        /// Only these flags, 1–7 (empty = any).
        #[serde(default)]
        flags: Vec<u8>,
    },
    /// Classification values of a card or note changed.
    ClassificationChanged,
    /// An import finished (its added and updated notes are checked).
    Imported,
    /// Every `every_mins` minutes, over the whole collection.
    Schedule {
        /// Interval, [`MIN_SCHEDULE_MINS`]–[`MAX_SCHEDULE_MINS`].
        every_mins: u32,
    },
}

impl RuleTrigger {
    /// Storage / log name of the trigger kind.
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::NoteAdded => "note_added",
            Self::NoteUpdated => "note_updated",
            Self::CardAnswered { .. } => "card_answered",
            Self::CardFlagged { .. } => "card_flagged",
            Self::ClassificationChanged => "classification_changed",
            Self::Imported => "imported",
            Self::Schedule { .. } => "schedule",
        }
    }

    /// Checks the trigger's parameters. `Err` names the offending field.
    pub fn validate(&self) -> Result<(), &'static str> {
        match self {
            Self::CardFlagged { flags } if flags.iter().any(|f| !(1..=7).contains(f)) => {
                Err("flags")
            }
            Self::Schedule { every_mins }
                if !(MIN_SCHEDULE_MINS..=MAX_SCHEDULE_MINS).contains(every_mins) =>
            {
                Err("every_mins")
            }
            _ => Ok(()),
        }
    }

    /// Whether a commit's signal starts a rule with this trigger. Schedules start only from
    /// the scheduler ([`SignalKind::Schedule`]); a manual run ([`SignalKind::Manual`]) starts
    /// any rule (it names the rule explicitly).
    pub fn accepts(&self, signal: &SignalKind) -> bool {
        match (self, signal) {
            (_, SignalKind::Manual) => true,
            (Self::NoteAdded, SignalKind::NoteAdded)
            | (Self::NoteUpdated, SignalKind::NoteUpdated)
            | (Self::ClassificationChanged, SignalKind::ClassificationChanged)
            | (Self::Imported, SignalKind::Imported)
            | (Self::Schedule { .. }, SignalKind::Schedule) => true,
            (Self::CardAnswered { ratings }, SignalKind::CardAnswered(r)) => {
                ratings.is_empty() || ratings.contains(r)
            }
            (Self::CardFlagged { flags }, SignalKind::CardFlagged(f)) => {
                flags.is_empty() || flags.contains(f)
            }
            _ => false,
        }
    }

    /// Whether the scheduler runs this rule (`Some(interval in ms)`).
    pub fn schedule_ms(&self) -> Option<i64> {
        match self {
            Self::Schedule { every_mins } => Some(i64::from(*every_mins) * 60_000),
            _ => None,
        }
    }
}

/// What the scheduler does with a scheduled rule now ([`schedule_state`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduleState {
    /// Run it.
    Due,
    /// Its interval has not passed yet.
    Waiting,
    /// Its last run lies in the future: the wall clock went backwards. Its last run is reset to
    /// now (not run), so the interval counts from now — the rule waits at most one interval
    /// after a clock change instead of until the clock catches up.
    ClockWentBack,
}

/// Monotonic guard of the scheduler: a rule never run is due; one whose interval `every_ms`
/// has passed since `last_run` is due; a `last_run` after `now` means the clock moved back.
pub fn schedule_state(last_run: Option<Timestamp>, now: Timestamp, every_ms: i64) -> ScheduleState {
    match last_run {
        None => ScheduleState::Due,
        Some(last) if last.0 > now.0 => ScheduleState::ClockWentBack,
        Some(last) if now.0 - last.0 >= every_ms => ScheduleState::Due,
        Some(_) => ScheduleState::Waiting,
    }
}

/// What the app observed in a committed change (or the scheduler / a manual run).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SignalKind {
    /// Notes were added.
    NoteAdded,
    /// Notes were edited.
    NoteUpdated,
    /// Cards were answered with this rating.
    CardAnswered(Rating),
    /// This flag (1–7) was set on cards.
    CardFlagged(u8),
    /// Classification values changed.
    ClassificationChanged,
    /// An import finished.
    Imported,
    /// The scheduler found a rule due.
    Schedule,
    /// The user ran a rule by hand.
    Manual,
}

impl SignalKind {
    /// Log name (the trigger kind it corresponds to, or `manual`).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NoteAdded => "note_added",
            Self::NoteUpdated => "note_updated",
            Self::CardAnswered(_) => "card_answered",
            Self::CardFlagged(_) => "card_flagged",
            Self::ClassificationChanged => "classification_changed",
            Self::Imported => "imported",
            Self::Schedule => "schedule",
            Self::Manual => "manual",
        }
    }
}

/// The guard's verdict on running a rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admission {
    /// Run it.
    Run,
    /// The rule is already in the chain that triggered it (its own actions, directly or
    /// through other rules, would start it again).
    Cycle,
    /// The chain is already as deep as allowed.
    DepthLimit,
    /// This event already started as many rule runs as allowed.
    Budget,
}

impl Admission {
    /// Log status of a refusal (`None` for [`Admission::Run`]).
    pub const fn status(self) -> Option<&'static str> {
        match self {
            Self::Run => None,
            Self::Cycle => Some("skipped_cycle"),
            Self::DepthLimit => Some("skipped_depth"),
            Self::Budget => Some("skipped_budget"),
        }
    }
}

/// Loop safety for one originating change (one user action, one import, one scheduler tick):
/// rules run in order and may trigger further rules, but
///
/// * a rule never runs inside a chain it already belongs to (**cycle detection**: a rule's
///   actions triggering itself, directly or via other rules),
/// * a chain is at most `max_depth` rules long (**depth limit**; the originating change is
///   depth 0, a rule it starts runs at depth 0, a rule *that* rule starts at depth 1…),
/// * at most `max_runs` rule runs happen in total (**per-event budget**).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoopGuard {
    max_depth: u32,
    max_runs: u32,
    runs: u32,
}

impl LoopGuard {
    /// A fresh guard (both limits at least 1).
    pub fn new(max_depth: u32, max_runs: u32) -> Self {
        Self {
            max_depth: max_depth.max(1),
            max_runs: max_runs.max(1),
            runs: 0,
        }
    }

    /// Decides whether `rule`, triggered by a change its `chain` of rules made (empty = by the
    /// originating change), runs. A [`Admission::Run`] verdict counts against the budget.
    pub fn admit(&mut self, rule: AutomationRuleId, chain: &[AutomationRuleId]) -> Admission {
        if chain.contains(&rule) {
            return Admission::Cycle;
        }
        if u32::try_from(chain.len()).unwrap_or(u32::MAX) >= self.max_depth {
            return Admission::DepthLimit;
        }
        if self.runs >= self.max_runs {
            return Admission::Budget;
        }
        self.runs += 1;
        Admission::Run
    }

    /// Runs admitted so far.
    pub fn runs(&self) -> u32 {
        self.runs
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(n: u128) -> AutomationRuleId {
        AutomationRuleId(uuid::Uuid::from_u128(n))
    }

    #[test]
    fn triggers_accept_their_signals_only() {
        let again = RuleTrigger::CardAnswered {
            ratings: vec![Rating::Again],
        };
        assert!(again.accepts(&SignalKind::CardAnswered(Rating::Again)));
        assert!(!again.accepts(&SignalKind::CardAnswered(Rating::Good)));
        assert!(!again.accepts(&SignalKind::NoteAdded));
        let any = RuleTrigger::CardAnswered { ratings: vec![] };
        assert!(any.accepts(&SignalKind::CardAnswered(Rating::Easy)));
        let red = RuleTrigger::CardFlagged { flags: vec![1] };
        assert!(red.accepts(&SignalKind::CardFlagged(1)));
        assert!(!red.accepts(&SignalKind::CardFlagged(7)));
        let every = RuleTrigger::Schedule { every_mins: 60 };
        assert!(every.accepts(&SignalKind::Schedule));
        assert!(!RuleTrigger::NoteAdded.accepts(&SignalKind::Schedule));
        // A manual run names the rule: any trigger accepts it.
        assert!(RuleTrigger::Imported.accepts(&SignalKind::Manual));
        assert_eq!(every.schedule_ms(), Some(3_600_000));
        assert_eq!(RuleTrigger::NoteAdded.schedule_ms(), None);
    }

    #[test]
    fn trigger_parameters_are_checked() {
        assert_eq!(
            RuleTrigger::CardFlagged { flags: vec![0] }.validate(),
            Err("flags")
        );
        assert_eq!(
            RuleTrigger::CardFlagged { flags: vec![8] }.validate(),
            Err("flags")
        );
        assert_eq!(
            RuleTrigger::Schedule { every_mins: 5 }.validate(),
            Err("every_mins")
        );
        assert!(RuleTrigger::Schedule { every_mins: 15 }.validate().is_ok());
        assert!(RuleTrigger::NoteAdded.validate().is_ok());
    }

    #[test]
    fn trigger_json_is_tagged() {
        let t: RuleTrigger =
            serde_json::from_str(r#"{"kind":"card_answered","ratings":["again"]}"#).unwrap();
        assert_eq!(
            t,
            RuleTrigger::CardAnswered {
                ratings: vec![Rating::Again]
            }
        );
        let t: RuleTrigger = serde_json::from_str(r#"{"kind":"card_flagged"}"#).unwrap();
        assert_eq!(t, RuleTrigger::CardFlagged { flags: vec![] });
        assert_eq!(
            serde_json::to_string(&RuleTrigger::NoteAdded).unwrap(),
            r#"{"kind":"note_added"}"#
        );
    }

    #[test]
    fn guard_detects_cycles_before_anything_else() {
        let mut g = LoopGuard::new(3, 10);
        assert_eq!(g.admit(id(1), &[]), Admission::Run);
        // Rule 1's own actions would start rule 1 again.
        assert_eq!(g.admit(id(1), &[id(1)]), Admission::Cycle);
        // 1 → 2 → 1.
        assert_eq!(g.admit(id(1), &[id(1), id(2)]), Admission::Cycle);
        assert_eq!(g.runs(), 1);
    }

    #[test]
    fn guard_limits_depth() {
        let mut g = LoopGuard::new(2, 10);
        assert_eq!(g.admit(id(1), &[]), Admission::Run);
        assert_eq!(g.admit(id(2), &[id(1)]), Admission::Run);
        assert_eq!(g.admit(id(3), &[id(1), id(2)]), Admission::DepthLimit);
        assert_eq!(Admission::DepthLimit.status(), Some("skipped_depth"));
    }

    #[test]
    fn schedule_survives_the_clock_going_back() {
        let hour = 3_600_000;
        let t = |ms: i64| Timestamp(ms);
        assert_eq!(schedule_state(None, t(0), hour), ScheduleState::Due);
        assert_eq!(
            schedule_state(Some(t(0)), t(hour - 1), hour),
            ScheduleState::Waiting
        );
        assert_eq!(
            schedule_state(Some(t(0)), t(hour), hour),
            ScheduleState::Due
        );
        // Last run "in the future": the clock moved back a day.
        assert_eq!(
            schedule_state(Some(t(25 * hour)), t(hour), hour),
            ScheduleState::ClockWentBack
        );
    }

    #[test]
    fn guard_limits_runs_per_event() {
        let mut g = LoopGuard::new(5, 2);
        assert_eq!(g.admit(id(1), &[]), Admission::Run);
        assert_eq!(g.admit(id(2), &[]), Admission::Run);
        assert_eq!(g.admit(id(3), &[]), Admission::Budget);
        // Limits are at least 1.
        let mut g = LoopGuard::new(0, 0);
        assert_eq!(g.admit(id(1), &[]), Admission::Run);
        assert_eq!(g.admit(id(2), &[]), Admission::Budget);
    }
}
