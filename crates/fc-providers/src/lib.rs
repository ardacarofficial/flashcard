//! # fc-providers
//!
//! Pluggable interfaces (docs/03 §10). v0.1 ships **signatures only**; built-in
//! implementations arrive with their features (FSRS v0.2, card types v0.3, AI v0.8, …) and
//! plugins can implement the same traits (v0.7).
//!
//! * [`provider`]: I/O-bound services, `async` and object-safe (`Arc<dyn AiProvider>`):
//!   [`ai::AiProvider`], [`speech::TtsProvider`], [`speech::SttProvider`],
//!   [`language::TranslationProvider`], [`language::DictionaryProvider`],
//!   [`language::Lemmatizer`].
//! * [`strategy`]: pure, synchronous strategies: [`strategy::Scheduler`],
//!   [`strategy::CardTypeEngine`], [`strategy::AnswerChecker`], [`strategy::HintStrategy`],
//!   [`strategy::Importer`], [`strategy::Exporter`].
//!
//! Every implementation has an id, a capability declaration and a settings schema
//! ([`provider::ProviderInfo`]); the active one is chosen in settings.
#![forbid(unsafe_code)]

pub mod ai;
pub mod language;
pub mod provider;
pub mod speech;
pub mod strategy;

pub use provider::{Capability, ProviderError, ProviderErrorKind, ProviderInfo, ProviderResult};

#[cfg(test)]
mod tests {
    use super::*;

    /// All interfaces must stay object-safe: the app stores them as `Arc<dyn Trait>`.
    #[allow(dead_code, clippy::too_many_arguments)]
    fn object_safety(
        _: &dyn ai::AiProvider,
        _: &dyn speech::TtsProvider,
        _: &dyn speech::SttProvider,
        _: &dyn language::TranslationProvider,
        _: &dyn language::DictionaryProvider,
        _: &dyn language::Lemmatizer,
        _: &dyn strategy::Scheduler,
        _: &dyn strategy::CardTypeEngine,
        _: &dyn strategy::AnswerChecker,
        _: &dyn strategy::HintStrategy,
        _: &dyn strategy::Importer,
        _: &dyn strategy::Exporter,
    ) {
    }

    #[test]
    fn retryable_follows_kind() {
        assert!(ProviderError::new(ProviderErrorKind::RateLimited, "slow down").retryable);
        assert!(ProviderError::new(ProviderErrorKind::Unavailable, "offline").retryable);
        assert!(!ProviderError::new(ProviderErrorKind::Unauthorized, "bad key").retryable);
        assert!(!ProviderError::new(ProviderErrorKind::InvalidRequest, "too long").retryable);
    }
}
