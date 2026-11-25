//! Offline text-to-speech with the voices the operating system already has (v1.4.2 J4).
//!
//! [`SystemTts`] is a [`TtsProvider`] over a [`Synthesiser`], the thin boundary to the OS. On
//! Windows that is `Windows.Media.SpeechSynthesis` (WinRT; the voices of Settings → Time &
//! language → Speech, OneCore and the installed language packs). Everything above that boundary
//! is here and tested on every platform with a fake synthesiser: which installed voice speaks a
//! note's language, the speaking rate, checking that the bytes are a playable WAV before they
//! reach the media store, and plain errors (no voice for a language → what to install).
//!
//! On other systems [`provider`] is `None` (nothing is registered, so the app answers
//! `no_tts_provider` exactly as before) and [`Platform`] answers "not available on this system".

use async_trait::async_trait;
use fc_providers::speech::{AudioClip, SpeechRequest, TtsProvider, Voice};
use fc_providers::{Capability, ProviderError, ProviderErrorKind, ProviderInfo, ProviderResult};
use std::sync::Arc;

pub mod wav;
#[cfg(windows)]
mod winrt;

/// Provider id (`tts.provider` setting; sorts before `plugin.…`, so it is the default where it
/// exists).
pub const PROVIDER_ID: &str = "builtin.system";

/// i18n key of the provider's name.
pub const NAME_KEY: &str = "tts_providers.system";

/// MIME type of what the system voice returns.
pub const MIME: &str = "audio/wav";

/// Speaking rates the system accepts (WinRT `SpeakingRate`: 0.5–6.0).
pub const RATE_RANGE: (f64, f64) = (0.5, 6.0);

/// An installed voice as the OS reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemVoice {
    /// OS id (stable on one machine).
    pub id: String,
    /// Display name ("Microsoft Zira").
    pub name: String,
    /// BCP-47 language (`en-US`).
    pub language: String,
    /// The voice the user chose as default in the OS settings.
    pub is_default: bool,
}

/// The OS boundary: lists voices, turns text into a WAV file. Blocking.
pub trait Synthesiser: Send + Sync {
    /// Installed voices.
    fn voices(&self) -> ProviderResult<Vec<SystemVoice>>;
    /// Speaks `text` with the voice `voice_id` at `rate` (1.0 = normal, within [`RATE_RANGE`])
    /// and returns the bytes of a WAV file.
    fn speak(&self, text: &str, voice_id: &str, rate: f64) -> ProviderResult<Vec<u8>>;
}

/// The error for "no installed voice speaks `language`", with what to do about it. Its kind is
/// `Unavailable`; the app shows the localized sentence `errors.provider.no_voice`.
pub fn no_voice(language: &str) -> ProviderError {
    ProviderError::new(
        ProviderErrorKind::Unavailable,
        format!(
            "no installed system voice speaks '{language}'; add one in Windows Settings → \
             Time & language → Speech → Add voices (or install the language pack), then try again"
        ),
    )
}

fn normalized(tag: &str) -> String {
    tag.trim().replace('_', "-").to_ascii_lowercase()
}

fn primary(tag: &str) -> String {
    normalized(tag)
        .split('-')
        .next()
        .unwrap_or_default()
        .to_owned()
}

/// Language tags that name no language: the default voice speaks them.
fn no_language(tag: &str) -> bool {
    matches!(primary(tag).as_str(), "" | "und" | "mul" | "zxx")
}

/// The OS default voice among those `pred` accepts, else the first of them.
fn best(voices: &[SystemVoice], pred: impl Fn(&SystemVoice) -> bool) -> Option<&SystemVoice> {
    voices
        .iter()
        .find(|v| v.is_default && pred(v))
        .or_else(|| voices.iter().find(|v| pred(v)))
}

/// Which voice speaks a request: the asked voice (it must be installed); else, for the
/// request's language, a voice of exactly that tag, then one of the same primary language
/// (`en` ↔ `en-GB`), the OS default voice first in both; without a language, the OS default.
pub fn choose_voice<'a>(
    voices: &'a [SystemVoice],
    language: &str,
    wanted: Option<&str>,
) -> ProviderResult<&'a SystemVoice> {
    if let Some(id) = wanted.filter(|w| !w.is_empty()) {
        return voices.iter().find(|v| v.id == id).ok_or_else(|| {
            ProviderError::new(
                ProviderErrorKind::Unavailable,
                format!("the system voice '{id}' is not installed"),
            )
        });
    }
    if no_language(language) {
        return best(voices, |_| true).ok_or_else(|| no_voice(language));
    }
    let tag = normalized(language);
    let lang = primary(language);
    best(voices, |v| normalized(&v.language) == tag)
        .or_else(|| best(voices, |v| primary(&v.language) == lang))
        .ok_or_else(|| no_voice(language))
}

/// The system TTS provider over a [`Synthesiser`].
pub struct SystemTts<S> {
    sys: S,
}

impl<S: Synthesiser> SystemTts<S> {
    /// Wraps a synthesiser.
    pub fn new(sys: S) -> Self {
        Self { sys }
    }

    fn speak_now(&self, request: &SpeechRequest) -> ProviderResult<AudioClip> {
        let text = request.text.trim();
        if text.is_empty() {
            return Err(ProviderError::new(
                ProviderErrorKind::InvalidRequest,
                "there is no text to read",
            ));
        }
        let voices = self.sys.voices()?;
        let voice = choose_voice(&voices, &request.language, request.voice.as_deref())?;
        let rate = if request.rate.is_finite() {
            f64::from(request.rate).clamp(RATE_RANGE.0, RATE_RANGE.1)
        } else {
            1.0
        };
        let bytes = self.sys.speak(text, &voice.id, rate)?;
        wav::parse(&bytes).map_err(|why| {
            ProviderError::new(
                ProviderErrorKind::Failed,
                format!("the system voice returned no playable audio ({why})"),
            )
        })?;
        Ok(AudioClip {
            mime: MIME.to_owned(),
            bytes,
        })
    }
}

#[async_trait]
impl<S: Synthesiser> TtsProvider for SystemTts<S> {
    fn info(&self) -> ProviderInfo {
        let mut languages: Vec<String> = self
            .sys
            .voices()
            .unwrap_or_default()
            .into_iter()
            .map(|v| v.language)
            .collect();
        languages.sort();
        languages.dedup();
        ProviderInfo {
            id: PROVIDER_ID.to_owned(),
            name_key: NAME_KEY.to_owned(),
            capabilities: languages.into_iter().map(Capability::Language).collect(),
            settings: vec![],
            secret_keys: vec![],
            local: true,
        }
    }

    async fn voices(&self) -> ProviderResult<Vec<Voice>> {
        Ok(self
            .sys
            .voices()?
            .into_iter()
            .map(|v| Voice {
                id: v.id,
                name: v.name,
                language: v.language,
            })
            .collect())
    }

    async fn synthesize(&self, request: SpeechRequest) -> ProviderResult<AudioClip> {
        self.speak_now(&request)
    }
}

/// This platform's synthesiser: Windows speech on Windows; elsewhere one that answers "not
/// available on this system" to everything.
#[derive(Debug, Default, Clone, Copy)]
pub struct Platform;

/// Whether this build has system voices at all (Windows).
pub const AVAILABLE: bool = cfg!(windows);

#[cfg(not(windows))]
fn unavailable() -> ProviderError {
    ProviderError::new(
        ProviderErrorKind::Unavailable,
        "system voices are not available on this system (only on Windows)",
    )
}

impl Synthesiser for Platform {
    fn voices(&self) -> ProviderResult<Vec<SystemVoice>> {
        #[cfg(windows)]
        return winrt::voices();
        #[cfg(not(windows))]
        Err(unavailable())
    }

    fn speak(&self, text: &str, voice_id: &str, rate: f64) -> ProviderResult<Vec<u8>> {
        #[cfg(windows)]
        return winrt::speak(text, voice_id, rate);
        #[cfg(not(windows))]
        {
            let _ = (text, voice_id, rate);
            Err(unavailable())
        }
    }
}

/// The system provider to register, or `None` where the platform has none (anything but
/// Windows). The `tts` module's switch and the `tts.provider` setting decide whether it speaks.
pub fn provider() -> Option<Arc<dyn TtsProvider>> {
    AVAILABLE.then(|| Arc::new(SystemTts::new(Platform)) as Arc<dyn TtsProvider>)
}

#[cfg(test)]
mod tests;
