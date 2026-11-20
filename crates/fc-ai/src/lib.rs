//! # fc-ai
//!
//! Built-in AI providers (docs/06 §1): adapters that implement
//! [`fc_providers::ai::AiProvider`] over HTTP. They hold **no business logic** — prompts,
//! previews, usage accounting and retries live in `fc-app`'s `ai` module.
//!
//! * [`ProviderKind`] + [`ProviderConfig`] → [`build`]: Anthropic Claude (Messages API, the
//!   first-class default), OpenAI (Chat Completions), any OpenAI-compatible endpoint
//!   (OpenRouter, LM Studio, …) and Ollama (local, native `/api/chat`).
//! * [`secrets`]: where API keys live. [`secrets::SecretStore`] with the OS keychain
//!   (`keychain` feature) and an in-memory store for tests. Keys are [`secrets::Secret`]s:
//!   no `Display`, no `Serialize`, a redacted `Debug`, and provider errors are scrubbed of
//!   them ([`redact`]).
//! * [`sources`]: card-generation sources — text of a PDF page range (lopdf) and the
//!   readable text of a web page (fetched with size/time limits, no scripts run).
//! * [`block_on`]: runs a provider future on the calling thread (the app's job worker).
//! * `mock` (feature, also in this crate's tests): a local HTTP server that records requests,
//!   so every provider is tested without the network. Real-API tests are opt-in via
//!   environment variables (`FC_AI_LIVE_*`, see `tests.rs`).
#![forbid(unsafe_code)]

mod anthropic;
mod executor;
mod http;
mod ollama;
mod openai;
pub mod secrets;
pub mod sources;

#[cfg(any(test, feature = "mock"))]
pub mod mock;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_v2;

pub use executor::block_on;
pub use http::redact;

use fc_providers::ai::AiProvider;
use secrets::Secret;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;

/// Which wire protocol a configured provider speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    /// Anthropic Claude (Messages API).
    Anthropic,
    /// OpenAI (Chat Completions).
    Openai,
    /// Any OpenAI-compatible endpoint (OpenRouter, LM Studio, vLLM, …).
    OpenaiCompatible,
    /// Ollama, local (`/api/chat`).
    Ollama,
}

impl ProviderKind {
    /// Every kind, in display order (Claude first).
    pub const ALL: [ProviderKind; 4] = [
        ProviderKind::Anthropic,
        ProviderKind::Openai,
        ProviderKind::OpenaiCompatible,
        ProviderKind::Ollama,
    ];

    /// Base URL a new provider of this kind starts with (editable).
    pub fn default_base_url(self) -> &'static str {
        match self {
            Self::Anthropic => "https://api.anthropic.com",
            Self::Openai => "https://api.openai.com/v1",
            Self::OpenaiCompatible => "http://localhost:1234/v1",
            Self::Ollama => "http://localhost:11434",
        }
    }

    /// Whether requests need an API key.
    pub fn needs_key(self) -> bool {
        matches!(self, Self::Anthropic | Self::Openai)
    }

    /// Runs on the user's machine by default (nothing leaves the computer).
    pub fn local(self) -> bool {
        matches!(self, Self::Ollama)
    }

    /// Stable provider id reported by [`AiProvider::info`].
    pub fn provider_id(self) -> &'static str {
        match self {
            Self::Anthropic => "builtin.ai.anthropic",
            Self::Openai => "builtin.ai.openai",
            Self::OpenaiCompatible => "builtin.ai.openai_compatible",
            Self::Ollama => "builtin.ai.ollama",
        }
    }
}

/// Resolves media hashes of [`fc_providers::ai::ContentPart::Image`] to bytes (the app reads
/// its media store).
pub trait MediaSource: Send + Sync {
    /// Bytes of the media file with this SHA-256 hash, if present.
    fn media_bytes(&self, hash: &str) -> Option<Vec<u8>>;
}

/// Everything needed to talk to one configured provider.
#[derive(Clone)]
pub struct ProviderConfig {
    /// Wire protocol.
    pub kind: ProviderKind,
    /// Base URL (see [`ProviderKind::default_base_url`]); a trailing `/` is ignored.
    pub base_url: String,
    /// API key from the keychain (`None` for local providers).
    pub api_key: Option<Secret>,
    /// Whole-request timeout.
    pub timeout: Duration,
    /// Image resolver (`None` = images are rejected).
    pub media: Option<Arc<dyn MediaSource>>,
}

impl std::fmt::Debug for ProviderConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderConfig")
            .field("kind", &self.kind)
            .field("base_url", &self.base_url)
            .field("api_key", &self.api_key)
            .field("timeout", &self.timeout)
            .finish_non_exhaustive()
    }
}

/// Builds the provider for `config`.
pub fn build(config: ProviderConfig) -> Arc<dyn AiProvider> {
    match config.kind {
        ProviderKind::Anthropic => Arc::new(anthropic::Anthropic::new(config)),
        ProviderKind::Openai | ProviderKind::OpenaiCompatible => {
            Arc::new(openai::OpenAi::new(config))
        }
        ProviderKind::Ollama => Arc::new(ollama::Ollama::new(config)),
    }
}
