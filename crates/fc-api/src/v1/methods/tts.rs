//! Text-to-speech, media origin and filling missing audio.

use crate::v1::common::Empty;
use crate::v1::jobs::JobDto;
use crate::v1::tts::*;

methods! {
    // ---- tts (v0.7) ----
    /// Registered TTS providers.
    TtsProviders = "tts.providers", Query, "tts", "tts:read", Public, undo: false, Empty => TtsProviderList;
    /// Voices of a provider.
    TtsVoices = "tts.voices", Query, "tts", "tts:read", Public, undo: false, ListVoices => VoiceList;
    /// Synthesizes speech.
    TtsSynthesize = "tts.synthesize", Query, "tts", "tts:read", Public, undo: false, Synthesize => AudioDto;
    // ---- media origin + fill missing audio (v1.4 W4) ----
    /// Where stored media files came from (original / clip / tts / user).
    MediaOrigins = "media.origins", Query, "media", "media:read", Public, undo: false, ListMediaOrigins => MediaOriginList;
    /// Records where a stored media file came from (derived metadata, like a note's origin).
    MediaOriginsRecord = "media.origin.record", Command, "media", "media:write", Internal, undo: false, MediaOriginDto => Empty;
    /// What `media.audio.fill` would do: how many notes, which source each would get.
    MediaAudioFillPreview = "media.audio.fill_preview", Query, "audio_fill", "media:read", Public, undo: false, FillAudio => AudioFillPreview;
    /// Fills missing sound (original, else a clip of a local video, else TTS) as a cancellable job; one undo step.
    MediaAudioFill = "media.audio.fill", Command, "audio_fill", "media:write", Public, undo: false, FillAudio => JobDto;
    /// Puts a stored sound on a note (and its encounter).
    MediaAudioAttach = "media.audio.attach", Command, "audio_fill", "media:write", Internal, undo: true, AttachAudio => Empty;
    /// Reverts what an unfinished fill wrote.
    MediaAudioFillRollback = "media.audio.fill_rollback", Command, "audio_fill", "media:write", Internal, undo: true (replay: false), RollbackAudioFill => Empty;
}
