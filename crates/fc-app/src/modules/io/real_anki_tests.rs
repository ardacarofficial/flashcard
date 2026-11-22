//! v1.0 real-Anki validation (docs/04 v0.5 → v1.0, docs/10 §34), through the app's own
//! import/export jobs:
//!
//! * packages **made by real Anki** (`fc-interop/tests/fixtures/anki`, built with the
//!   official `anki` library by `tools/anki/make_fixtures.py`) import in every layout, the
//!   cards draw through the v0.3 engines (cloze, image occlusion, audio) and study;
//! * our export of that collection keeps every field exactly as Anki wrote it;
//! * `our_exports_open_in_real_anki` (opt-in: `FC_ANKI_PYTHON=<python with anki>`) opens our
//!   `.apkg` and `.colpkg` in real Anki (`tools/anki/check_import.py`): Check Database clean,
//!   notes/cards/scheduling/FSRS/flags/history/media identical, every card renders, image
//!   occlusion opens in Anki's own editor.

use crate::testing::{TempSqlite, TestApp};
use fc_api::v1::cardtypes::CardRef;
use fc_api::v1::common::Empty;
use fc_api::v1::io::*;
use fc_api::v1::jobs::{JobDto, JobRef, JobStatus};
use fc_api::v1::methods::*;
use fc_api::v1::study::{AnswerCard, StartSession, StudyKind, StudyMode};
use fc_core::card::{Card, CardState, Rating};
use fc_core::content::builtin;
use fc_core::DeckId;
use fc_interop::anki::{self, occlusion, AnkiCollection};
use fc_storage::repo::CardFilter;
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn fixture_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../fc-interop/tests/fixtures/anki")
        .join(name)
}

fn real_json() -> Value {
    serde_json::from_slice(&std::fs::read(fixture_path("real.json")).unwrap()).unwrap()
}

fn finish(t: &TestApp, job: &JobDto) -> JobDto {
    t.app.run_pending_jobs().unwrap();
    t.ok::<JobsGet>(JobRef { id: job.id })
}

fn result<T: DeserializeOwned>(job: &JobDto) -> T {
    assert_eq!(job.status, JobStatus::Succeeded, "{:?}", job.error);
    serde_json::from_value(job.result.clone().unwrap()).unwrap()
}

fn import_file(t: &TestApp, path: &Path) -> ImportReport {
    let job = t.ok::<IoImportStart>(StartImport {
        source: ImportSource::File {
            path: path.to_string_lossy().into_owned(),
        },
        format: None,
        text_options: None,
        deck_id: None,
        notetype_id: None,
        columns: None,
        skip_first_row: None,
        duplicates: None,
        create_missing_options: None,
        include_history: None,
    });
    result(&finish(t, &job))
}

fn export(t: &TestApp, format: ExportFormat, path: &Path) -> ExportReport {
    let job = t.ok::<IoExportStart>(StartExport {
        format,
        path: path.to_string_lossy().into_owned(),
        deck_id: None,
        include_history: Some(true),
        text_options: None,
    });
    result(&finish(t, &job))
}

fn all_cards(t: &TestApp) -> Vec<Card> {
    let mut cards = t
        .store
        .read()
        .unwrap()
        .list_cards(&CardFilter::default())
        .unwrap();
    cards.sort_by(|a, b| a.created_at.cmp(&b.created_at).then(a.id.cmp(&b.id)));
    cards
}

/// Imports a real-Anki package into a fresh SQLite app and checks what arrived.
fn import_real(name: &str) -> TestApp {
    let t = TestApp::new(Arc::new(TempSqlite::new()));
    let r = import_file(&t, &fixture_path(name));
    let exp = real_json();
    let notes = exp["notes"].as_array().unwrap();
    let cards: usize = notes.iter().map(|n| n["cards"].as_array().unwrap().len()).sum();
    assert_eq!(r.added_notes as usize, notes.len(), "{name}: {r:?}");
    assert_eq!(r.added_cards as usize, cards, "{name}: {r:?}");
    assert_eq!(r.added_media, 3, "{name}: {r:?}");
    // Every review except Anki's manual "set due date" entry (reported, not imported).
    let reviews = exp["revlog"].as_u64().unwrap() as u32 - 1;
    assert_eq!(r.added_reviews, reviews, "{name}: {r:?}");
    // Stock note types land on built-ins; only the custom "Vocab TR" is new.
    assert_eq!(r.added_notetypes, 1, "{name}: {r:?}");
    let keys: Vec<&str> = r.issues.iter().map(|i| i.message.key.as_str()).collect();
    assert_eq!(keys, ["io.issue.anki_skipped_reviews"], "{name}: {r:?}");
    t
}

fn render(t: &TestApp, card: &Card) -> fc_api::v1::cardtypes::RenderedSides {
    t.ok::<CardsRender>(CardRef { id: card.id }).sides
}

#[test]
fn real_anki_packages_import_draw_and_study() {
    for name in ["real-latest.apkg", "real-legacy.apkg", "real.colpkg"] {
        let t = import_real(name);
        let cards = all_cards(&t);
        assert_eq!(cards.iter().filter(|c| c.suspended).count(), 1, "{name}");
        assert_eq!(cards.iter().filter(|c| c.buried_until.is_some()).count(), 1);
        assert_eq!(
            cards
                .iter()
                .filter(|c| c.scheduling.state == CardState::Review)
                .count(),
            4,
            "{name}"
        );

        // Cloze: the gap is hidden on the question, shown on the answer.
        let c1 = cards.iter().find(|c| c.template_key == "c1").unwrap();
        let side = render(&t, c1);
        assert_eq!(side.card_type, builtin::engine::CLOZE);
        assert!(!side.question.contains("Gitmek") && side.answer.contains("Gitmek"));
        // Image occlusion: three masks over the image, one card each.
        let io: Vec<&Card> = cards
            .iter()
            .filter(|c| c.card_type == builtin::engine::IMAGE_OCCLUSION)
            .collect();
        assert_eq!(io.len(), 3, "{name}");
        let q = render(&t, io[1]).question;
        assert!(q.contains("fc-io-active") && q.contains("<img src=\"media:"), "{q}");
        assert_eq!(q.matches("fc-io-mask").count(), 3, "hide all: {q}");
        // Audio.
        let audio = cards
            .iter()
            .find(|c| render(&t, c).question.contains("<audio controls"))
            .expect("audio card");
        assert_eq!(audio.scheduling.state, CardState::Learning);

        // Study a new card: our own review is written next to the imported ones.
        let session = t.ok::<StudyStart>(StartSession {
            kind: StudyKind::Normal,
            mode: Some(StudyMode::Scheduled),
            deck_id: None,
            include_subdecks: None,
            filter: None,
            from_session: None,
            new_limit: None,
            order: None,
            tz_offset_mins: Some(0),
            modes: None,
            scope: None,
        });
        let current = session.current.clone().expect("a card to study");
        t.ok::<StudyAnswer>(AnswerCard {
            session_id: session.id,
            card_id: current.card_id,
            rating: Rating::Good,
            duration_ms: 1_000,
            reveal_ms: Some(400),
        });
        let logs = t
            .store
            .read()
            .unwrap()
            .review_logs_for_card(current.card_id)
            .unwrap();
        assert_eq!(logs.last().unwrap().mode, "normal");
    }
}

/// Anki → us → Anki keeps every field Anki wrote, byte for byte (image occlusion: the image
/// and header; its shapes become rectangles — see `occlusion_export_uses_anki_format`).
#[test]
fn exported_apkg_keeps_real_anki_fields() {
    let t = import_real("real-latest.apkg");
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("out.apkg");
    let report = export(&t, ExportFormat::Apkg, &out);
    let exp = real_json();
    let originals = exp["notes"].as_array().unwrap();
    assert_eq!(report.notes as usize, originals.len());
    let ours = anki::read_package(&std::fs::read(&out).unwrap()).unwrap();
    assert_eq!(ours.notes.len(), originals.len());
    // Notes are written in creation order = Anki's note id order (real.json's order).
    for (orig, mine) in originals.iter().zip(&ours.notes) {
        let fields: Vec<&str> = orig["fields"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        let nt = ours.notetypes.iter().find(|n| n.id == mine.mid).unwrap();
        if occlusion::is_occlusion(nt) {
            assert_eq!(&mine.fields[1..3], &fields[1..3]);
            assert!(mine.fields[0].starts_with("{{c1::image-occlusion:rect:left=0.1:top=0.2"));
        } else {
            assert_eq!(mine.fields, fields, "{}", nt.name);
        }
        let mut tags = mine.tags.clone();
        tags.sort();
        assert_eq!(tags, orig["tags"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect::<Vec<_>>());
    }
    let mut media: Vec<&str> = ours.media.iter().map(|(n, _)| n.as_str()).collect();
    media.sort_unstable();
    assert_eq!(media, ["apple.png", "hello.wav", "map.png"]);
}

#[test]
fn occlusion_export_uses_anki_format() {
    let t = import_real("real.colpkg");
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("all.colpkg");
    export(&t, ExportFormat::Colpkg, &out);
    let col = anki::read_package(&std::fs::read(&out).unwrap()).unwrap();
    let nt = col
        .notetypes
        .iter()
        .find(|n| occlusion::is_occlusion(n))
        .expect("Anki's image occlusion note type");
    assert_eq!(nt.fields, occlusion::FIELDS);
    let note = col.notes.iter().find(|n| n.mid == nt.id).unwrap();
    assert_eq!(note.fields[0].matches("image-occlusion:rect").count(), 3);
    let mut ords: Vec<i64> = col
        .cards
        .iter()
        .filter(|c| c.nid == note.id)
        .map(|c| c.ord)
        .collect();
    ords.sort_unstable();
    assert_eq!(ords, [0, 1, 2]);
    // Re-importing gives the same three masks back.
    let again = TestApp::new(Arc::new(TempSqlite::new()));
    let r = import_file(&again, &out);
    assert_eq!(r.added_cards as usize, all_cards(&t).len());
    let io = all_cards(&again)
        .into_iter()
        .filter(|c| c.card_type == builtin::engine::IMAGE_OCCLUSION)
        .count();
    assert_eq!(io, 3);
}

/// What `tools/anki/check_import.py` must find in real Anki after importing `col` (our file,
/// read back).
fn expectation(col: &AnkiCollection) -> Value {
    let guid: std::collections::HashMap<i64, &str> =
        col.notes.iter().map(|n| (n.id, n.guid.as_str())).collect();
    let deck: std::collections::HashMap<i64, &str> =
        col.decks.iter().map(|d| (d.id, d.name.as_str())).collect();
    let nt_name: std::collections::HashMap<i64, &str> =
        col.notetypes.iter().map(|n| (n.id, n.name.as_str())).collect();
    let occluded: Vec<i64> = col
        .notetypes
        .iter()
        .filter(|n| occlusion::is_occlusion(n))
        .map(|n| n.id)
        .collect();
    json!({
        "crt": col.crt,
        "notes": col.notes.iter().map(|n| json!({
            "guid": n.guid,
            "notetype": nt_name[&n.mid],
            "fields": n.fields,
            "tags": n.tags,
        })).collect::<Vec<_>>(),
        "cards": col.cards.iter().map(|c| {
            let fsrs: Value = serde_json::from_str(&c.data).unwrap_or(Value::Null);
            json!({
                "guid": guid[&c.nid],
                "ord": c.ord,
                "deck": deck[&c.did],
                "type": c.ctype,
                "queue": c.queue,
                "due": c.due,
                "ivl": c.ivl,
                "reps": c.reps,
                "lapses": c.lapses,
                "flags": c.flags & 7,
                "fsrs": if fsrs.get("s").is_none() { Value::Null } else {
                    json!({ "stability": fsrs["s"], "difficulty": fsrs["d"] })
                },
            })
        }).collect::<Vec<_>>(),
        "revlog": col.revlog.len(),
        "media": col.media.iter().map(|(name, bytes)| {
            (name.clone(), Value::String(fc_interop::media::sha256_hex(bytes)))
        }).collect::<serde_json::Map<_, _>>(),
        "occlusion": col.notes.iter().filter(|n| occluded.contains(&n.mid))
            .map(|n| n.guid.clone()).collect::<Vec<_>>(),
    })
}

/// Opens our `.apkg` and `.colpkg` in **real Anki**. Needs Python with the official `anki`
/// package: `python3 -m venv /tmp/ankivenv && /tmp/ankivenv/bin/pip install anki`, then
/// `FC_ANKI_PYTHON=/tmp/ankivenv/bin/python cargo test -p fc-app our_exports_open_in_real_anki
/// -- --ignored --nocapture`. CI runs it as a step of the `rust` job (`anki==26.09.4` in an
/// isolated venv).
#[test]
#[ignore = "needs FC_ANKI_PYTHON (Python with the official anki package)"]
fn our_exports_open_in_real_anki() {
    let python = std::env::var("FC_ANKI_PYTHON").expect("FC_ANKI_PYTHON");
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/anki/check_import.py");
    // A real Anki collection, studied here, plus a note we wrote ourselves.
    let t = import_real("real-latest.apkg");
    let deck = t
        .ok::<DecksList>(Empty {})
        .decks
        .into_iter()
        .find(|d| d.path == "Lang::English")
        .unwrap();
    t.ok::<NotesCreate>(fc_api::v1::notes::CreateNote {
        notetype_id: fc_core::notetype::builtin::CLASSIC_ID,
        deck_id: deck.id,
        fields: [
            ("front".into(), "<b>porsuk</b> & \"kedi\"".into()),
            ("back".into(), "badger".into()),
        ]
        .into(),
        tags: vec!["mine".into()],
        attributes: vec![],
        allow_duplicate: None,
    });
    let _: DeckId = deck.id;
    let dir = tempfile::tempdir().unwrap();
    for (format, file) in [
        (ExportFormat::Apkg, "ours.apkg"),
        (ExportFormat::Colpkg, "ours.colpkg"),
    ] {
        let out = dir.path().join(file);
        export(&t, format, &out);
        let col = anki::read_package(&std::fs::read(&out).unwrap()).unwrap();
        let expect = dir.path().join(format!("{file}.json"));
        std::fs::write(&expect, expectation(&col).to_string()).unwrap();
        let run = std::process::Command::new(&python)
            .arg("-I")
            .arg(&script)
            .arg(&out)
            .arg(&expect)
            .current_dir(dir.path())
            .output()
            .expect("run python");
        let stdout = String::from_utf8_lossy(&run.stdout);
        eprintln!("{file}: {stdout}{}", String::from_utf8_lossy(&run.stderr));
        assert!(run.status.success(), "{file}: real Anki disagrees: {stdout}");
        let report: Value = serde_json::from_str(stdout.trim()).unwrap();
        assert_eq!(report["notes"], col.notes.len());
        assert_eq!(report["cards"], col.cards.len());
        assert_eq!(report["rendered"], col.cards.len());
        assert_eq!(report["occlusion_shapes"], 3);
    }
}
