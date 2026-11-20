//! Real-world implementations of the core's injected abstractions. This is the only place that
//! reads the system clock or OS randomness, and the one place that pins the process
//! environment third-party crates read ([`pin_process_env`]).

use fc_core::rng::{Rng, SplitMix64};
use fc_core::{Clock, Timestamp};
use std::time::{SystemTime, UNIX_EPOCH};

/// Environment variables dependencies read at run time, removed at process start so behavior
/// never depends on the user's shell:
///
/// * `FSRS_NO_OUTLIER` — `fsrs-rs` (`training.rs`) skips its outlier filter when the variable
///   is merely **set**, whatever its value. Unset = the filter on, as in Anki: the optimizer
///   (`optimizer.run`, `fc_study::optimize`) gives the same weights on every machine.
pub const PINNED_ENV_VARS: &[&str] = &["FSRS_NO_OUTLIER"];

/// Removes [`PINNED_ENV_VARS`] from the process environment. Transports call it first thing
/// in `main` (CLI) / `run` (desktop), **before any thread starts**: changing the environment
/// while other threads read it is not thread-safe.
pub fn pin_process_env() {
    for var in PINNED_ENV_VARS {
        std::env::remove_var(var);
    }
}

/// Wall clock (UTC).
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Timestamp {
        let ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX));
        Timestamp(ms)
    }
}

/// OS-seeded randomness (a SplitMix64 stream seeded from the OS RNG; fine for ids and
/// shuffling, not for cryptography).
#[derive(Debug, Clone)]
pub struct OsRng(SplitMix64);

impl OsRng {
    /// Seeds from the OS; falls back to the clock if the OS RNG is unavailable.
    pub fn new() -> Self {
        let seed = getrandom::u64().unwrap_or_else(|_| SystemClock.now().as_millis() as u64);
        Self(SplitMix64::new(seed))
    }
}

impl Default for OsRng {
    fn default() -> Self {
        Self::new()
    }
}

impl Rng for OsRng {
    fn next_u64(&mut self) -> u64 {
        self.0.next_u64()
    }
}
