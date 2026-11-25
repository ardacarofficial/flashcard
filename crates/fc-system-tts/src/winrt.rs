//! Windows speech through WinRT `Windows.Media.SpeechSynthesis` (the voices of Settings → Time &
//! language → Speech). Offline, no key. The calls run on a short-lived worker thread: WinRT's
//! blocking `join` must not run on a UI (STA) thread, and the worker joins the process MTA.

use crate::SystemVoice;
use fc_providers::{ProviderError, ProviderErrorKind, ProviderResult};
use windows::core::HSTRING;
use windows::Media::SpeechSynthesis::SpeechSynthesizer;
use windows::Storage::Streams::DataReader;

fn os_error(e: windows::core::Error) -> ProviderError {
    ProviderError::new(
        ProviderErrorKind::Unavailable,
        format!("Windows speech failed: {e}"),
    )
}

/// Runs `f` on a worker thread and waits for it.
fn on_worker<T: Send + 'static>(
    f: impl FnOnce() -> windows::core::Result<T> + Send + 'static,
) -> ProviderResult<T> {
    std::thread::Builder::new()
        .name("fc-system-tts".into())
        .spawn(f)
        .map_err(|e| ProviderError::new(ProviderErrorKind::Failed, e.to_string()))?
        .join()
        .map_err(|_| ProviderError::new(ProviderErrorKind::Failed, "Windows speech crashed"))?
        .map_err(os_error)
}

/// Installed voices, the OS default marked.
pub(crate) fn voices() -> ProviderResult<Vec<SystemVoice>> {
    on_worker(|| {
        let default_id = SpeechSynthesizer::DefaultVoice()
            .and_then(|v| v.Id())
            .map(|id| id.to_string_lossy())
            .ok();
        let mut out = Vec::new();
        for v in &SpeechSynthesizer::AllVoices()? {
            let id = v.Id()?.to_string_lossy();
            out.push(SystemVoice {
                is_default: default_id.as_deref() == Some(id.as_str()),
                id,
                name: v.DisplayName()?.to_string_lossy(),
                language: v.Language()?.to_string_lossy(),
            });
        }
        Ok(out)
    })
}

/// Speaks `text` with the installed voice `voice_id`; the stream WinRT returns is a WAV file.
pub(crate) fn speak(text: &str, voice_id: &str, rate: f64) -> ProviderResult<Vec<u8>> {
    let text = HSTRING::from(text);
    let voice_id = voice_id.to_owned();
    let found = on_worker(move || {
        let Some(voice) = SpeechSynthesizer::AllVoices()?
            .into_iter()
            .find(|v| v.Id().is_ok_and(|id| id.to_string_lossy() == voice_id))
        else {
            return Ok(None);
        };
        let synth = SpeechSynthesizer::new()?;
        synth.SetVoice(&voice)?;
        // Normal speed needs no call (the option is missing before Windows 10 1709).
        if (rate - 1.0).abs() > 0.001 {
            synth.Options()?.SetSpeakingRate(rate)?;
        }
        let stream = synth.SynthesizeTextToStreamAsync(&text)?.join()?;
        let size = u32::try_from(stream.Size()?).unwrap_or(u32::MAX);
        let reader = DataReader::CreateDataReader(&stream.GetInputStreamAt(0)?)?;
        let loaded = reader.LoadAsync(size)?.join()?;
        let mut bytes = vec![0u8; loaded as usize];
        reader.ReadBytes(&mut bytes)?;
        let _ = reader.Close();
        let _ = stream.Close();
        let _ = synth.Close();
        Ok(Some(bytes))
    })?;
    found.ok_or_else(|| {
        ProviderError::new(
            ProviderErrorKind::Unavailable,
            "the chosen Windows voice is no longer installed",
        )
    })
}

#[cfg(test)]
mod tests {
    //! Runs on the Windows CI job (`cargo test -p fc-system-tts`): the real system voice.
    use crate::{Platform, Synthesiser, SystemTts};
    use fc_providers::speech::{SpeechRequest, TtsProvider};
    use std::io::Write as _;

    /// Bypasses the test harness's output capture, so a skipped check is visible in the CI log
    /// (and as a GitHub warning annotation).
    fn say(line: &str) {
        let mut out = std::io::stdout().lock();
        // Own line: the harness may have printed "test … " without a newline.
        let _ = writeln!(out, "\n{line}");
        let _ = out.flush();
    }

    #[test]
    fn windows_voice_speaks_one_english_sentence() {
        let voices = Platform.voices().expect("listing Windows voices");
        let english: Vec<_> = voices
            .iter()
            .filter(|v| v.language.to_ascii_lowercase().starts_with("en"))
            .collect();
        say(&format!(
            "fc-system-tts: {} Windows voice(s) installed, {} English: {:?}",
            voices.len(),
            english.len(),
            english
                .iter()
                .map(|v| (&v.name, &v.language))
                .collect::<Vec<_>>()
        ));
        if english.is_empty() {
            say(
                "::warning title=fc-system-tts::no voice: this Windows machine has no English \
                 voice, so the real speech check was NOT run (install an English voice to run it)",
            );
            return;
        }
        let tts = SystemTts::new(Platform);
        let clip = crate::tests::run(tts.synthesize(SpeechRequest {
            text: "The quick brown fox jumps over the lazy dog.".into(),
            language: "en-US".into(),
            voice: None,
            rate: 1.0,
        }))
        .expect("synthesis with an installed English voice");
        assert_eq!(clip.mime, "audio/wav");
        let info = crate::wav::parse(&clip.bytes).expect("a decodable WAV file");
        say(&format!(
            "fc-system-tts: spoke {} bytes, {:?}, {} ms",
            clip.bytes.len(),
            info,
            info.duration_ms()
        ));
        assert!(info.duration_ms() > 300, "{info:?}");
        // A language this machine surely lacks is a plain "no voice", not a crash.
        let missing = crate::tests::run(tts.synthesize(SpeechRequest {
            text: "x".into(),
            language: "tlh".into(),
            voice: None,
            rate: 1.0,
        }))
        .expect_err("no Klingon voice");
        assert!(
            missing.message.contains("no installed system voice"),
            "{missing:?}"
        );
    }
}
