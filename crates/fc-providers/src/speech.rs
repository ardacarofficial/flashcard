//! Text-to-speech and speech-to-text providers.

use crate::provider::{ProviderInfo, ProviderResult};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Encoded audio.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioClip {
    /// MIME type (`audio/mpeg`, `audio/wav`, …).
    pub mime: String,
    /// Bytes.
    pub bytes: Vec<u8>,
}

/// A TTS voice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Voice {
    /// Provider-specific id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// BCP-47 language.
    pub language: String,
}

/// TTS request.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpeechRequest {
    /// Plain text (HTML already stripped).
    pub text: String,
    /// BCP-47 language.
    pub language: String,
    /// Voice id (`None` = default for the language).
    pub voice: Option<String>,
    /// Speed multiplier (1.0 = normal).
    pub rate: f32,
}

/// Text-to-speech (system SAPI/OneCore, Piper, cloud…).
#[async_trait]
pub trait TtsProvider: Send + Sync {
    /// Metadata.
    fn info(&self) -> ProviderInfo;
    /// Voices available.
    async fn voices(&self) -> ProviderResult<Vec<Voice>>;
    /// Synthesizes speech.
    async fn synthesize(&self, request: SpeechRequest) -> ProviderResult<AudioClip>;
}

/// A recognized segment.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TranscriptSegment {
    /// Start, ms.
    pub start_ms: u64,
    /// End, ms.
    pub end_ms: u64,
    /// Text.
    pub text: String,
}

/// STT result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Transcript {
    /// Full text.
    pub text: String,
    /// Detected/used language.
    pub language: Option<String>,
    /// Segments (may be empty).
    pub segments: Vec<TranscriptSegment>,
}

/// Speech-to-text (whisper.cpp, Whisper API…).
#[async_trait]
pub trait SttProvider: Send + Sync {
    /// Metadata.
    fn info(&self) -> ProviderInfo;
    /// Transcribes audio. `language = None` → auto-detect.
    async fn transcribe(
        &self,
        audio: AudioClip,
        language: Option<String>,
    ) -> ProviderResult<Transcript>;
}
