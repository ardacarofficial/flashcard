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

// ---------------------------------------------------------------- v1.4 W4: media origin

pub use fc_core::content::media::MediaOrigin;

/// Where one stored media file came from (also the request of the internal
/// `media.origin.record`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(optional_fields = nullable)]
pub struct MediaOriginDto {
    /// Media hash.
    pub hash: String,
    /// Origin (`user` when nothing was recorded).
    pub origin: MediaOrigin,
    /// TTS: provider id.
    #[serde(default)]
    pub provider: Option<String>,
    /// TTS: voice id (`null` = the language's default voice).
    #[serde(default)]
    pub voice: Option<String>,
    /// TTS: language.
    #[serde(default)]
    pub language: Option<String>,
    /// TTS: speed (1.0 = normal).
    #[serde(default)]
    pub rate: Option<f32>,
    /// TTS: SHA-256 of the spoken text (whitespace collapsed).
    #[serde(default)]
    pub text_hash: Option<String>,
    /// Clip: the video file name it was cut from.
    #[serde(default)]
    pub source: Option<String>,
}

/// `media.origins`: the origins of `hashes` and/or of every media file a note references.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct ListMediaOrigins {
    /// Media hashes.
    #[serde(default)]
    pub hashes: Vec<String>,
    /// A note: every `media:` reference in its fields.
    #[serde(default)]
    pub note_id: Option<fc_core::NoteId>,
}

/// Response of `media.origins`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct MediaOriginList {
    /// One per existing media file, by hash (unknown hashes are left out).
    pub origins: Vec<MediaOriginDto>,
}

// ---------------------------------------------------------------- v1.4 W4: fill missing audio

/// Which notes `media.audio.fill` looks at (exactly one of the two).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct AudioFillScope {
    /// A search query (the browse/search language; `""` = every note).
    #[serde(default)]
    pub query: Option<String>,
    /// These notes.
    #[serde(default)]
    pub note_ids: Option<Vec<fc_core::NoteId>>,
}

/// A local video file to cut missing lines from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct AudioFillVideo {
    /// Absolute path of the video (or audio) file.
    pub path: String,
    /// Only encounters of this source (video id or title, exact, case-insensitive); `null` =
    /// every encounter in the scope that has times (one video for the whole scope).
    #[serde(default)]
    pub source: Option<String>,
}

/// What TTS reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum AudioFillText {
    /// The term (dictionary form).
    #[default]
    Term,
    /// The example sentence (the term when the note has none).
    Sentence,
}

/// `media.audio.fill` / `media.audio.fill_preview`.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct FillAudio {
    /// Notes to look at.
    pub scope: AudioFillScope,
    /// Re-cut from this local video when the encounter has start–end times.
    #[serde(default)]
    pub video: Option<AudioFillVideo>,
    /// Shift of the cut in ms (a release whose timing differs; `null` = the
    /// `audio_fill.clip_offset_ms` setting).
    #[serde(default)]
    pub offset_ms: Option<i64>,
    /// Use TTS when there is neither an original nor a clip (`null` = true).
    #[serde(default)]
    pub tts: Option<bool>,
    /// What TTS reads (`null` = the term).
    #[serde(default)]
    pub tts_text: Option<AudioFillText>,
    /// TTS provider (`null` = the `tts.provider` setting, else the first one).
    #[serde(default)]
    pub provider: Option<String>,
    /// TTS voice (`null` = the language's default).
    #[serde(default)]
    pub voice: Option<String>,
    /// TTS speed (`null` = 1.0; 0.25–4).
    #[serde(default)]
    pub rate: Option<f32>,
}

/// Where a note's sound would come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum AudioFillSource {
    /// The film's own sound, kept on one of its encounters.
    Original,
    /// Cut from the given local video.
    Clip,
    /// Synthesised (or reused from the TTS cache).
    Tts,
    /// Nothing can be used (see the reason).
    None,
}

/// One note of a preview.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AudioFillItem {
    /// Note.
    pub note_id: fc_core::NoteId,
    /// The source that would be used.
    pub source: AudioFillSource,
    /// `none`: why (`no_audio_field`, `no_text`, `tts_off`, `no_tts_provider`,
    /// `no_voice`); `tts`: `cached` when the cache already has it.
    pub reason: Option<String>,
    /// The text TTS would read / the language.
    pub text: Option<String>,
    /// Language (TTS).
    pub language: Option<String>,
}

/// Why TTS cannot be used for a language.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AudioFillTtsProblem {
    /// Language.
    pub language: String,
    /// `no_tts_provider` (none registered) or `no_voice` ("no offline voice for <lang>
    /// installed": the provider lists voices and none speaks it).
    pub reason: String,
    /// Provider asked.
    pub provider: Option<String>,
    /// Notes affected.
    pub notes: u32,
}

/// Response of `media.audio.fill_preview` (and the counts of a finished fill).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AudioFillPreview {
    /// Notes in the scope.
    pub notes: u32,
    /// Notes that already have sound.
    pub with_audio: u32,
    /// Would get / got the film's own sound.
    pub original: u32,
    /// Would be / were cut from the video.
    pub clip: u32,
    /// Would get / got TTS.
    pub tts: u32,
    /// … of which already in the TTS cache (no new synthesis).
    pub tts_cached: u32,
    /// Nothing usable.
    pub none: u32,
    /// Encounters with times whose source does not match the given video (left out).
    pub other_video: u32,
    /// Encounters without start–end times (never cut: nothing is invented).
    pub no_times: u32,
    /// Languages TTS cannot serve.
    pub tts_problems: Vec<AudioFillTtsProblem>,
    /// The first notes, with their source (at most 200).
    pub items: Vec<AudioFillItem>,
}

/// Result of a `media.audio.fill` job.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AudioFillReport {
    /// What was done (`items` = the notes that failed, with the reason).
    pub done: AudioFillPreview,
    /// Notes whose source failed (ffmpeg or the provider said no).
    pub failed: u32,
}

/// Internal `media.audio.attach`: puts a stored sound on a note (and its encounter).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct AttachAudio {
    /// Note.
    pub note_id: fc_core::NoteId,
    /// Media hash (audio).
    pub hash: String,
    /// The Subtitle encounter the sound belongs to (its `audio_hash` is set too).
    #[serde(default)]
    pub capture_id: Option<String>,
}

/// Internal `media.audio.fill_rollback`: reverts the notes an unfinished fill already wrote.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct RollbackAudioFill {
    /// Inverse calls, newest first.
    pub calls: Vec<crate::InvokeRequest>,
}
