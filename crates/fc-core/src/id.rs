//! Typed identifiers (UUIDv7) and their injected generator.
//!
//! Every entity id is a distinct newtype around [`Uuid`] so a `NoteId` can never be passed where
//! a `DeckId` is expected. All ids serialize as the canonical hyphenated lowercase string.
//!
//! Ids are produced by an [`IdGen`]; the core never reads the clock or an OS RNG itself.

use crate::rng::Rng;
use crate::time::{Clock, Timestamp};
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use std::sync::Arc;
use uuid::Uuid;

/// Produces fresh UUIDs. Implementations must be time-ordered (UUIDv7) so ids sort by creation.
pub trait IdGen: Send {
    /// Returns a new, unique UUID.
    fn next_uuid(&mut self) -> Uuid;
}

/// Builds a UUIDv7 from a millisecond timestamp and 10 random bytes (RFC 9562 §5.7).
pub fn uuid_v7_from_parts(millis: Timestamp, random: &[u8; 10]) -> Uuid {
    let ms = u64::try_from(millis.as_millis()).unwrap_or(0);
    uuid::Builder::from_unix_timestamp_millis(ms, random).into_uuid()
}

/// Standard UUIDv7 generator: time from an injected [`Clock`], randomness from an injected
/// [`Rng`]. Ids generated within the same millisecond stay strictly increasing (the 12-bit
/// `rand_a` field is used as a counter, RFC 9562 method 1).
pub struct V7IdGen<R: Rng> {
    clock: Arc<dyn Clock>,
    rng: R,
    last_ms: i64,
    counter: u16,
}

impl<R: Rng> V7IdGen<R> {
    /// Creates a generator reading time from `clock` and randomness from `rng`.
    pub fn new(clock: Arc<dyn Clock>, rng: R) -> Self {
        Self {
            clock,
            rng,
            last_ms: i64::MIN,
            counter: 0,
        }
    }
}

impl<R: Rng> IdGen for V7IdGen<R> {
    fn next_uuid(&mut self) -> Uuid {
        let mut ms = self.clock.now().as_millis().max(0);
        if ms <= self.last_ms {
            // Same (or earlier) millisecond: keep ordering by bumping the counter.
            ms = self.last_ms;
            if self.counter >= 0x0FFF {
                ms += 1;
                self.counter = (self.rng.next_u64() & 0x03FF) as u16;
            } else {
                self.counter += 1;
            }
        } else {
            // New millisecond: random counter start, leaving headroom for increments.
            self.counter = (self.rng.next_u64() & 0x03FF) as u16;
        }
        self.last_ms = ms;

        let mut random = [0u8; 10];
        self.rng.fill_bytes(&mut random);
        // rand_a (12 bits) = counter; Builder overwrites the version nibble of byte 0.
        random[0] = ((self.counter >> 8) & 0x0F) as u8;
        random[1] = (self.counter & 0xFF) as u8;
        uuid_v7_from_parts(Timestamp(ms), &random)
    }
}

macro_rules! define_id {
    ($(#[$meta:meta])* $name:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        #[cfg_attr(feature = "ts", derive(ts_rs::TS))]
        pub struct $name(pub Uuid);

        impl $name {
            /// Generates a new id.
            pub fn generate(ids: &mut dyn IdGen) -> Self {
                Self(ids.next_uuid())
            }

            /// Wraps an existing UUID.
            pub const fn from_uuid(u: Uuid) -> Self {
                Self(u)
            }

            /// The underlying UUID.
            pub const fn as_uuid(&self) -> &Uuid {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Display::fmt(&self.0.hyphenated(), f)
            }
        }

        impl FromStr for $name {
            type Err = crate::error::CoreError;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Uuid::parse_str(s).map(Self).map_err(|_| crate::error::CoreError::InvalidId {
                    kind: stringify!($name),
                    value: s.to_owned(),
                })
            }
        }
    };
}

define_id!(
    /// Deck id.
    DeckId
);
define_id!(
    /// Preset (deck options template) id.
    PresetId
);
define_id!(
    /// Note type id.
    NoteTypeId
);
define_id!(
    /// Note id.
    NoteId
);
define_id!(
    /// Card id.
    CardId
);
define_id!(
    /// Tag id.
    TagId
);
define_id!(
    /// Attribute (classification) definition id.
    AttributeDefId
);
define_id!(
    /// Attribute option id. Stable across renames/reorders.
    AttributeOptionId
);
define_id!(
    /// Oplog entry id.
    OplogId
);
define_id!(
    /// External MCP server registration id (v0.8).
    McpServerId
);
define_id!(
    /// Groups oplog entries that are undone together.
    BatchId
);
define_id!(
    /// Background job id.
    JobId
);
define_id!(
    /// Review log entry id.
    ReviewLogId
);
define_id!(
    /// Study session id.
    SessionId
);
define_id!(
    /// Saved query (named filter) id.
    SavedQueryId
);
define_id!(
    /// Exam date id (v0.6 exam-date mode).
    ExamId
);
define_id!(
    /// Goal id (v0.6).
    GoalId
);
define_id!(
    /// Configured AI provider id (v0.8).
    AiProviderId
);
define_id!(
    /// Reading-mode text, subtitle or extract id (v0.10).
    ReadingItemId
);
define_id!(
    /// Automation rule id (v0.9).
    AutomationRuleId
);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::SplitMix64;
    use crate::time::FixedClock;

    fn gen(at: i64) -> (Arc<FixedClock>, V7IdGen<SplitMix64>) {
        let clock = Arc::new(FixedClock::new(Timestamp(at)));
        let g = V7IdGen::new(clock.clone(), SplitMix64::new(42));
        (clock, g)
    }

    #[test]
    fn generates_v7_with_embedded_time() {
        let (_, mut g) = gen(1_700_000_000_000);
        let u = g.next_uuid();
        assert_eq!(u.get_version_num(), 7);
        let (secs, nanos) = u.get_timestamp().unwrap().to_unix();
        assert_eq!(
            secs * 1000 + u64::from(nanos / 1_000_000),
            1_700_000_000_000
        );
    }

    #[test]
    fn ids_are_strictly_increasing_within_same_ms() {
        let (clock, mut g) = gen(1_700_000_000_000);
        let mut prev = g.next_uuid();
        for i in 0..10_000 {
            if i % 3_000 == 0 {
                clock.advance(1);
            }
            let next = g.next_uuid();
            assert!(next > prev, "ids must be monotonic");
            prev = next;
        }
    }

    #[test]
    fn deterministic_with_same_seed_and_clock() {
        let (_, mut a) = gen(5);
        let (_, mut b) = gen(5);
        assert_eq!(a.next_uuid(), b.next_uuid());
    }

    #[test]
    fn typed_ids_roundtrip_as_strings() {
        let (_, mut g) = gen(1);
        let id = DeckId::generate(&mut g);
        let s = id.to_string();
        assert_eq!(s.parse::<DeckId>().unwrap(), id);
        assert_eq!(serde_json::to_string(&id).unwrap(), format!("\"{s}\""));
        assert!("nope".parse::<NoteId>().is_err());
    }
}
