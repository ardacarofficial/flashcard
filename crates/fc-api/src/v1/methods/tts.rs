//! Text-to-speech.

use crate::v1::common::Empty;
use crate::v1::tts::*;

methods! {
    // ---- tts (v0.7) ----
    /// Registered TTS providers.
    TtsProviders = "tts.providers", Query, "tts", "tts:read", Public, undo: false, Empty => TtsProviderList;
    /// Voices of a provider.
    TtsVoices = "tts.voices", Query, "tts", "tts:read", Public, undo: false, ListVoices => VoiceList;
    /// Synthesizes speech.
    TtsSynthesize = "tts.synthesize", Query, "tts", "tts:read", Public, undo: false, Synthesize => AudioDto;
}
