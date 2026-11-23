//! v1.4 Z1: the **film-watching workload** of the performance suite (docs/12-performans.md).
//!
//! Layered on the [`crate::perf_tests`] world: the owner watches films and series with the
//! Subtitle extension, so the collection also holds Subtitle encounters — several scenes per
//! word — with small screenshots (JPEG) and audio clips (WAV), words in three languages, the
//! shipped English word-data pack, and a large media folder that backups must carry.
//!
//! [`build`] fills the world through the real paths (`subtitle.media.upload`,
//! `subtitle.captures.create` via [`crate::modules::subtitle::capture`], `media.add`), timing
//! every capture. [`measure`] runs in the probe process after the v1.0 measurements and times
//! what the owner does with it: a capture arriving while a study answer is being saved, the
//! study launcher and the next card, `words.lookup` of 500 forms, `words.list` by frequency,
//! the language queries, `subtitle.encounters.rank`, the word-index rebuild, and a backup
//! (create = write + verify-by-restore, then `io.backup.verify` alone) with its own peak
//! memory (the probe's high-water mark is reset before it, Linux `clear_refs`).

use crate::modules::subtitle;
use crate::App;
use base64::Engine;
use fc_api::v1::oplog::Actor;
use fc_api::v1::subtitle::SubtitleKeyDto;
use fc_core::rng::{Rng, SplitMix64};
use fc_core::word_data::WordPack;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;
use std::time::Instant;

/// How big the film layer is.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub(crate) struct FilmScale {
    /// Subtitle captures (encounters).
    pub encounters: usize,
    /// Distinct words (notes) they belong to.
    pub notes: usize,
    /// Captures that come with a screenshot and an audio clip.
    pub with_media: usize,
    /// Extra large media (MiB, in 16 MiB files) so a backup carries ~a GB.
    pub bulk_media_mib: usize,
}

/// What building the layer measured (captures through the real path, ms).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct FilmBuild {
    pub captures: usize,
    pub capture_p50: f64,
    pub capture_p95: f64,
    pub capture_max: f64,
    pub media_files: usize,
    pub media_bytes: u64,
}

/// What the probe measured (ms; MB for memory).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct FilmMeasured {
    /// Captures posted while a study session answers (another thread).
    pub busy_captures: usize,
    pub busy_capture_p50: f64,
    pub busy_capture_p95: f64,
    pub busy_capture_max: f64,
    /// The answers given meanwhile.
    pub busy_answers: usize,
    pub busy_answer_p50: f64,
    pub busy_answer_p95: f64,
    pub busy_answer_max: f64,
    /// `study.options` for the Subtitle deck.
    pub study_options: f64,
    /// `study.start` on the Subtitle deck until the first card is there.
    pub subtitle_first_card: f64,
    /// `words.lookup` of 500 forms.
    pub lookup_p50: f64,
    pub lookup_p95: f64,
    pub lookup_max: f64,
    /// `words.list` sorted by frequency (worst of the variants).
    pub words_list_rank: f64,
    /// Language queries: slowest count + page.
    pub lang_query_worst: f64,
    pub lang_query_worst_query: String,
    /// `subtitle.encounters.rank` over the notes with the most scenes.
    pub rank_p50: f64,
    pub rank_p95: f64,
    pub rank_max: f64,
    /// `words.index.rebuild_now`.
    pub index_rebuild: f64,
    pub index_words: u64,
    /// `io.backup.create` (write + verify by restore into a scratch profile).
    pub backup_create: f64,
    pub backup_create_peak_mb: Option<f64>,
    /// `io.backup.verify` alone.
    pub backup_verify: f64,
    pub backup_verify_peak_mb: Option<f64>,
    pub backup_bytes: u64,
    pub backup_media: u64,
}

fn invoke(app: &App, method: &str, params: Value) -> Value {
    match app.invoke(&format!("v1.{method}"), params, &Actor::User) {
        Ok(v) => v,
        Err(e) => panic!("{method} failed: {e:?}"),
    }
}

fn time<R>(f: impl FnOnce() -> R) -> (f64, R) {
    let t = Instant::now();
    let r = f();
    (t.elapsed().as_secs_f64() * 1000.0, r)
}

fn best(runs: usize, mut f: impl FnMut()) -> f64 {
    (0..runs)
        .map(|_| time(&mut f).0)
        .fold(f64::INFINITY, f64::min)
}

/// p50, p95 and max of `laps` (ms).
fn spread(laps: &mut [f64]) -> (f64, f64, f64) {
    laps.sort_by(f64::total_cmp);
    let p = |q: f64| {
        if laps.is_empty() {
            return 0.0;
        }
        let rank = ((laps.len() as f64) * q).ceil() as usize;
        laps[rank.clamp(1, laps.len()) - 1]
    };
    (p(0.5), p(0.95), laps.last().copied().unwrap_or(0.0))
}

/// Peak resident set (MB) of this process since the last [`reset_peak_rss`].
pub(crate) fn peak_rss_mb() -> Option<f64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|l| l.starts_with("VmHWM:"))?;
    let kb: f64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb / 1024.0)
}

/// Resets the process's peak resident set to its current size (Linux ≥ 4.0; elsewhere the
/// peak keeps counting from the start, which only over-reports).
fn reset_peak_rss() {
    let _ = std::fs::write("/proc/self/clear_refs", "5");
}

// ---------------------------------------------------------------------------- media

/// A tiny valid 8×8 baseline JPEG (made with Pillow, quality 50).
const JPEG_8X8: &[u8] = &[
    0xff, 0xd8, 0xff, 0xe0, 0x00, 0x10, 0x4a, 0x46, 0x49, 0x46, 0x00, 0x01, 0x01, 0x00, 0x00, 0x01,
    0x00, 0x01, 0x00, 0x00, 0xff, 0xdb, 0x00, 0x43, 0x00, 0x10, 0x0b, 0x0c, 0x0e, 0x0c, 0x0a, 0x10,
    0x0e, 0x0d, 0x0e, 0x12, 0x11, 0x10, 0x13, 0x18, 0x28, 0x1a, 0x18, 0x16, 0x16, 0x18, 0x31, 0x23,
    0x25, 0x1d, 0x28, 0x3a, 0x33, 0x3d, 0x3c, 0x39, 0x33, 0x38, 0x37, 0x40, 0x48, 0x5c, 0x4e, 0x40,
    0x44, 0x57, 0x45, 0x37, 0x38, 0x50, 0x6d, 0x51, 0x57, 0x5f, 0x62, 0x67, 0x68, 0x67, 0x3e, 0x4d,
    0x71, 0x79, 0x70, 0x64, 0x78, 0x5c, 0x65, 0x67, 0x63, 0xff, 0xdb, 0x00, 0x43, 0x01, 0x11, 0x12,
    0x12, 0x18, 0x15, 0x18, 0x2f, 0x1a, 0x1a, 0x2f, 0x63, 0x42, 0x38, 0x42, 0x63, 0x63, 0x63, 0x63,
    0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63,
    0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63,
    0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63, 0x63,
    0xff, 0xc0, 0x00, 0x11, 0x08, 0x00, 0x08, 0x00, 0x08, 0x03, 0x01, 0x22, 0x00, 0x02, 0x11, 0x01,
    0x03, 0x11, 0x01, 0xff, 0xc4, 0x00, 0x15, 0x00, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0xff, 0xc4, 0x00, 0x14, 0x10, 0x01,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0xff, 0xc4, 0x00, 0x14, 0x01, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x05, 0xff, 0xc4, 0x00, 0x14, 0x11, 0x01, 0x00, 0x00, 0x00, 0x00,
    0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xff, 0xda, 0x00, 0x0c,
    0x03, 0x01, 0x00, 0x02, 0x11, 0x03, 0x11, 0x00, 0x3f, 0x00, 0xa0, 0x01, 0x8b, 0x3f, 0xff, 0xd9,
];

/// [`JPEG_8X8`] made unique by a comment segment (still a valid JPEG).
fn jpeg(seed: u64) -> Vec<u8> {
    let comment = format!("fc-perf {seed}");
    let len = u16::try_from(comment.len() + 2).unwrap_or(u16::MAX);
    let mut v = Vec::with_capacity(JPEG_8X8.len() + comment.len() + 4);
    v.extend_from_slice(&JPEG_8X8[..2]);
    v.extend_from_slice(&[0xff, 0xfe]);
    v.extend_from_slice(&len.to_be_bytes());
    v.extend_from_slice(comment.as_bytes());
    v.extend_from_slice(&JPEG_8X8[2..]);
    v
}

/// A valid 16 kHz mono PCM WAV of `samples` samples of noise seeded by `seed`.
fn wav(seed: u64, samples: usize) -> Vec<u8> {
    let data_len = u32::try_from(samples * 2).unwrap_or(u32::MAX);
    let mut v = Vec::with_capacity(44 + samples * 2);
    v.extend_from_slice(b"RIFF");
    v.extend_from_slice(&(36 + data_len).to_le_bytes());
    v.extend_from_slice(b"WAVEfmt ");
    v.extend_from_slice(&16u32.to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes()); // PCM
    v.extend_from_slice(&1u16.to_le_bytes()); // mono
    v.extend_from_slice(&16_000u32.to_le_bytes());
    v.extend_from_slice(&32_000u32.to_le_bytes());
    v.extend_from_slice(&2u16.to_le_bytes());
    v.extend_from_slice(&16u16.to_le_bytes());
    v.extend_from_slice(b"data");
    v.extend_from_slice(&data_len.to_le_bytes());
    let mut rng = SplitMix64::new(seed);
    for _ in 0..samples {
        v.extend_from_slice(&((rng.next_u64() >> 52) as i16).to_le_bytes());
    }
    v
}

// ---------------------------------------------------------------------------- words

/// Made-up words of a language without a pack (deterministic, unique per `n`).
fn made_up(n: usize, syllables: &[&str]) -> String {
    let mut s = String::new();
    let mut k = n + syllables.len() * syllables.len();
    while k > 0 {
        s.push_str(syllables[k % syllables.len()]);
        k /= syllables.len();
    }
    s
}

/// The shipped English pack.
fn english_pack() -> WordPack {
    let shipped = fc_providers::word_data::bundled("en").expect("the English pack ships");
    WordPack::parse(shipped.bytes).expect("the shipped pack parses")
}

/// One word of the film layer: language, lemma, a surface form, a meaning.
struct Word {
    lang: &'static str,
    lemma: String,
    surface: String,
    meaning: String,
}

/// `notes` distinct words: 85 % English lemmas from the pack (skipping the 150 most frequent
/// function words), 10 % German, 5 % Spanish made-up words.
fn words(pack: &WordPack, notes: usize) -> Vec<Word> {
    // Inflected forms by lemma (`go` → `went`).
    let mut inflected: std::collections::HashMap<&str, &str> = Default::default();
    for (form, lemma) in &pack.forms {
        inflected.entry(lemma.as_str()).or_insert(form.as_str());
    }
    let mut english = pack.words.iter().skip(150);
    (0..notes)
        .map(|n| match n % 20 {
            0 | 1 => {
                let w = made_up(n, &["ha", "ber", "un", "ge", "lich", "sch", "ein", "rau"]);
                Word {
                    lang: "de",
                    surface: w.clone(),
                    lemma: w,
                    meaning: format!("almanca anlam {n}"),
                }
            }
            2 => {
                let w = made_up(n, &["ca", "mi", "no", "ra", "sol", "de", "ta", "lu"]);
                Word {
                    lang: "es",
                    surface: w.clone(),
                    lemma: w,
                    meaning: format!("ispanyolca anlam {n}"),
                }
            }
            _ => {
                let lemma = english.next().cloned().unwrap_or_else(|| {
                    made_up(n, &["ka", "lo", "mi", "ne", "ru", "sa", "te", "vi"])
                });
                let surface = if n % 3 == 0 {
                    inflected
                        .get(lemma.as_str())
                        .map_or_else(|| lemma.clone(), |f| (*f).to_owned())
                } else {
                    lemma.clone()
                };
                Word {
                    lang: "en",
                    surface,
                    lemma,
                    meaning: format!("anlam {n}"),
                }
            }
        })
        .collect()
}

/// The capture of scene `i` of word `w` (contract schema 1), with uploads if given.
fn capture_json(
    i: usize,
    w: &Word,
    fillers: &[String],
    roll: &mut impl FnMut(u64) -> u64,
) -> Value {
    let pick = |r: u64| fillers[r as usize % fillers.len()].as_str();
    let before = format!(
        "{} {} {}",
        capitalise(pick(roll(1_000))),
        pick(roll(1_000)),
        pick(roll(1_000))
    );
    let after = format!("{} {}", pick(roll(1_000)), pick(roll(1_000)));
    let sentence = format!("{before} {} {after}.", w.surface);
    let start = before.chars().count() + 1;
    let end = start + w.surface.chars().count();
    let episode = i % 60;
    let t = (i / 60) as u64 * 4_000 + 1_000;
    json!({
        "schema": 1,
        "capture_id": capture_id(i),
        "kind": "word",
        "lang": { "target": w.lang, "native": "tr" },
        "term": { "surface": w.surface, "lemma": w.lemma, "pos": "noun" },
        "definition": { "text": w.meaning, "source": "dict" },
        "context": {
            "sentence": sentence,
            "translation": format!("çeviri {i}"),
            "term_offset": [start, end],
        },
        "source": {
            "type": "video",
            "url": format!("https://www.youtube.com/watch?v=film{episode:07}"),
            "title": format!("Dizi S01E{:02}", episode + 1),
            "site": "youtube",
            "video_id": format!("film{episode:07}"),
            "t_start_ms": t,
            "t_end_ms": t + 2_500,
        },
        "media": {},
        "deck_id": null,
        "tags": ["subtitle"],
        "created_at": "2026-10-12T10:00:00.000Z",
        "client": { "name": "subtitle-extension", "ext_version": "3.1.3" },
    })
}

fn capitalise(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().chain(c).collect())
        .unwrap_or_default()
}

fn capture_id(i: usize) -> String {
    format!("0192812a-6f3c-7a41-9c2e-{i:012x}")
}

/// Enables the Subtitle module and makes an access key.
fn subtitle_key(app: &App) -> SubtitleKeyDto {
    invoke(
        app,
        "modules.set_enabled",
        json!({ "id": "subtitle", "enabled": true }),
    );
    let made = invoke(app, "subtitle.keys.create", json!({ "name": "perf" }));
    let secret = made["secret"].as_str().unwrap().to_owned();
    subtitle::authenticate(app, &subtitle::key_hash(&secret)).unwrap()
}

/// Sends capture `i` (uploading its media first when it has any); ms.
fn post(app: &App, key: &SubtitleKeyDto, i: usize, v: &Value, with_media: bool) -> f64 {
    let (ms, r) = time(|| {
        let mut v = v.clone();
        if with_media {
            let id = capture_id(i);
            let shot = subtitle::upload(app, key, &id, "screenshot", "image/jpeg", &jpeg(i as u64))
                .unwrap()
                .upload_id;
            let audio =
                subtitle::upload(app, key, &id, "audio", "audio/wav", &wav(i as u64, 2_000))
                    .unwrap()
                    .upload_id;
            v["media"] = json!({
                "screenshot": { "upload_id": shot, "mime": "image/jpeg" },
                "audio": { "upload_id": audio, "mime": "audio/wav" },
            });
        }
        subtitle::capture(app, key, &v)
    });
    if let Err(e) = r {
        panic!("capture {i} failed: {e:?}");
    }
    ms
}

/// Which note scene `i` belongs to: every note once, then skewed towards the first notes
/// (a few words are heard in many scenes).
fn note_of(i: usize, notes: usize, roll: &mut impl FnMut(u64) -> u64) -> usize {
    if i < notes {
        i
    } else {
        let a = roll(notes as u64) as usize;
        let b = roll(notes as u64) as usize;
        a * b / notes
    }
}

/// Builds the film layer on the collection at `path` (opened by the app as the desktop does).
pub(crate) fn build(path: &Path, scale: FilmScale) -> FilmBuild {
    let app = App::open_sqlite(path).unwrap();
    let key = subtitle_key(&app);
    let pack = english_pack();
    let fillers: Vec<String> = pack.words.iter().take(1_000).cloned().collect();
    let words = words(&pack, scale.notes);
    let mut rng = SplitMix64::new(1_404);
    let mut roll = move |n: u64| rng.next_u64() % n.max(1);
    let media_every = (scale.encounters / scale.with_media.max(1)).max(1);
    let mut laps = Vec::with_capacity(scale.encounters);
    let mut out = FilmBuild::default();
    for i in 0..scale.encounters {
        let n = note_of(i, scale.notes, &mut roll);
        let v = capture_json(i, &words[n], &fillers, &mut roll);
        let with_media = i % media_every == 0 && out.media_files < scale.with_media * 2;
        let ms = post(&app, &key, i, &v, with_media);
        if std::env::var("FC_PERF_SLOW_MS")
            .ok()
            .and_then(|v| v.parse::<f64>().ok())
            .is_some_and(|t| ms > t)
        {
            println!("slow capture #{i}: {ms:.1} ms (media: {with_media})");
        }
        laps.push(ms);
        if with_media {
            out.media_files += 2;
        }
    }
    // Large media (films' audio cut by the user, high-resolution images…): 16 MiB WAVs.
    for k in 0..scale.bulk_media_mib.div_ceil(16) {
        let bytes = wav(1_000_000 + k as u64, 8 * 1024 * 1024);
        out.media_bytes += bytes.len() as u64;
        invoke(
            &app,
            "media.add",
            json!({
                "name": format!("film-{k}.wav"),
                "data_base64": base64::engine::general_purpose::STANDARD.encode(&bytes),
            }),
        );
        out.media_files += 1;
    }
    out.captures = laps.len();
    (out.capture_p50, out.capture_p95, out.capture_max) = spread(&mut laps);
    out
}

// ---------------------------------------------------------------------------- probe

/// Language queries timed (docs/02 §9.1).
const LANGUAGE_QUERIES: &[&str] = &[
    "durum:öğreniyorum",
    "durum:biliyorum",
    "-durum:biliyorum dil:en",
    "karşılaşma>=3",
    "karşılaşma>=2 ses:özgün",
    "ses:yok",
    "görsel:var dil:de",
    "sıklık<2000",
    "sıklık<5000 -durum:biliyorum",
    "cefr:B1",
    "kelime:went",
    "nereden:altyazı",
    "kaynak:\"Dizi S01E1*\"",
];

/// Times the film workload on an opened collection (the probe process).
pub(crate) fn measure(app: &App, dir: &Path, scale: FilmScale) -> FilmMeasured {
    let mut m = FilmMeasured::default();
    let key = subtitle::authenticate(
        app,
        &subtitle::key_hash(&{
            let made = invoke(app, "subtitle.keys.create", json!({ "name": "probe" }));
            made["secret"].as_str().unwrap().to_owned()
        }),
    )
    .unwrap();
    let pack = english_pack();
    let fillers: Vec<String> = pack.words.iter().take(1_000).cloned().collect();
    let words = words(&pack, scale.notes + 200);

    // A capture arriving while a study answer is being saved: one thread answers, another
    // posts captures (new words and new scenes of known ones, every fourth with media).
    let session = invoke(
        app,
        "study.start",
        json!({ "kind": "normal", "tz_offset_mins": 180 }),
    );
    let done = std::sync::atomic::AtomicBool::new(false);
    let (mut answers, mut captures) = std::thread::scope(|s| {
        let poster = s.spawn(|| {
            let mut rng = SplitMix64::new(7);
            let mut roll = move |n: u64| rng.next_u64() % n.max(1);
            let mut laps = Vec::new();
            let mut i = 0usize;
            while !done.load(std::sync::atomic::Ordering::Relaxed) || laps.len() < 20 {
                let n = if i % 2 == 0 {
                    scale.notes + (i / 2) % 200
                } else {
                    note_of(scale.notes + i, scale.notes, &mut roll)
                };
                let id = 10_000_000 + i;
                let v = capture_json(id, &words[n], &fillers, &mut roll);
                laps.push(post(app, &key, id, &v, i % 4 == 0));
                i += 1;
                if laps.len() >= 400 {
                    break;
                }
            }
            laps
        });
        let mut laps = Vec::new();
        let mut session = session;
        for _ in 0..40 {
            let Some(card) = session["current"].as_object().map(|c| c["card_id"].clone()) else {
                break;
            };
            let (ms, next) = time(|| {
                invoke(
                    app,
                    "study.answer",
                    json!({
                        "session_id": session["id"],
                        "card_id": card,
                        "rating": "good",
                        "duration_ms": 4_000,
                    }),
                )
            });
            laps.push(ms);
            session = next;
        }
        done.store(true, std::sync::atomic::Ordering::Relaxed);
        invoke(app, "study.end", json!({ "session_id": session["id"] }));
        (laps, poster.join().unwrap())
    });
    m.busy_answers = answers.len();
    (m.busy_answer_p50, m.busy_answer_p95, m.busy_answer_max) = spread(&mut answers);
    m.busy_captures = captures.len();
    (m.busy_capture_p50, m.busy_capture_p95, m.busy_capture_max) = spread(&mut captures);

    // The launcher on the Subtitle deck, then its first card.
    let deck = invoke(app, "subtitle.status", json!({}))["deck_id"].clone();
    let deck = if deck.is_null() {
        invoke(app, "decks.list", json!({}))["decks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| {
                d["path"]
                    .as_str()
                    .is_some_and(|p| p.contains("Subtitle") || p.contains("Altyazı"))
            })
            .map(|d| d["id"].clone())
            .unwrap_or(Value::Null)
    } else {
        deck
    };
    m.study_options = best(3, || {
        invoke(
            app,
            "study.options",
            json!({ "deck_id": deck, "tz_offset_mins": 180 }),
        );
    });
    let (ms, s) = time(|| {
        invoke(
            app,
            "study.start",
            json!({ "kind": "normal", "deck_id": deck, "tz_offset_mins": 180 }),
        )
    });
    m.subtitle_first_card = ms;
    assert!(s["current"].is_object(), "no Subtitle card to study: {s}");
    invoke(app, "study.end", json!({ "session_id": s["id"] }));

    // `words.lookup` of 500 forms (lemmas, inflected forms, unknown words, expressions).
    let mut forms: Vec<String> = Vec::with_capacity(500);
    for (k, w) in words
        .iter()
        .filter(|w| w.lang == "en")
        .take(300)
        .enumerate()
    {
        forms.push(if k % 2 == 0 {
            w.surface.clone()
        } else {
            w.lemma.clone()
        });
    }
    forms.extend(pack.forms.keys().take(100).cloned());
    forms.extend((0..60).map(|k| made_up(k, &["zo", "qu", "xi", "py"])));
    forms.extend((0..40).map(|k| format!("{} up", pack.words[300 + k])));
    forms.truncate(500);
    let mut laps: Vec<f64> = (0..20)
        .map(|_| {
            time(|| {
                let r = invoke(
                    app,
                    "words.lookup",
                    json!({ "language": "en", "forms": forms }),
                );
                assert_eq!(r["forms"].as_array().map_or(0, Vec::len), forms.len());
            })
            .0
        })
        .collect();
    (m.lookup_p50, m.lookup_p95, m.lookup_max) = spread(&mut laps);

    // `words.list` by frequency: the whole language, then "not known yet, among the 5000 heard
    // most" (the owner's question), each a page of 200.
    for params in [
        json!({ "language": "en", "sort": "rank", "limit": 200 }),
        json!({ "language": "en", "sort": "rank", "status": "learning", "max_rank": 5000, "limit": 200 }),
        json!({ "sort": "band", "limit": 200 }),
    ] {
        let ms = best(3, || {
            let r = invoke(app, "words.list", params.clone());
            assert!(!r["words"].as_array().unwrap().is_empty(), "{params}: {r}");
        });
        m.words_list_rank = m.words_list_rank.max(ms);
    }

    // Language queries (count + first page of 100).
    for q in LANGUAGE_QUERIES {
        let ms = best(3, || {
            invoke(app, "search.cards", json!({ "query": q, "limit": 100 }));
        });
        if ms > m.lang_query_worst {
            m.lang_query_worst = ms;
            m.lang_query_worst_query = (*q).to_owned();
        }
    }

    // The most teachable scene, over the notes with the most scenes.
    let busy = invoke(
        app,
        "search.cards",
        json!({ "query": "karşılaşma>=4", "limit": 50 }),
    );
    let notes: Vec<Value> = busy["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["note_id"].clone())
        .collect();
    assert!(!notes.is_empty(), "no note with 4+ scenes");
    let mut laps: Vec<f64> = notes
        .iter()
        .map(|n| {
            time(|| {
                let r = invoke(app, "subtitle.encounters.rank", json!({ "note_id": n }));
                assert!(r["items"].as_array().map_or(0, Vec::len) >= 1, "{r}");
            })
            .0
        })
        .collect();
    (m.rank_p50, m.rank_p95, m.rank_max) = spread(&mut laps);

    // Word-index rebuild.
    let (ms, r) = time(|| invoke(app, "words.index.rebuild_now", json!({})));
    m.index_rebuild = ms;
    m.index_words = r["words"].as_u64().unwrap_or(0);

    // Backup: create (write, restore into a scratch profile, compare, put in place), then
    // verify alone; each with its own memory high-water mark.
    let file = dir.join("perf.fcbackup");
    let run = |method: &str| {
        let job = invoke(app, method, json!({ "path": file.to_string_lossy() }));
        app.run_pending_jobs().unwrap();
        let j = invoke(app, "jobs.get", json!({ "id": job["id"] }));
        assert_eq!(j["status"], json!("succeeded"), "{method}: {j}");
        j["result"].clone()
    };
    reset_peak_rss();
    let (ms, report) = time(|| run("io.backup.create"));
    m.backup_create = ms;
    m.backup_create_peak_mb = peak_rss_mb();
    m.backup_bytes = report["bytes"].as_u64().unwrap_or(0);
    m.backup_media = report["media"].as_u64().unwrap_or(0);
    assert_eq!(report["verified"], json!(true), "{report}");
    assert!(
        report["missing_media"]
            .as_array()
            .is_some_and(Vec::is_empty),
        "{report}"
    );
    reset_peak_rss();
    let (ms, report) = time(|| run("io.backup.verify"));
    m.backup_verify = ms;
    m.backup_verify_peak_mb = peak_rss_mb();
    assert_eq!(report["ok"], json!(true), "{report}");
    let _ = std::fs::remove_file(&file);
    m
}
