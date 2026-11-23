//! v1.0 performance suite (docs/02 §34, docs/03 §17; acceptance criterion "performance targets
//! are met by benchmarks", docs/04 v1.0).
//!
//! A collection is built on a SQLite file (classifications, tags, all card states, a review
//! history), then a **separate process** — this test binary re-run with only [`probe`] —
//! opens it like the desktop app does and times what the user does, through [`App::invoke`]
//! (JSON in, JSON out, as every transport). The separate process keeps the measurements and
//! the peak memory free of the world builder and of tests running in parallel.
//!
//! | Target | Measured as | Full (`--ignored`, release) | CI (every `cargo test`) |
//! |---|---|---|---|
//! | Startup < 2 s | `App::open_sqlite` (migrations check, seeding, crash recovery, trash purge, plugins) + the first screen's queries | 2 000 ms | 6 000 ms |
//! | Next card < 20 ms | `study.answer` round trip that returns the next card (p95 of 40) | 20 ms | 120 ms |
//! | Study start | `study.start` over the whole collection (normal session) | 500 ms | 3 000 ms |
//! | Search 100k < 100 ms | slowest browser query (count + page of 100), classification queries included | 100 ms | 300 ms |
//! | Browser scroll | one 100-row page anywhere in the full list, three sort orders | 100 ms | 300 ms |
//! | Import 10k | 10 000-row text import job, start to finished | 10 000 ms | 20 000 ms (2 000 rows) |
//! | Stats on 2M reviews | cached overview, breakdown by classification (cached day counts + appended rows), deck scope | 1 000 / 1 500 / 10 000 ms | 3 000 / 10 000 / 10 000 ms |
//! | Memory | peak resident set of the probe process (Linux) | 400 MB | 400 MB |
//!
//! Full size: 100k notes / 200k cards / 2M reviews:
//! `cargo test -p fc-app --release --lib perf_suite_100k -- --ignored --nocapture`
//! (`FC_PERF_DIR=<dir>` when the temp directory is a small tmpfs).
//! CI size: 10k notes / 20k cards / 200k reviews (`perf_suite_scaled`, debug build).
//! Last measurements: docs/10 §36.

use crate::perf_film::{self, FilmBuild, FilmMeasured, FilmScale};
use crate::App;
use fc_api::v1::oplog::Actor;
use fc_core::attribute::{AttributeValue, EntityRef};
use fc_core::card::{CardState, Rating};
use fc_core::content::builtin;
use fc_core::note::{note_checksum, searchable_text, sort_field_text};
use fc_core::notetype::FieldKey;
use fc_core::rng::{Rng, SplitMix64};
use fc_core::{CardId, Clock, NoteId, ReviewLogId, Timestamp};
use fc_storage::repo::{CardFilter, ReviewLogRecord};
use fc_storage::{SqliteStore, Store};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;
use std::time::Instant;

const DAY: i64 = 86_400_000;
/// Environment variable naming the collection the [`probe`] process measures.
const PROBE_DB: &str = "FC_PERF_PROBE_DB";
/// Optional directory for the built collection (default: the system temp directory).
const PERF_DIR: &str = "FC_PERF_DIR";
/// Optional file the freshly built collection is copied to (profiling).
const PERF_KEEP: &str = "FC_PERF_KEEP";
/// Import rows for the probe.
const PROBE_IMPORT_ROWS: &str = "FC_PERF_PROBE_IMPORT_ROWS";
/// The film layer's scale (JSON) for the probe.
const PROBE_FILM: &str = "FC_PERF_PROBE_FILM";
/// Marker of the probe's result line on stdout.
const RESULT_MARKER: &str = "FC_PERF_RESULT ";

/// Words for fields (Turkish letters on purpose: folding and FTS work on them).
const WORDS: &[&str] = &[
    "kılıç",
    "kalkan",
    "ağaç",
    "çiçek",
    "göl",
    "şehir",
    "ışık",
    "öğretmen",
    "apple",
    "badger",
    "river",
    "mountain",
    "journey",
    "whisper",
    "harbor",
    "lantern",
    "meadow",
    "thunder",
    "türev",
    "integral",
    "kuvvet",
    "enerji",
    "hücre",
    "atom",
];

/// Browser queries timed (docs/02 §9 and §8.1: classification queries included).
const SEARCH_QUERIES: &[&str] = &[
    "",
    "kılıç",
    "\"ağaç çiçek\"",
    "deck:İngilizce is:due",
    "tag:fiil -is:suspended",
    "level>=B1",
    "level:B1 is:due",
    "exam:TYT deck:YKS",
    "(level:A1 OR level:C2) apple",
    "-level:A1 deck:İngilizce tag:isim",
    "stability>20 lapses>=2",
    "retrievability<0.8",
    "front:wh*",
    "is:new deck:YKS::TYT",
];

/// How big a world, and the budgets that apply to it (ms unless noted).
#[derive(Debug, Clone, Copy)]
struct Scale {
    notes: usize,
    reviews: usize,
    import_rows: usize,
    startup: f64,
    next_card_p95: f64,
    study_start: f64,
    search: f64,
    scroll: f64,
    import: f64,
    stats_cached: f64,
    stats_breakdown: f64,
    stats_deck: f64,
    peak_rss_mb: f64,
    /// v1.4 film-watching layer (docs/12-performans.md) and its budgets.
    film: FilmScale,
    film_capture_p95: f64,
    busy_answer_p95: f64,
    busy_capture_p95: f64,
    film_first_card: f64,
    lookup_500_p95: f64,
    words_list: f64,
    lang_query: f64,
    rank_p95: f64,
    index_rebuild: f64,
    backup_create: f64,
    backup_verify: f64,
    backup_peak_mb: f64,
}

/// docs/03 §17 size, docs/02 §34 targets (release build).
const FULL: Scale = Scale {
    notes: 100_000,
    reviews: 2_000_000,
    import_rows: 10_000,
    startup: 2_000.0,
    next_card_p95: 20.0,
    study_start: 500.0,
    search: 100.0,
    scroll: 100.0,
    import: 10_000.0,
    stats_cached: 1_000.0,
    stats_breakdown: 1_500.0,
    stats_deck: 10_000.0,
    peak_rss_mb: 400.0,
    film: FilmScale {
        encounters: 20_000,
        notes: 6_000,
        with_media: 3_000,
        bulk_media_mib: 1_024,
    },
    film_capture_p95: 50.0,
    busy_answer_p95: 60.0,
    busy_capture_p95: 100.0,
    // Film budgets: ~2× the first release measurement on a 4-core 2.1 GHz Xeon (docs/12).
    film_first_card: 1_500.0,
    lookup_500_p95: 100.0,
    words_list: 200.0,
    lang_query: 300.0,
    rank_p95: 50.0,
    index_rebuild: 5_000.0,
    backup_create: 150_000.0,
    backup_verify: 120_000.0,
    // v1.4 Z2: the dump is streamed both ways (2M reviews: ~380 MB, was 2.7 GB).
    backup_peak_mb: 1_000.0,
};

/// CI regression check: 1/10 size in a debug build on shared runners, so the budgets are
/// the full targets with generous headroom (they catch an accidental O(n²) or a lost index,
/// not 10 % drifts).
const SCALED: Scale = Scale {
    notes: 10_000,
    reviews: 200_000,
    import_rows: 2_000,
    startup: 6_000.0,
    next_card_p95: 120.0,
    study_start: 3_000.0,
    search: 300.0,
    scroll: 300.0,
    import: 20_000.0,
    stats_cached: 3_000.0,
    stats_breakdown: 10_000.0,
    stats_deck: 10_000.0,
    peak_rss_mb: 400.0,
    film: FilmScale {
        encounters: 1_000,
        notes: 300,
        with_media: 100,
        bulk_media_mib: 32,
    },
    film_capture_p95: 300.0,
    busy_answer_p95: 400.0,
    busy_capture_p95: 600.0,
    film_first_card: 3_000.0,
    lookup_500_p95: 600.0,
    words_list: 1_000.0,
    lang_query: 1_000.0,
    rank_p95: 300.0,
    index_rebuild: 20_000.0,
    backup_create: 60_000.0,
    backup_verify: 30_000.0,
    backup_peak_mb: 800.0,
};

/// What the probe measured (ms; memory in MB, `None` where the platform cannot tell).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Measured {
    startup: f64,
    study_start: f64,
    custom_start: f64,
    next_card_p50: f64,
    next_card_p95: f64,
    search_worst: f64,
    search_worst_query: String,
    scroll_worst: f64,
    stats_cached: f64,
    stats_breakdown: f64,
    stats_deck: f64,
    import: f64,
    imported: u64,
    peak_rss_mb: Option<f64>,
    /// v1.4 film-watching workload.
    film: FilmMeasured,
}

fn invoke(app: &App, method: &str, params: Value) -> Value {
    match app.invoke(&format!("v1.{method}"), params, &Actor::User) {
        Ok(v) => v,
        Err(e) => panic!("{method} failed: {e:?}"),
    }
}

/// Milliseconds of one run of `f`.
fn time<R>(f: impl FnOnce() -> R) -> (f64, R) {
    let t = Instant::now();
    let r = f();
    (t.elapsed().as_secs_f64() * 1000.0, r)
}

/// Best of `runs`, ms.
fn best(runs: usize, mut f: impl FnMut()) -> f64 {
    (0..runs)
        .map(|_| time(&mut f).0)
        .fold(f64::INFINITY, f64::min)
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let rank = ((sorted.len() as f64) * p).ceil() as usize;
    sorted[rank.clamp(1, sorted.len()) - 1]
}

fn id_u128(kind: u128, n: usize) -> uuid::Uuid {
    // UUIDv7 layout (version 7, variant 10) with a 2024 timestamp prefix: never collides with
    // the app's own ids (taken from the wall clock).
    uuid::Uuid::from_u128(0x0190_0000_0000_7000_8000_0000_0000_0000 | (kind << 40) | n as u128)
}

// ---------------------------------------------------------------------------- world

/// Builds the collection at `path`: decks under `İngilizce` (CEFR `level`) and `YKS` (`exam`),
/// `notes` two-card notes (classic + reverse) with classifications and tags, card states as in
/// a long-used collection, `reviews` review rows over two years, and a warm statistics cache.
fn build_world(path: &Path, notes: usize, reviews: usize) {
    let app = App::open_sqlite(path).unwrap();
    let now = crate::system::SystemClock.now().0;
    let deck = |path: &str| -> fc_core::DeckId {
        let d = invoke(&app, "decks.create", json!({ "path": path }));
        serde_json::from_value(d["id"].clone()).unwrap()
    };
    let english = deck("İngilizce");
    let yks = deck("YKS");
    let mut en_decks = Vec::new();
    for i in 1..=8 {
        en_decks.push(deck(&format!("İngilizce::Kelimeler {i}")));
    }
    let mut yks_decks = Vec::new();
    for exam in ["TYT", "AYT"] {
        let parent = deck(&format!("YKS::{exam}"));
        yks_decks.push(parent);
        for lesson in ["Matematik", "Fizik", "Kimya"] {
            yks_decks.push(deck(&format!("YKS::{exam}::{lesson}")));
        }
    }
    let level = invoke(
        &app,
        "attributes.define_from_template",
        json!({ "template_id": "cefr", "scope_deck_id": english }),
    );
    let exam = invoke(
        &app,
        "attributes.define_from_template",
        json!({ "template_id": "yks", "scope_deck_id": yks }),
    );
    let options = |def: &Value| -> Vec<fc_core::AttributeOptionId> {
        def["options"]
            .as_array()
            .unwrap()
            .iter()
            .map(|o| serde_json::from_value(o["id"].clone()).unwrap())
            .collect()
    };
    let (level_id, level_opts): (fc_core::AttributeDefId, _) = (
        serde_json::from_value(level["id"].clone()).unwrap(),
        options(&level),
    );
    let (exam_id, exam_opts): (fc_core::AttributeDefId, _) = (
        serde_json::from_value(exam["id"].clone()).unwrap(),
        options(&exam),
    );
    let reverse = builtin::ALL
        .iter()
        .find(|b| b.key == "classic_reverse")
        .unwrap()
        .id;
    let seed = invoke(
        &app,
        "notes.create",
        json!({
            "notetype_id": reverse,
            "deck_id": en_decks[0],
            "fields": { "front": "seed", "back": "tohum" },
            "tags": ["fiil", "isim", "fiil::düzensiz"],
        }),
    );
    let seed_id: NoteId = serde_json::from_value(seed["id"].clone()).unwrap();
    drop(app);

    let store = SqliteStore::open(path).unwrap();
    let mut rng = SplitMix64::new(2026);
    let mut roll = move |n: u64| rng.next_u64() % n;
    let mut tx = store.write().unwrap();
    let nt = tx.get_notetype(reverse).unwrap().unwrap();
    let note0 = tx.get_note(seed_id).unwrap().unwrap();
    let cards0 = tx
        .list_cards(&CardFilter {
            note_ids: Some(vec![seed_id]),
            ..CardFilter::default()
        })
        .unwrap();
    assert_eq!(cards0.len(), 2);
    let mut review_cards: Vec<CardId> = Vec::new();
    for i in 0..notes {
        let mut note = note0.clone();
        note.id = NoteId(id_u128(1, i));
        let front = format!(
            "{} {} {i}",
            WORDS[roll(WORDS.len() as u64) as usize],
            WORDS[roll(WORDS.len() as u64) as usize]
        );
        let back = format!(
            "{} — örnek cümle {i}",
            WORDS[roll(WORDS.len() as u64) as usize]
        );
        note.fields.insert(FieldKey::new("front"), front);
        note.fields.insert(FieldKey::new("back"), back);
        note.sort_field = sort_field_text(&nt, &note.fields);
        note.checksum = note_checksum(&nt, &note.fields);
        note.tags = note0
            .tags
            .iter()
            .filter(|_| roll(3) == 0)
            .cloned()
            .collect();
        let english_note = roll(3) != 0;
        note.home_deck_id = if english_note {
            en_decks[roll(en_decks.len() as u64) as usize]
        } else {
            yks_decks[roll(yks_decks.len() as u64) as usize]
        };
        let created = now - (1 + roll(700) as i64) * DAY;
        note.created_at = Timestamp(created);
        note.updated_at = Timestamp(created + roll(30) as i64 * DAY);
        tx.insert_note(&note, &searchable_text(&nt, &note.fields))
            .unwrap();
        let value = if english_note {
            (roll(5) != 0).then(|| {
                (
                    level_id,
                    Some(AttributeValue::Choice(
                        level_opts[roll(level_opts.len() as u64) as usize],
                    )),
                )
            })
        } else {
            (roll(5) != 0).then(|| {
                (
                    exam_id,
                    Some(AttributeValue::Choice(
                        exam_opts[roll(exam_opts.len() as u64) as usize],
                    )),
                )
            })
        };
        if let Some(v) = value {
            tx.set_attribute_values(EntityRef::Note(note.id), &[v])
                .unwrap();
        }
        for (k, card0) in cards0.iter().enumerate() {
            let mut card = card0.clone();
            card.id = CardId(id_u128(2, i * 2 + k));
            card.note_id = note.id;
            card.deck_id = note.home_deck_id;
            card.created_at = note.created_at;
            card.updated_at = note.updated_at;
            let s = &mut card.scheduling;
            match roll(20) {
                // 25 % new
                0..=4 => {}
                // 10 % (re)learning, due within the hour (all of them are in a normal
                // session's queue: learning cards are never limited)
                5 | 6 => {
                    s.state = if roll(2) == 0 {
                        CardState::Learning
                    } else {
                        CardState::Relearning
                    };
                    s.due = Some(Timestamp(now - DAY / 24 + roll(2 * 3_600_000) as i64));
                    s.stability = Some(0.5 + roll(30) as f64 / 10.0);
                    s.difficulty = Some(1.0 + roll(90) as f64 / 10.0);
                    s.reps = 1 + roll(3) as u32;
                    s.lapses = roll(3) as u32;
                    s.last_review = Some(Timestamp(now - DAY));
                }
                // 65 % review: ~1/12 due today or overdue
                _ => {
                    s.state = CardState::Review;
                    s.due = Some(Timestamp(now + (roll(120) as i64 - 10) * DAY));
                    s.stability = Some(1.0 + roll(3_000) as f64 / 10.0);
                    s.difficulty = Some(1.0 + roll(90) as f64 / 10.0);
                    s.reps = 2 + roll(20) as u32;
                    s.lapses = roll(8) as u32;
                    s.last_review = Some(Timestamp(now - (1 + roll(60) as i64) * DAY));
                    review_cards.push(card.id);
                }
            }
            card.suspended = roll(40) == 0;
            card.flags = if roll(25) == 0 { 1 } else { 0 };
            tx.insert_card(&card).unwrap();
        }
    }
    let span = 730 * DAY;
    let mut batch = Vec::with_capacity(10_000);
    for n in 0..reviews {
        let at = now - span + (n as i64 * span) / reviews.max(1) as i64;
        let again = roll(10) == 0;
        batch.push(ReviewLogRecord {
            id: ReviewLogId(id_u128(3, n)),
            card_id: review_cards[roll(review_cards.len() as u64) as usize],
            session_id: None,
            rating: if again { Rating::Again } else { Rating::Good },
            state_before: if n % 50 == 0 {
                CardState::New
            } else {
                CardState::Review
            },
            state_after: CardState::Review,
            scheduled_days: 1 + roll(60) as u32,
            elapsed_days: roll(60) as u32,
            duration_ms: 2_000 + roll(15_000) as u32,
            answer_duration_ms: None,
            hints_used: 0,
            mode: if n % 20 == 0 { "imported" } else { "normal" }.into(),
            stability_after: Some(10.0),
            difficulty_after: Some(5.0),
            created_at: Timestamp(at),
        });
        if batch.len() == batch.capacity() {
            tx.insert_review_logs(&batch).unwrap();
            batch.clear();
        }
    }
    tx.insert_review_logs(&batch).unwrap();
    tx.commit().unwrap();
    drop(store);

    // The statistics cache is built once (in the background after an upgrade) and then
    // folded on every answer: the probe measures the screen, not the one-time build.
    let app = App::open_sqlite(path).unwrap();
    invoke(
        &app,
        "stats.refresh_cache",
        json!({ "tz_offset_mins": 180 }),
    );
}

// ---------------------------------------------------------------------------- probe

/// Peak resident set (MB) of this process, where the platform reports it.
fn peak_rss_mb() -> Option<f64> {
    perf_film::peak_rss_mb()
}

/// What the desktop app does, timed. Runs in its own process (see [`run_suite`]).
fn measure(path: &Path, import_rows: usize, film: Option<FilmScale>) -> Measured {
    let mut m = Measured::default();

    // Startup: open (+ migration check, seeding, recovery, trash purge, plugins) and the
    // queries the shell and the first screen (study) send before anything is drawn.
    let (startup, app) = time(|| {
        let app = App::open_sqlite(path).unwrap();
        invoke(&app, "modules.list", json!({}));
        invoke(&app, "decks.tree", json!({}));
        invoke(&app, "study.active", json!({}));
        invoke(&app, "shortcuts.list", json!({}));
        invoke(&app, "saved_queries.list", json!({}));
        invoke(&app, "flags.list", json!({}));
        app
    });
    m.startup = startup;

    // Search: count + first page of 100 rows, slowest query.
    for q in SEARCH_QUERIES {
        let ms = best(3, || {
            invoke(&app, "search.cards", json!({ "query": q, "limit": 100 }));
        });
        if ms > m.search_worst {
            m.search_worst = ms;
            m.search_worst_query = (*q).to_owned();
        }
    }

    // Browser scroll: a page anywhere in the full list (virtual scrolling asks by offset).
    let total = invoke(&app, "search.cards", json!({ "query": "", "limit": 1 }))["total"]
        .as_u64()
        .unwrap();
    for sort in ["sort_field", "due", "modified"] {
        for offset in [0, total / 3, total / 2, total.saturating_sub(100)] {
            let ms = best(2, || {
                let page = invoke(
                    &app,
                    "search.cards",
                    json!({ "query": "", "sort": sort, "offset": offset, "limit": 100 }),
                );
                assert!(!page["items"].as_array().unwrap().is_empty());
            });
            m.scroll_worst = m.scroll_worst.max(ms);
        }
    }

    // Study: start a session over the whole collection, then answer; every answer returns
    // the next card (that round trip is "next card").
    let (start, mut session) = time(|| {
        invoke(
            &app,
            "study.start",
            json!({ "kind": "normal", "tz_offset_mins": 180 }),
        )
    });
    m.study_start = start;
    let mut laps = Vec::new();
    for _ in 0..40 {
        let Some(card) = session["current"].as_object().map(|c| c["card_id"].clone()) else {
            break;
        };
        let (ms, next) = time(|| {
            invoke(
                &app,
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
    assert!(
        laps.len() >= 20,
        "study session ran dry after {} cards",
        laps.len()
    );
    laps.sort_by(f64::total_cmp);
    m.next_card_p50 = percentile(&laps, 0.5);
    m.next_card_p95 = percentile(&laps, 0.95);
    invoke(&app, "study.end", json!({ "session_id": session["id"] }));
    // A custom session by classification (docs/02 §8.1).
    let (custom, s) = time(|| {
        invoke(
            &app,
            "study.start",
            json!({
                "kind": "custom",
                "filter": { "query": "level>=B1 deck:İngilizce", "limit": 200 },
            }),
        )
    });
    m.custom_start = custom;
    assert!(s["current"].is_object(), "no card for level>=B1");
    invoke(&app, "study.end", json!({ "session_id": s["id"] }));

    // Statistics (docs/02 §17, §8.1 breakdown) on the warm cache.
    m.stats_cached = best(3, || {
        let o = invoke(&app, "stats.overview", json!({ "tz_offset_mins": 180 }));
        assert_eq!(o["from_cache"], json!(true));
    });
    let level_id = invoke(&app, "attributes.list", json!({}))["defs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["key"] == "level")
        .unwrap()["id"]
        .clone();
    m.stats_breakdown = best(2, || {
        let b = invoke(
            &app,
            "stats.breakdown",
            json!({ "attr_id": level_id, "tz_offset_mins": 180 }),
        );
        assert!(!b["rows"].as_array().unwrap().is_empty());
    });
    let english = invoke(&app, "decks.list", json!({}))["decks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["path"] == "İngilizce")
        .unwrap()["id"]
        .clone();
    m.stats_deck = best(2, || {
        invoke(
            &app,
            "stats.overview",
            json!({ "scope": { "deck_id": english }, "tz_offset_mins": 180 }),
        );
    });

    // Import: a pasted word list of `import_rows` lines, start to finished job.
    let target = invoke(&app, "decks.create", json!({ "path": "İçe aktarılan" }))["id"].clone();
    let text: String = (0..import_rows)
        .map(|i| format!("yeni kelime {i}\tnew word {i}\n"))
        .collect();
    let (import, report) = time(|| {
        let job = invoke(
            &app,
            "io.import.start",
            json!({ "source": { "kind": "text", "text": text }, "deck_id": target }),
        );
        app.run_pending_jobs().unwrap();
        invoke(&app, "jobs.get", json!({ "id": job["id"] }))
    });
    assert_eq!(report["status"], json!("succeeded"), "{report}");
    m.import = import;
    m.imported = report["result"]["added_notes"].as_u64().unwrap_or(0);

    m.peak_rss_mb = peak_rss_mb();

    // v1.4: the film-watching workload (its backup measures its own memory peak).
    if let Some(film) = film {
        m.film = perf_film::measure(&app, path.parent().unwrap(), film);
    }
    m
}

/// The measuring process: does nothing unless started by [`run_suite`] with
/// [`PROBE_DB`] set.
#[test]
#[ignore = "performance probe: started by the perf suites in their own process"]
fn probe() {
    let Ok(path) = std::env::var(PROBE_DB) else {
        return;
    };
    let rows = std::env::var(PROBE_IMPORT_ROWS)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1_000);
    let film = std::env::var(PROBE_FILM)
        .ok()
        .and_then(|v| serde_json::from_str(&v).ok());
    let m = measure(Path::new(&path), rows, film);
    println!("{RESULT_MARKER}{}", serde_json::to_string(&m).unwrap());
}

/// Builds the world, measures it in a fresh process, prints the table, checks the budgets.
fn run_suite(scale: Scale) -> Measured {
    // The full collection is ~1–2 GB with its WAL: `FC_PERF_DIR` puts it on a real disk
    // when the temp directory is a small tmpfs.
    let dir = match std::env::var_os(PERF_DIR) {
        Some(base) => {
            std::fs::create_dir_all(&base).unwrap();
            tempfile::tempdir_in(base).unwrap()
        }
        None => tempfile::tempdir().unwrap(),
    };
    let path = dir.path().join("collection.db");
    let (setup, ()) = time(|| build_world(&path, scale.notes, scale.reviews));
    let (film_setup, built) = time(|| perf_film::build(&path, scale.film));
    // `FC_PERF_KEEP=<file>`: a copy of the freshly built collection, for profiling the
    // probe on its own (`FC_PERF_PROBE_DB=<copy> cargo test … perf_tests::probe -- --ignored`).
    if let Some(keep) = std::env::var_os(PERF_KEEP) {
        std::fs::copy(&path, &keep).unwrap();
        let wal = path.with_extension("db-wal");
        if wal.exists() {
            let mut to = keep.clone();
            to.push("-wal");
            std::fs::copy(wal, to).unwrap();
        }
    }
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "perf_tests::probe",
            "--exact",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(PROBE_DB, &path)
        .env(PROBE_IMPORT_ROWS, scale.import_rows.to_string())
        .env(PROBE_FILM, serde_json::to_string(&scale.film).unwrap())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let Some(line) = stdout
        .lines()
        .find_map(|l| l.split_once(RESULT_MARKER).map(|(_, json)| json))
    else {
        panic!(
            "probe failed ({}):\n{stdout}\n{}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        );
    };
    let m: Measured = serde_json::from_str(line).unwrap();
    let rss = m
        .peak_rss_mb
        .map_or_else(|| "n/a".to_owned(), |v| format!("{v:.0} MB"));
    println!(
        "perf suite: {} notes / {} cards / {} reviews (setup {:.1} s)\n\
         | startup                | {:>8.1} ms | budget {:>6.0} ms |\n\
         | study.start (all)      | {:>8.1} ms | budget {:>6.0} ms |\n\
         | study.start (level>=B1)| {:>8.1} ms |                  |\n\
         | next card p50 / p95    | {:>4.1} / {:>4.1} ms | budget p95 {:>2.0} ms |\n\
         | search (worst: {:?}) | {:>8.1} ms | budget {:>6.0} ms |\n\
         | browser page (worst)   | {:>8.1} ms | budget {:>6.0} ms |\n\
         | stats cached overview  | {:>8.1} ms | budget {:>6.0} ms |\n\
         | stats by classification| {:>8.1} ms | budget {:>6.0} ms |\n\
         | stats deck scope       | {:>8.1} ms | budget {:>6.0} ms |\n\
         | import {} rows      | {:>8.1} ms | budget {:>6.0} ms |\n\
         | peak memory            | {rss:>11} | budget {:>6.0} MB |",
        scale.notes,
        scale.notes * 2,
        scale.reviews,
        setup / 1000.0,
        m.startup,
        scale.startup,
        m.study_start,
        scale.study_start,
        m.custom_start,
        m.next_card_p50,
        m.next_card_p95,
        scale.next_card_p95,
        m.search_worst_query,
        m.search_worst,
        scale.search,
        m.scroll_worst,
        scale.scroll,
        m.stats_cached,
        scale.stats_cached,
        m.stats_breakdown,
        scale.stats_breakdown,
        m.stats_deck,
        scale.stats_deck,
        scale.import_rows,
        m.import,
        scale.import,
        scale.peak_rss_mb,
    );
    print_film(&scale, &built, film_setup, &m.film);
    assert_eq!(m.imported, scale.import_rows as u64, "{m:?}");
    let checks = [
        ("startup", m.startup, scale.startup),
        ("next card p95", m.next_card_p95, scale.next_card_p95),
        ("study.start", m.study_start, scale.study_start),
        ("search", m.search_worst, scale.search),
        ("browser page", m.scroll_worst, scale.scroll),
        ("stats cached", m.stats_cached, scale.stats_cached),
        ("stats breakdown", m.stats_breakdown, scale.stats_breakdown),
        ("stats deck scope", m.stats_deck, scale.stats_deck),
        ("import", m.import, scale.import),
        (
            "film capture p95",
            built.capture_p95,
            scale.film_capture_p95,
        ),
        (
            "answer p95 while capturing",
            m.film.busy_answer_p95,
            scale.busy_answer_p95,
        ),
        (
            "capture p95 while answering",
            m.film.busy_capture_p95,
            scale.busy_capture_p95,
        ),
        (
            "study.options + first Subtitle card",
            m.film.study_options + m.film.subtitle_first_card,
            scale.film_first_card,
        ),
        (
            "words.lookup 500 p95",
            m.film.lookup_p95,
            scale.lookup_500_p95,
        ),
        (
            "words.list by frequency",
            m.film.words_list_rank,
            scale.words_list,
        ),
        (
            "language queries",
            m.film.lang_query_worst,
            scale.lang_query,
        ),
        (
            "subtitle.encounters.rank p95",
            m.film.rank_p95,
            scale.rank_p95,
        ),
        (
            "words.index.rebuild_now",
            m.film.index_rebuild,
            scale.index_rebuild,
        ),
        (
            "io.backup.create",
            m.film.backup_create,
            scale.backup_create,
        ),
        (
            "io.backup.verify",
            m.film.backup_verify,
            scale.backup_verify,
        ),
    ];
    for (name, got, budget) in checks {
        assert!(
            got < budget,
            "{name}: {got:.1} ms ≥ budget {budget:.0} ms ({m:?})"
        );
    }
    if let Some(mb) = m.peak_rss_mb {
        assert!(
            mb < scale.peak_rss_mb,
            "peak memory {mb:.0} MB ≥ {} MB",
            scale.peak_rss_mb
        );
    }
    assert!(m.film.busy_answers >= 20, "{:?}", m.film);
    assert!(m.film.busy_captures >= 20, "{:?}", m.film);
    // The backup carries the large media folder (and every small file).
    assert!(
        m.film.backup_media >= built.media_files as u64,
        "backup has {} media, the layer made {}",
        m.film.backup_media,
        built.media_files
    );
    for (name, peak) in [
        ("backup create", m.film.backup_create_peak_mb),
        ("backup verify", m.film.backup_verify_peak_mb),
    ] {
        if let Some(mb) = peak {
            assert!(
                mb < scale.backup_peak_mb,
                "{name}: peak memory {mb:.0} MB ≥ {} MB",
                scale.backup_peak_mb
            );
        }
    }
    m
}

/// The film layer's table (docs/12-performans.md).
fn print_film(scale: &Scale, built: &FilmBuild, setup: f64, f: &FilmMeasured) {
    let mb = |v: Option<f64>| v.map_or_else(|| "n/a".to_owned(), |v| format!("{v:.0} MB"));
    println!(
        "film layer: {} encounters / {} words / {} media files ({:.0} MiB large) (setup {:.1} s)\n\
         | capture (build, {} sent)      | p50 {:.1} / p95 {:.1} / max {:.1} ms | budget p95 {:.0} ms |\n\
         | answer while capturing ({})    | p50 {:.1} / p95 {:.1} / max {:.1} ms | budget p95 {:.0} ms |\n\
         | capture while answering ({})   | p50 {:.1} / p95 {:.1} / max {:.1} ms | budget p95 {:.0} ms |\n\
         | study.options / first card     | {:.1} / {:.1} ms | budget sum {:.0} ms |\n\
         | words.lookup 500 forms         | p50 {:.1} / p95 {:.1} / max {:.1} ms | budget p95 {:.0} ms |\n\
         | words.list by frequency        | {:.1} ms | budget {:.0} ms |\n\
         | language queries (worst {:?}) | {:.1} ms | budget {:.0} ms |\n\
         | subtitle.encounters.rank       | p50 {:.1} / p95 {:.1} / max {:.1} ms | budget p95 {:.0} ms |\n\
         | words.index.rebuild_now ({} w) | {:.1} ms | budget {:.0} ms |\n\
         | io.backup.create ({:.0} MiB, {} media) | {:.1} ms, peak {} | budget {:.0} ms / {:.0} MB |\n\
         | io.backup.verify               | {:.1} ms, peak {} | budget {:.0} ms / {:.0} MB |",
        scale.film.encounters,
        scale.film.notes,
        built.media_files,
        built.media_bytes as f64 / 1_048_576.0,
        setup / 1000.0,
        built.captures,
        built.capture_p50,
        built.capture_p95,
        built.capture_max,
        scale.film_capture_p95,
        f.busy_answers,
        f.busy_answer_p50,
        f.busy_answer_p95,
        f.busy_answer_max,
        scale.busy_answer_p95,
        f.busy_captures,
        f.busy_capture_p50,
        f.busy_capture_p95,
        f.busy_capture_max,
        scale.busy_capture_p95,
        f.study_options,
        f.subtitle_first_card,
        scale.film_first_card,
        f.lookup_p50,
        f.lookup_p95,
        f.lookup_max,
        scale.lookup_500_p95,
        f.words_list_rank,
        scale.words_list,
        f.lang_query_worst_query,
        f.lang_query_worst,
        scale.lang_query,
        f.rank_p50,
        f.rank_p95,
        f.rank_max,
        scale.rank_p95,
        f.index_words,
        f.index_rebuild,
        scale.index_rebuild,
        f.backup_bytes as f64 / 1_048_576.0,
        f.backup_media,
        f.backup_create,
        mb(f.backup_create_peak_mb),
        scale.backup_create,
        scale.backup_peak_mb,
        f.backup_verify,
        mb(f.backup_verify_peak_mb),
        scale.backup_verify,
        scale.backup_peak_mb,
    );
}

/// docs/02 §34 targets at docs/03 §17 size. Release build:
/// `cargo test -p fc-app --release --lib perf_suite_100k -- --ignored --nocapture`.
#[test]
#[ignore = "benchmark: builds a 100k-note / 200k-card / 2M-review collection"]
fn perf_suite_100k() {
    run_suite(FULL);
}

/// The same suite at 1/10 size, on every `cargo test` (CI regression check).
#[test]
fn perf_suite_scaled() {
    run_suite(SCALED);
}

/// Profiling helper (v1.4 Z2): times one hot path of a kept collection (`FC_PERF_KEEP`)
/// `FC_PERF_HOT_N` times, so a sampler (gdb, perf) can attach to a steady loop:
/// `FC_PERF_HOT_DB=<copy> FC_PERF_HOT_OP=tree|start|breakdown|page|options cargo test -p fc-app
/// --release --lib perf_tests::hot -- --ignored --nocapture`. The collection is written to
/// (sessions are started and ended): give it a copy.
#[test]
#[ignore = "profiling helper: needs FC_PERF_HOT_DB"]
fn hot() {
    let Ok(path) = std::env::var("FC_PERF_HOT_DB") else {
        return;
    };
    let op = std::env::var("FC_PERF_HOT_OP").unwrap_or_else(|_| "tree".into());
    let n: usize = std::env::var("FC_PERF_HOT_N")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5);
    let (open, app) = time(|| App::open_sqlite(Path::new(&path)).unwrap());
    println!("open {open:.1} ms");
    let level_id = invoke(&app, "attributes.list", json!({}))["defs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["key"] == "level")
        .map(|a| a["id"].clone());
    let total = invoke(&app, "search.cards", json!({ "query": "", "limit": 1 }))["total"]
        .as_u64()
        .unwrap();
    for i in 0..n {
        let (ms, ()) = time(|| match op.as_str() {
            "tree" => {
                invoke(&app, "decks.tree", json!({}));
            }
            "summaries" => {
                invoke(&app, "decks.summaries", json!({}));
            }
            "start" => {
                let s = invoke(
                    &app,
                    "study.start",
                    json!({ "kind": "normal", "tz_offset_mins": 180 }),
                );
                invoke(&app, "study.end", json!({ "session_id": s["id"] }));
            }
            "options" => {
                invoke(&app, "study.options", json!({ "tz_offset_mins": 180 }));
            }
            "breakdown" => {
                invoke(
                    &app,
                    "stats.breakdown",
                    json!({ "attr_id": level_id, "tz_offset_mins": 180 }),
                );
            }
            "page" => {
                for sort in ["sort_field", "due", "modified"] {
                    for offset in [0, total / 3, total / 2, total.saturating_sub(100)] {
                        let (ms, _) = time(|| {
                            invoke(
                                &app,
                                "search.cards",
                                json!({ "query": "", "sort": sort, "offset": offset, "limit": 100 }),
                            )
                        });
                        println!("  page {sort} @{offset}: {ms:.1} ms");
                    }
                }
            }
            other => panic!("unknown FC_PERF_HOT_OP {other}"),
        });
        println!("{op} #{i}: {ms:.1} ms");
    }
}
