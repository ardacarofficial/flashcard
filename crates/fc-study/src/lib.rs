//! # fc-study
//!
//! Pure study logic (docs/02 §5–7, docs/03 §9–10). Everything here is deterministic given its
//! inputs: time comes in as [`fc_core::Timestamp`] arguments, randomness as a seeded
//! [`fc_core::Rng`]. The app layer (`fc-app`, `study` module) owns persistence and
//! transactions; this crate only computes.
//!
//! * [`scheduler`]: the built-in [`Scheduler`](fc_providers::strategy::Scheduler)
//!   implementations — [`scheduler::FsrsScheduler`] (FSRS-6 through `fsrs-rs`, default) and
//!   [`scheduler::Sm2Scheduler`] (fallback) — sharing one learning-step state machine.
//! * [`answer`]: typed-answer checkers (exact, normalized, tolerant, multi) and the answer
//!   diff.
//! * [`hint`]: hint strategies (first letters, word starts, partial reveal).
//! * [`session`]: the study queue — building it from candidate cards, picking the next card,
//!   re-queueing intraday learning cards.
//! * [`day`]: the study-day boundary (local rollover hour).
//! * [`render`]: a minimal, HTML-escaping `{{field}}` renderer used until the card type
//!   engines of v0.3 take over.
#![forbid(unsafe_code)]

pub mod answer;
pub mod day;
pub mod hint;
pub mod render;
pub mod scheduler;
pub mod session;
