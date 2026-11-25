//! Everything above the system call, with a fake synthesiser (runs on every platform).

use super::*;
use std::future::Future;
use std::sync::Mutex;
use std::task::{Context, Poll, Waker};

/// Drives a provider future (they finish without waiting).
pub(crate) fn run<F: Future>(fut: F) -> F::Output {
    let mut cx = Context::from_waker(Waker::noop());
    let mut fut = std::pin::pin!(fut);
    loop {
        if let Poll::Ready(out) = fut.as_mut().poll(&mut cx) {
            return out;
        }
    }
}

fn voice(id: &str, language: &str, is_default: bool) -> SystemVoice {
    SystemVoice {
        id: id.into(),
        name: format!("Voice {id}"),
        language: language.into(),
        is_default,
    }
}

#[derive(Default)]
struct Fake {
    voices: Vec<SystemVoice>,
    /// What `speak` returns (`None` = a valid WAV).
    output: Option<Vec<u8>>,
    spoken: Mutex<Vec<(String, String, f64)>>,
}

impl Synthesiser for Fake {
    fn voices(&self) -> ProviderResult<Vec<SystemVoice>> {
        Ok(self.voices.clone())
    }

    fn speak(&self, text: &str, voice_id: &str, rate: f64) -> ProviderResult<Vec<u8>> {
        self.spoken
            .lock()
            .unwrap()
            .push((text.into(), voice_id.into(), rate));
        Ok(self
            .output
            .clone()
            .unwrap_or_else(|| wav::pcm16_mono(16_000, &[0, 50, -50, 0])))
    }
}

fn machine() -> Vec<SystemVoice> {
    vec![
        voice("tr", "tr-TR", false),
        voice("gb", "en-GB", false),
        voice("us1", "en-US", false),
        voice("us2", "en-US", true),
        voice("de", "de_DE", false),
    ]
}

fn request(text: &str, language: &str) -> SpeechRequest {
    SpeechRequest {
        text: text.into(),
        language: language.into(),
        voice: None,
        rate: 1.0,
    }
}

#[test]
fn picks_the_voice_of_the_notes_language() {
    let v = machine();
    let pick = |lang: &str| choose_voice(&v, lang, None).map(|v| v.id.as_str());
    // Exact tag, the OS default first among equals.
    assert_eq!(pick("en-US"), Ok("us2"));
    assert_eq!(pick("EN_us"), Ok("us2"));
    assert_eq!(pick("en-GB"), Ok("gb"));
    // Primary language only (the note says `en`, or a region nobody installed).
    assert_eq!(pick("en"), Ok("us2"));
    assert_eq!(pick("en-AU"), Ok("us2"));
    assert_eq!(pick("tr"), Ok("tr"));
    assert_eq!(pick("de-AT"), Ok("de"));
    // No language → the OS default voice.
    assert_eq!(pick(""), Ok("us2"));
    assert_eq!(pick("und"), Ok("us2"));
    // A voice asked by id must be installed.
    assert_eq!(choose_voice(&v, "en", Some("gb")).unwrap().id, "gb");
    let gone = choose_voice(&v, "en", Some("zz")).unwrap_err();
    assert_eq!(gone.kind, ProviderErrorKind::Unavailable);
    // Without any default the first listed voice of the language speaks.
    let plain = vec![voice("a", "en-US", false), voice("b", "en-US", false)];
    assert_eq!(choose_voice(&plain, "en", None).unwrap().id, "a");
    assert_eq!(choose_voice(&plain, "", None).unwrap().id, "a");
}

#[test]
fn no_voice_for_the_language_says_what_to_install() {
    let v = machine();
    for lang in ["ja-JP", "fr"] {
        let e = choose_voice(&v, lang, None).unwrap_err();
        assert_eq!(e.kind, ProviderErrorKind::Unavailable);
        assert!(e.message.contains(lang), "{e:?}");
        assert!(e.message.contains("Speech"), "{e:?}");
        assert!(e.message.contains("language pack"), "{e:?}");
    }
    // No voices at all: even the default request has none.
    assert!(choose_voice(&[], "", None).is_err());
    assert!(choose_voice(&[], "en", None).is_err());
}

#[test]
fn synthesizes_a_wav_with_the_chosen_voice_and_a_clamped_rate() {
    let tts = SystemTts::new(Fake {
        voices: machine(),
        ..Fake::default()
    });
    let clip = run(tts.synthesize(request("  hello  ", "en"))).unwrap();
    assert_eq!(clip.mime, "audio/wav");
    assert!(wav::parse(&clip.bytes).is_ok());
    let mut slow = request("merhaba", "tr-TR");
    slow.rate = 0.25;
    run(tts.synthesize(slow)).unwrap();
    let mut fast = request("hallo", "de");
    fast.rate = f32::NAN;
    run(tts.synthesize(fast)).unwrap();
    let mut asked = request("hi", "tr");
    asked.voice = Some("gb".into());
    run(tts.synthesize(asked)).unwrap();
    assert_eq!(
        *tts.sys.spoken.lock().unwrap(),
        vec![
            ("hello".to_owned(), "us2".to_owned(), 1.0),
            ("merhaba".to_owned(), "tr".to_owned(), 0.5),
            ("hallo".to_owned(), "de".to_owned(), 1.0),
            ("hi".to_owned(), "gb".to_owned(), 1.0),
        ]
    );
}

#[test]
fn refuses_empty_text_missing_voices_and_unplayable_output() {
    let tts = SystemTts::new(Fake {
        voices: machine(),
        ..Fake::default()
    });
    let e = run(tts.synthesize(request("   ", "en"))).unwrap_err();
    assert_eq!(e.kind, ProviderErrorKind::InvalidRequest);
    let e = run(tts.synthesize(request("bonjour", "fr-FR"))).unwrap_err();
    assert!(e
        .message
        .contains("no installed system voice speaks 'fr-FR'"));
    assert!(
        tts.sys.spoken.lock().unwrap().is_empty(),
        "nothing was spoken"
    );

    let broken = SystemTts::new(Fake {
        voices: machine(),
        output: Some(b"RIFF\0\0\0\0WAVE".to_vec()),
        ..Fake::default()
    });
    let e = run(broken.synthesize(request("hello", "en"))).unwrap_err();
    assert_eq!(e.kind, ProviderErrorKind::Failed);
    assert!(e.message.contains("no playable audio"), "{e:?}");
}

#[test]
fn lists_voices_and_their_languages() {
    let tts = SystemTts::new(Fake {
        voices: machine(),
        ..Fake::default()
    });
    let info = tts.info();
    assert_eq!(info.id, PROVIDER_ID);
    assert!(info.local);
    assert_eq!(
        info.capabilities,
        ["de_DE", "en-GB", "en-US", "tr-TR"]
            .map(|l| Capability::Language(l.into()))
            .to_vec()
    );
    let voices = run(tts.voices()).unwrap();
    assert_eq!(voices.len(), 5);
    assert_eq!(
        (
            voices[3].id.as_str(),
            voices[3].name.as_str(),
            voices[3].language.as_str()
        ),
        ("us2", "Voice us2", "en-US")
    );
}

#[test]
fn elsewhere_the_platform_says_not_available() {
    if AVAILABLE {
        return; // Windows: the real voices are tested in `winrt::tests`.
    }
    assert!(provider().is_none(), "nothing to register off Windows");
    let e = Platform.voices().unwrap_err();
    assert_eq!(e.kind, ProviderErrorKind::Unavailable);
    assert!(e.message.contains("not available on this system"));
    let e = Platform.speak("hi", "x", 1.0).unwrap_err();
    assert!(e.message.contains("not available on this system"));
    let tts = SystemTts::new(Platform);
    assert!(tts.info().capabilities.is_empty());
    assert!(run(tts.synthesize(request("hi", "en"))).is_err());
}
