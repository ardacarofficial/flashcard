//! v1.0 first-run and everyday-use building blocks (docs/02 §19, §28, §29, §32), all pure:
//!
//! * [`starter`]: the built-in starter decks (data, TR + EN, with classification values).
//! * [`onboarding`]: "what do you study?" → which deck, classification template, note type,
//!   preset and starter decks to set up.
//! * [`reminder`]: when a daily study reminder is due.
//! * [`lock`]: app lock rules (PIN/password shape, idle timeout, retry back-off).
//! * [`profile`]: profile name rules.
//!
//! No I/O, clock or randomness here: the app passes `now`, the local UTC offset and the
//! stored state in.

pub mod lock;
pub mod onboarding;
pub mod profile;
pub mod reminder;
pub mod starter;
