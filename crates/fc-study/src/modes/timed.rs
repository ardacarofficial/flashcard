//! Timed mode (docs/02 §6.1): every card has a time limit; when it runs out the card counts
//! as "Again" or is skipped (settings). The limit is measured on the **backend's** clock from
//! the moment the card was put on screen ([`CardTimer`], started by the screen when it shows
//! the card); the client only reports. A card whose countdown never started can neither be
//! expired nor answered, so a card nobody saw never runs out.

use fc_core::{CardId, Timestamp};
use serde::{Deserialize, Serialize};

/// Shortest limit, seconds.
pub const MIN_SECONDS: u32 = 3;
/// Longest limit, seconds.
pub const MAX_SECONDS: u32 = 600;
/// Clock tolerance between the client's countdown and the backend, ms: an answer up to this
/// late is still accepted, an expiry up to this early is too.
pub const GRACE_MS: i64 = 1_500;

/// What happens when the time runs out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeoutAction {
    /// The card is answered "Again".
    Again,
    /// The card leaves the session unanswered (no review, schedule unchanged).
    Skip,
}

/// The countdown of one card: when it went on screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CardTimer {
    /// Card on screen.
    pub card_id: CardId,
    /// When its countdown started.
    pub started_at: Timestamp,
}

impl CardTimer {
    /// When the countdown of `card` started, if `timer` is its countdown (`None`: another
    /// card's, or none — `card`'s has not started).
    pub fn of(timer: Option<&CardTimer>, card: CardId) -> Option<Timestamp> {
        timer.filter(|t| t.card_id == card).map(|t| t.started_at)
    }
}

/// Time limit of a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Timed {
    /// Seconds per card.
    pub seconds: u32,
    /// On timeout.
    pub on_timeout: TimeoutAction,
}

impl Timed {
    /// Whether `seconds` is within [`MIN_SECONDS`]–[`MAX_SECONDS`].
    pub const fn valid(&self) -> bool {
        self.seconds >= MIN_SECONDS && self.seconds <= MAX_SECONDS
    }

    /// When the time of a card shown at `shown_at` runs out.
    pub fn deadline(&self, shown_at: Timestamp) -> Timestamp {
        shown_at.plus_millis(i64::from(self.seconds) * 1_000)
    }

    /// An answer at `now` is too late (past the deadline by more than [`GRACE_MS`]).
    pub fn is_late(&self, shown_at: Timestamp, now: Timestamp) -> bool {
        now.as_millis() > self.deadline(shown_at).as_millis() + GRACE_MS
    }

    /// The time has run out at `now` (allowing the client to be [`GRACE_MS`] early).
    pub fn has_expired(&self, shown_at: Timestamp, now: Timestamp) -> bool {
        now.as_millis() + GRACE_MS >= self.deadline(shown_at).as_millis()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deadline_late_and_expired() {
        let t = Timed {
            seconds: 10,
            on_timeout: TimeoutAction::Again,
        };
        assert!(t.valid());
        let shown = Timestamp(1_000_000);
        assert_eq!(t.deadline(shown), Timestamp(1_010_000));
        assert!(!t.has_expired(shown, Timestamp(1_008_000)));
        assert!(t.has_expired(shown, Timestamp(1_008_500)));
        assert!(!t.is_late(shown, Timestamp(1_011_500)));
        assert!(t.is_late(shown, Timestamp(1_011_501)));
        assert!(!Timed { seconds: 2, ..t }.valid());
        assert!(!Timed { seconds: 601, ..t }.valid());
    }

    #[test]
    fn a_countdown_belongs_to_one_card() {
        let a = CardId(uuid::Uuid::from_u128(1));
        let b = CardId(uuid::Uuid::from_u128(2));
        let timer = CardTimer {
            card_id: a,
            started_at: Timestamp(5),
        };
        assert_eq!(CardTimer::of(Some(&timer), a), Some(Timestamp(5)));
        assert_eq!(CardTimer::of(Some(&timer), b), None);
        assert_eq!(CardTimer::of(None, a), None);
    }
}
