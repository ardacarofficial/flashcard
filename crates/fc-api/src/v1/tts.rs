//! Text-to-speech through registered `TtsProvider`s (built-in or plugin-provided, v0.7).

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A registered TTS provider.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TtsProviderDto {
    /// Provider id (`plugin.<plugin id>.<provider>`).
    pub id: String,
    /// i18n key of the name (plugin providers: a plugin locale key).
    pub name_key: String,
    /// Runs offline.
    pub local: bool,
    /// Supported languages (BCP-47; empty = unspecified).
    pub languages: Vec<String>,
}

/// Response of `tts.providers`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TtsProviderList {
    /// Providers by id.
    pub providers: Vec<TtsProviderDto>,
}

/// `tts.voices`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct ListVoices {
    /// Provider (`null` = the `tts.provider` setting, else the first one).
    #[serde(default)]
    pub provider: Option<String>,
}

/// A voice.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct VoiceDto {
    /// Provider-specific id.
    pub id: String,
    /// Display name.
    pub name: String,
    /// BCP-47 language.
    pub language: String,
}

/// Response of `tts.voices`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct VoiceList {
    /// Provider used.
    pub provider: String,
    /// Voices.
    pub voices: Vec<VoiceDto>,
}

/// `tts.synthesize`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct Synthesize {
    /// Provider (`null` = setting / first).
    #[serde(default)]
    pub provider: Option<String>,
    /// Plain text.
    pub text: String,
    /// BCP-47 language.
    pub language: String,
    /// Voice (`null` = default for the language).
    #[serde(default)]
    pub voice: Option<String>,
    /// Speed (1.0 = normal; 0.25–4).
    #[serde(default)]
    pub rate: Option<f32>,
}

/// Synthesized audio.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AudioDto {
    /// Provider used.
    pub provider: String,
    /// MIME type.
    pub mime: String,
    /// Base64 bytes.
    pub data_b64: String,
}
