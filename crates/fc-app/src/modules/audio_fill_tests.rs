//! `audio_fill` (v1.4 W4): media origin, filling missing audio with a fake TTS provider, the
//! TTS cache, one undo step, "no voice" reporting and (with a real ffmpeg, when present) a
//! re-cut clip.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::modules::subtitle::{authenticate, capture, key_hash, upload};
use crate::testing::{MemStore, TestApp};
use async_trait::async_trait;
use fc_api::v1::jobs::{JobRef, JobStatus};
use fc_api::v1::modules::SetModuleEnabled;
use fc_api::v1::notes::{NoteDto, NoteRef};
use fc_api::v1::settings::SetSetting;
use fc_api::v1::subtitle::{CreateSubtitleKey, ListSubtitleEncounters, SubtitleKeyDto};
use fc_providers::provider::{ProviderInfo, ProviderResult};
use fc_providers::speech::{AudioClip, SpeechRequest, TtsProvider, Voice};
use fc_providers::{ProviderError, ProviderErrorKind};
use fc_storage::{SqliteStore, Store};
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};

/// A tiny valid WAV (44-byte header + 4 samples).
fn wav(seed: u8) -> Vec<u8> {
    let data = [seed, seed, 0, 0, seed, seed, 0, 0];
    let mut v = Vec::new();
    v.extend_from_slice(b"RIFF");
    v.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    v.extend_from_slice(b"WAVEfmt ");
    v.extend_from_slice(&16u32.to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes()); // PCM
    v.extend_from_slice(&1u16.to_le_bytes()); // mono
    v.extend_from_slice(&8000u32.to_le_bytes());
    v.extend_from_slice(&16000u32.to_le_bytes());
    v.extend_from_slice(&2u16.to_le_bytes());
    v.extend_from_slice(&16u16.to_le_bytes());
    v.extend_from_slice(b"data");
    v.extend_from_slice(&(data.len() as u32).to_le_bytes());
    v.extend_from_slice(&data);
    v
}

/// Fake TTS: a WAV per text; counts syntheses; speaks only `langs`.
struct FakeTts {
    langs: Vec<&'static str>,
    calls: Arc<AtomicUsize>,
}

#[async_trait]
impl TtsProvider for FakeTts {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            id: "test.fake".into(),
            name_key: "test.fake".into(),
            capabilities: vec![],
            settings: vec![],
            secret_keys: vec![],
            local: true,
        }
    }
    async fn voices(&self) -> ProviderResult<Vec<Voice>> {
        Ok(self
            .langs
            .iter()
            .map(|l| Voice {
                id: format!("v-{l}"),
                name: (*l).to_owned(),
                language: (*l).to_owned(),
            })
            .collect())
    }
    async fn synthesize(&self, r: SpeechRequest) -> ProviderResult<AudioClip> {
        if !self.langs.iter().any(|l| primary(l) == primary(&r.language)) {
            return Err(ProviderError::new(ProviderErrorKind::Unavailable, "no voice"));
        }
        let n = self.calls.fetch_add(1, Ordering::SeqCst);
        let seed = u8::try_from(r.text.len() % 200).unwrap() + u8::try_from(n % 50).unwrap();
        Ok(AudioClip {
            mime: "audio/wav".into(),
            bytes: wav(seed),
        })
    }
}

fn app(store: Arc<dyn Store>, langs: Vec<&'static str>) -> (TestApp, Arc<AtomicUsize>, SubtitleKeyDto) {
    let t = TestApp::new(store);
    let calls = Arc::new(AtomicUsize::new(0));
    t.app.registry().register_tts(Arc::new(FakeTts {
        langs,
        calls: calls.clone(),
    }));
    t.ok::<ModulesSetEnabled>(SetModuleEnabled {
        id: crate::modules::subtitle::ID.into(),
        enabled: true,
    });
    let c = t.ok::<SubtitleKeysCreate>(CreateSubtitleKey { name: "Chrome".into() });
    let key = authenticate(&t.app, &key_hash(&c.secret)).unwrap();
    (t, calls, key)
}

fn cid(n: u32) -> String {
    format!("0192812a-6f3c-7a41-9c2e-{n:012x}")
}

/// A capture of `lemma` meaning `meaning`; `times`: the line's start–end.
fn shot(n: u32, lemma: &str, meaning: &str, sentence: &str, times: bool) -> Value {
    json!({
      "schema": 1,
      "capture_id": cid(n),
      "kind": "word",
      "lang": { "target": "en", "native": "tr" },
      "term": { "surface": lemma, "lemma": lemma, "pos": "noun" },
      "definition": { "text": meaning, "source": "dict" },
      "context": { "sentence": sentence, "translation": "çeviri", "term_offset": null },
      "source": {
        "type": "video", "url": "https://www.youtube.com/watch?v=abc", "title": "Show · S1E2",
        "site": "youtube", "video_id": "abc",
        "t_start_ms": if times { json!(1000) } else { Value::Null },
        "t_end_ms": if times { json!(1800) } else { Value::Null }
      },
      "media": {},
      "deck_id": null,
      "tags": [],
      "created_at": "2026-10-12T10:00:00.000Z",
      "client": { "name": "subtitle-extension", "ext_version": "3.1.3" }
    })
}

fn note(t: &TestApp, id: NoteId) -> NoteDto {
    t.ok::<NotesGet>(NoteRef { id })
}

fn audio_of(t: &TestApp, id: NoteId) -> String {
    note(t, id).fields.get("audio").cloned().unwrap_or_default()
}

fn ids(v: &[NoteId]) -> FillAudio {
    FillAudio {
        scope: AudioFillScope {
            query: None,
            note_ids: Some(v.to_vec()),
        },
        ..Default::default()
    }
}

fn run(t: &TestApp, req: FillAudio) -> AudioFillReport {
    let j = t.ok::<MediaAudioFill>(req);
    t.app.run_pending_jobs().unwrap();
    let j = t.ok::<JobsGet>(JobRef { id: j.id });
    assert_eq!(j.status, JobStatus::Succeeded, "{j:?}");
    serde_json::from_value(j.result.unwrap()).unwrap()
}

fn origin(t: &TestApp, hash: &str) -> MediaOriginDto {
    t.ok::<MediaOrigins>(ListMediaOrigins {
        hashes: vec![hash.to_owned()],
        note_id: None,
    })
    .origins
    .remove(0)
}

fn hash_of(field: &str) -> String {
    field.strip_prefix("media:").unwrap().to_owned()
}

fn the_film_sound_is_original_and_missing_sound_is_filled_once_per_word(store: Arc<dyn Store>) {
    let (t, calls, key) = app(store, vec!["en-US"]);
    // A: the film's sound came with the capture.
    let up = upload(&t.app, &key, &cid(1), "audio", "audio/wav", &wav(1)).unwrap();
    let mut a = shot(1, "key", "anahtar", "Where are my keys?", true);
    a["media"] = json!({"audio": {"upload_id": up.upload_id, "mime": "audio/wav"}});
    let a = capture(&t.app, &key, &a).unwrap().note_id;
    let a_hash = hash_of(&audio_of(&t, a));
    let enc = t
        .ok::<SubtitleEncountersList>(ListSubtitleEncounters {
            note_id: Some(a),
            ..Default::default()
        })
        .encounters;
    assert_eq!(enc[0].facts.audio_origin, Some(MediaOrigin::Original));
    assert_eq!(origin(&t, &a_hash).origin, MediaOrigin::Original);
    let by_note = t.ok::<MediaOrigins>(ListMediaOrigins {
        hashes: vec![],
        note_id: Some(a),
    });
    assert_eq!(by_note.origins.len(), 1);
    // The user cleared the card's sound: the encounter still has it → original comes back.
    let n = note(&t, a);
    let mut fields = n.fields.clone();
    fields.insert("audio".into(), String::new());
    t.ok::<NotesUpdate>(UpdateNote {
        id: a,
        deck_id: None,
        fields,
        tags: n.tags.iter().map(|t| t.as_str().to_owned()).collect(),
        attributes: vec![],
    });
    // B, C: the same word with two meanings (two notes), no sound → TTS once.
    let b = capture(&t.app, &key, &shot(2, "bank", "banka", "To the bank.", false))
        .unwrap()
        .note_id;
    let c = capture(&t.app, &key, &shot(3, "bank", "kıyı", "The river bank.", false))
        .unwrap()
        .note_id;
    assert_ne!(b, c);
    let all = [a, b, c];

    let p = t.ok::<MediaAudioFillPreview>(ids(&all));
    assert_eq!((p.notes, p.with_audio, p.original, p.clip, p.tts, p.none), (3, 0, 1, 0, 2, 0));
    assert_eq!(p.tts_cached, 0);
    assert!(p.tts_problems.is_empty());
    assert_eq!(p.items[1].text.as_deref(), Some("bank"));
    assert_eq!(p.items[1].language.as_deref(), Some("en"));
    // The preview wrote nothing.
    assert_eq!(audio_of(&t, b), "");

    let r = run(&t, ids(&all));
    assert_eq!((r.done.original, r.done.tts, r.failed), (1, 2, 0));
    assert_eq!(calls.load(Ordering::SeqCst), 1, "the same word is synthesised once");
    assert_eq!(hash_of(&audio_of(&t, a)), a_hash);
    let (hb, hc) = (audio_of(&t, b), audio_of(&t, c));
    assert!(hb.starts_with("media:"));
    assert_eq!(hb, hc);
    let o = origin(&t, &hash_of(&hb));
    assert_eq!(o.origin, MediaOrigin::Tts);
    assert_eq!(o.provider.as_deref(), Some("test.fake"));
    assert_eq!(o.language.as_deref(), Some("en"));
    assert_eq!(o.rate, Some(1.0));
    assert_eq!(o.text_hash, Some(tts_text_hash("bank")));
    // The encounter carries it too, with its origin; a primary switch would keep it.
    let enc = t
        .ok::<SubtitleEncountersList>(ListSubtitleEncounters {
            note_id: Some(b),
            ..Default::default()
        })
        .encounters;
    assert_eq!(enc[0].facts.audio_origin, Some(MediaOrigin::Tts));
    assert_eq!(enc[0].facts.example_fields.get("audio"), Some(&hb));
    // `ses:tts` finds them, `ses:özgün` the film's.
    let q = |s: &str| {
        let p = t.ok::<MediaAudioFillPreview>(FillAudio {
            scope: AudioFillScope {
                query: Some(s.into()),
                note_ids: None,
            },
            ..Default::default()
        });
        p.notes
    };
    assert_eq!(q("ses:tts"), 2);
    assert_eq!(q("ses:özgün"), 1);

    // One undo step reverts the whole fill.
    t.ok::<OplogUndo>(Empty {});
    for n in all {
        assert_eq!(audio_of(&t, n), "", "{n}");
    }
    let enc = t
        .ok::<SubtitleEncountersList>(ListSubtitleEncounters {
            note_id: Some(b),
            ..Default::default()
        })
        .encounters;
    assert_eq!(enc[0].facts.audio_hash, None);
    // Redo puts it back; filling again uses the cache (no new synthesis).
    t.ok::<OplogRedo>(Empty {});
    assert_eq!(audio_of(&t, b), hb);
    t.ok::<OplogUndo>(Empty {});
    let p = t.ok::<MediaAudioFillPreview>(ids(&all));
    assert_eq!((p.tts, p.tts_cached), (2, 0), "undo forgot the unreferenced TTS file");
    let r = run(&t, ids(&all));
    assert_eq!(r.done.tts, 2);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    let p = t.ok::<MediaAudioFillPreview>(ids(&all));
    assert_eq!((p.with_audio, p.tts), (3, 0));
}

fn a_language_without_a_voice_is_reported_not_failed(store: Arc<dyn Store>) {
    let (t, calls, key) = app(store, vec!["de-DE"]);
    let b = capture(&t.app, &key, &shot(2, "bank", "banka", "To the bank.", false))
        .unwrap()
        .note_id;
    let p = t.ok::<MediaAudioFillPreview>(ids(&[b]));
    assert_eq!((p.tts, p.none), (0, 1));
    assert_eq!(p.tts_problems.len(), 1);
    let pr = &p.tts_problems[0];
    assert_eq!((pr.language.as_str(), pr.reason.as_str(), pr.notes), ("en", "no_voice", 1));
    assert_eq!(pr.provider.as_deref(), Some("test.fake"));
    assert_eq!(p.items[0].reason.as_deref(), Some("no_voice"));
    let r = run(&t, ids(&[b]));
    assert_eq!((r.done.tts, r.done.none, r.failed), (0, 1, 0));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    // No provider at all.
    t.app.registry().unregister_tts("test.");
    let p = t.ok::<MediaAudioFillPreview>(ids(&[b]));
    assert_eq!(p.tts_problems[0].reason, "no_tts_provider");
    // TTS off: nothing usable, said so.
    let p = t.ok::<MediaAudioFillPreview>(FillAudio {
        tts: Some(false),
        ..ids(&[b])
    });
    assert_eq!(p.items[0].reason.as_deref(), Some("tts_off"));
}

fn a_clip_is_never_invented_without_times(store: Arc<dyn Store>) {
    let (t, _, key) = app(store, vec![]);
    let with = capture(&t.app, &key, &shot(1, "key", "anahtar", "My keys.", true))
        .unwrap()
        .note_id;
    let without = capture(&t.app, &key, &shot(2, "bank", "banka", "The bank.", false))
        .unwrap()
        .note_id;
    // Absolute on every platform (`/videos/…` is not absolute on Windows); the preview
    // never opens the file.
    let video = std::env::temp_dir().join("videos").join("show-s1e2.mkv");
    let req = |source: Option<&str>| FillAudio {
        video: Some(AudioFillVideo {
            path: video.display().to_string(),
            source: source.map(str::to_owned),
        }),
        tts: Some(false),
        ..ids(&[with, without])
    };
    let p = t.ok::<MediaAudioFillPreview>(req(None));
    assert_eq!((p.clip, p.no_times, p.none), (1, 1, 1));
    let p = t.ok::<MediaAudioFillPreview>(req(Some("ABC")));
    assert_eq!(p.clip, 1, "the video id matches, case-insensitively");
    let p = t.ok::<MediaAudioFillPreview>(req(Some("Another Show")));
    assert_eq!((p.clip, p.other_video), (0, 1));
    // Offsets shift the cut: setting, then the request.
    assert_eq!(clip_window(1000, 1800, 0), Some((750, 2050)));
    assert_eq!(clip_window(1000, 1800, -2000), Some((0, 250)));
    assert_eq!(clip_window(1000, 1800, 500), Some((1250, 2550)));
    assert_eq!(clip_window(1800, 1000, 0), None);
    let e = t.err::<MediaAudioFillPreview>(FillAudio {
        video: Some(AudioFillVideo {
            path: "relative.mkv".into(),
            source: None,
        }),
        ..ids(&[with])
    });
    assert_eq!(e.code, ErrorCode::ValidationInvalidParams);
}

#[test]
fn mem_the_film_sound_is_original_and_missing_sound_is_filled_once_per_word() {
    the_film_sound_is_original_and_missing_sound_is_filled_once_per_word(Arc::new(MemStore::new()));
}

#[test]
fn sqlite_the_film_sound_is_original_and_missing_sound_is_filled_once_per_word() {
    the_film_sound_is_original_and_missing_sound_is_filled_once_per_word(Arc::new(
        SqliteStore::open_in_memory().unwrap(),
    ));
}

#[test]
fn sqlite_a_language_without_a_voice_is_reported_not_failed() {
    a_language_without_a_voice_is_reported_not_failed(Arc::new(SqliteStore::open_in_memory().unwrap()));
}

#[test]
fn mem_a_language_without_a_voice_is_reported_not_failed() {
    a_language_without_a_voice_is_reported_not_failed(Arc::new(MemStore::new()));
}

#[test]
fn sqlite_a_clip_is_never_invented_without_times() {
    a_clip_is_never_invented_without_times(Arc::new(SqliteStore::open_in_memory().unwrap()));
}

/// With the real ffmpeg (skipped cleanly when it is not installed): a line is re-cut from a
/// local file, stored as `clip` with the file name as its source.
#[test]
fn a_real_ffmpeg_recuts_the_line_from_a_local_video() {
    let ok = std::process::Command::new("ffmpeg")
        .arg("-version")
        .output()
        .is_ok_and(|o| o.status.success());
    if !ok {
        eprintln!("ffmpeg not installed: skipped");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let video = dir.path().join("show-s1e2.wav");
    let made = std::process::Command::new("ffmpeg")
        .args(["-nostdin", "-loglevel", "error", "-y", "-f", "lavfi", "-i"])
        .arg("sine=frequency=440:duration=3")
        .arg(&video)
        .status()
        .unwrap();
    assert!(made.success());
    let (t, _, key) = app(Arc::new(SqliteStore::open_in_memory().unwrap()), vec![]);
    let n = capture(&t.app, &key, &shot(1, "key", "anahtar", "My keys.", true))
        .unwrap()
        .note_id;
    t.ok::<SettingsSet>(SetSetting {
        scope: SettingScope::App,
        key: CLIP_OFFSET.into(),
        value: json!(-200),
    });
    let r = run(
        &t,
        FillAudio {
            video: Some(AudioFillVideo {
                path: video.display().to_string(),
                source: Some("Show · S1E2".into()),
            }),
            ..ids(&[n])
        },
    );
    assert_eq!((r.done.clip, r.failed), (1, 0), "{r:?}");
    let h = hash_of(&audio_of(&t, n));
    let o = origin(&t, &h);
    assert_eq!(o.origin, MediaOrigin::Clip);
    assert_eq!(o.source.as_deref(), Some("show-s1e2.wav"));
    let m = t.store.read().unwrap().get_media(&h).unwrap().unwrap();
    assert_eq!(m.mime, "audio/mpeg");
}

/// A note with several encounters: the fill gives the PRIMARY example its sound (and the
/// card); the other example is left as it is. W3's scene ranking sees the filled sound, and
/// ranks it as synthesised.
#[test]
fn only_the_primary_example_is_filled() {
    let (t, calls, key) = app(Arc::new(SqliteStore::open_in_memory().unwrap()), vec!["en"]);
    let n = capture(&t.app, &key, &shot(1, "key", "anahtar", "Where are my keys?", false))
        .unwrap()
        .note_id;
    let m = capture(&t.app, &key, &shot(2, "key", "anahtar", "Keys on the table.", false))
        .unwrap()
        .note_id;
    assert_eq!(n, m, "the same meaning is another example of one note");
    let r = run(&t, ids(&[n]));
    assert_eq!((r.done.tts, calls.load(Ordering::SeqCst)), (1, 1));
    let enc = t
        .ok::<SubtitleEncountersList>(ListSubtitleEncounters {
            note_id: Some(n),
            ..Default::default()
        })
        .encounters;
    assert_eq!(enc.len(), 2);
    let (p, o): (Vec<_>, Vec<_>) = enc.iter().partition(|e| e.is_primary);
    assert_eq!(p[0].facts.audio_origin, Some(MediaOrigin::Tts));
    assert_eq!(audio_of(&t, n), format!("media:{}", p[0].facts.audio_hash.clone().unwrap()));
    assert_eq!(o[0].facts.audio_hash, None);
    assert_eq!(o[0].facts.audio_origin, None);
    assert!(!o[0].facts.example_fields.contains_key("audio"));
    // W3's ranking: the filled sound counts, as synthesised.
    let ranked = t.ok::<SubtitleEncountersRank>(fc_api::v1::examples::RankEncounters {
        note_id: n,
    });
    let s = ranked
        .items
        .iter()
        .find(|e| e.id == p[0].capture_id)
        .unwrap();
    assert!(s
        .reasons
        .iter()
        .any(|r| r.code == fc_core::encounter_score::SceneReasonCode::SynthesizedAudio));
}
