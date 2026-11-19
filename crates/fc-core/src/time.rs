//! Time as data. The core never reads the system clock; callers inject a [`Clock`].

use serde::{Deserialize, Serialize};
use std::fmt;

/// A point in time: milliseconds since the Unix epoch, UTC.
///
/// Serialized as a plain JSON number (TypeScript `number`; safe until year 287 396).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct Timestamp(pub i64);

impl Timestamp {
    /// The Unix epoch.
    pub const EPOCH: Timestamp = Timestamp(0);

    /// Creates a timestamp from milliseconds since the Unix epoch.
    pub const fn from_millis(ms: i64) -> Self {
        Self(ms)
    }

    /// Milliseconds since the Unix epoch.
    pub const fn as_millis(self) -> i64 {
        self.0
    }

    /// Returns `self + ms` (saturating).
    pub const fn plus_millis(self, ms: i64) -> Self {
        Self(self.0.saturating_add(ms))
    }

    /// Returns `self + days * 86_400_000` (saturating).
    pub const fn plus_days(self, days: i64) -> Self {
        self.plus_millis(days.saturating_mul(MILLIS_PER_DAY))
    }
}

/// Milliseconds in one (UTC) day.
pub const MILLIS_PER_DAY: i64 = 86_400_000;

impl fmt::Display for Timestamp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}ms", self.0)
    }
}

/// Source of "now". Implemented by the app layer (system clock) and by tests (fixed clock).
pub trait Clock: Send + Sync {
    /// Current time, UTC milliseconds.
    fn now(&self) -> Timestamp;
}

/// A clock that always returns the same instant, optionally advanced by hand. For tests and
/// simulations.
#[derive(Debug)]
pub struct FixedClock {
    now: std::sync::atomic::AtomicI64,
}

impl FixedClock {
    /// Creates a clock frozen at `at`.
    pub fn new(at: Timestamp) -> Self {
        Self {
            now: std::sync::atomic::AtomicI64::new(at.0),
        }
    }

    /// Moves the clock forward by `ms` milliseconds.
    pub fn advance(&self, ms: i64) {
        self.now.fetch_add(ms, std::sync::atomic::Ordering::SeqCst);
    }

    /// Sets the clock to `at`.
    pub fn set(&self, at: Timestamp) {
        self.now.store(at.0, std::sync::atomic::Ordering::SeqCst);
    }
}

impl Clock for FixedClock {
    fn now(&self) -> Timestamp {
        Timestamp(self.now.load(std::sync::atomic::Ordering::SeqCst))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_clock_advances() {
        let c = FixedClock::new(Timestamp(1_000));
        assert_eq!(c.now(), Timestamp(1_000));
        c.advance(500);
        assert_eq!(c.now(), Timestamp(1_500));
        c.set(Timestamp::EPOCH);
        assert_eq!(c.now(), Timestamp::EPOCH);
    }

    #[test]
    fn timestamp_arithmetic_saturates() {
        assert_eq!(Timestamp(0).plus_days(2), Timestamp(2 * MILLIS_PER_DAY));
        assert_eq!(Timestamp(i64::MAX).plus_millis(1), Timestamp(i64::MAX));
        assert_eq!(serde_json::to_string(&Timestamp(42)).unwrap(), "42");
    }
}
