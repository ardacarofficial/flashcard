//! Common provider metadata and errors.

use fc_core::settings::SettingDef;
use serde::{Deserialize, Serialize};

/// Identity, capabilities and settings of a provider or strategy implementation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProviderInfo {
    /// Stable id (`builtin.fsrs`, `plugin.foo.tts`).
    pub id: String,
    /// i18n key of the display name.
    pub name_key: String,
    /// What it can do.
    pub capabilities: Vec<Capability>,
    /// Settings it needs (API endpoint, model, voice…). Secrets are never settings: they live in
    /// the OS keychain under `secret_keys`.
    pub settings: Vec<SettingDef>,
    /// Keychain entry names this provider reads (e.g. `api_key`).
    pub secret_keys: Vec<String>,
    /// Runs fully offline.
    pub local: bool,
}

/// Capability flags; consumers check these before calling optional features.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    /// Token streaming (AI).
    Streaming,
    /// JSON/structured output (AI).
    StructuredOutput,
    /// Image input (AI).
    Vision,
    /// Tool calling (AI).
    Tools,
    /// Supports this BCP-47 language (TTS/STT/translation/dictionary/lemmatizer).
    Language(String),
    /// Word-level timestamps (STT).
    WordTimestamps,
    /// Free-form capability for plugins.
    Other(String),
}

/// Provider failure category; maps to `PROVIDER_*` API errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderErrorKind {
    /// Not configured, offline, or model missing.
    Unavailable,
    /// Credentials missing/rejected.
    Unauthorized,
    /// Rate limit or quota exceeded.
    RateLimited,
    /// The request is invalid for this provider (too long, unsupported language…).
    InvalidRequest,
    /// The operation was cancelled.
    Cancelled,
    /// Anything else.
    Failed,
}

/// Error returned by providers.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("{kind:?}: {message}")]
pub struct ProviderError {
    /// Category.
    pub kind: ProviderErrorKind,
    /// Developer-facing message (never contains secrets).
    pub message: String,
    /// Whether retrying later may succeed.
    pub retryable: bool,
}

impl ProviderError {
    /// Creates an error; `retryable` is derived from the kind.
    pub fn new(kind: ProviderErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            retryable: matches!(
                kind,
                ProviderErrorKind::RateLimited | ProviderErrorKind::Unavailable
            ),
        }
    }
}

/// Result alias.
pub type ProviderResult<T> = Result<T, ProviderError>;
