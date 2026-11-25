//! v1.4.2 (J3): a wider set of packages **made by real Anki** (`tools/anki/wide.py`), checked
//! through the app's own import against what Anki itself reported (`wide-*.json`), and — with
//! `FC_ANKI_PYTHON` — exported again and opened in real Anki: every difference is either
//! fixed or listed in `ACCEPTED` with its reason (docs/16-anki-uyum.md).
//!
//! * `wide-fsrs.apkg` (latest layout, FSRS on): two templates per note type, a three-gap cloze,
//!   sub-decks, hierarchical tags, non-ASCII media names, suspended / buried / flagged cards, a
//!   filtered deck, "set due date" and "forget" rows in the review log;
//! * `wide-sm2.apkg` (schema 11, FSRS off): ease factors, a lapse, a deck options preset.

use super::real_anki_tests::{export, import_file};
use crate::testing::{TempSqlite, TestApp};
use fc_api::v1::common::Empty;
use fc_api::v1::io::ExportFormat;
use fc_api::v1::methods::DecksList;
use fc_core::card::{Card, CardState};
use fc_core::notetype::NoteTypeKind;
use fc_interop::pack::MANUAL_MODE;
use fc_storage::repo::CardFilter;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const DAY_MS: i64 = 86_400_000;

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../fc-interop/tests/fixtures/anki")
        .join(name)
}

fn json(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

/// Our cards keyed like Anki's report (`<note id>/<ord>`: a note's creation time is Anki's
/// note id), with what Anki reports about a card.
fn our_cards(t: &TestApp) -> HashMap<String, Value> {
    let decks: HashMap<_, _> = t
        .ok::<DecksList>(Empty {})
        .decks
        .into_iter()
        .map(|d| (d.id, d.path))
        .collect();
    let store = t.store.read().unwrap();
    let cards: Vec<Card> = store.list_cards(&CardFilter::default()).unwrap();
    let mut out = HashMap::new();
    for c in cards {
        let note = store.get_note(c.note_id).unwrap().unwrap();
        let nt = store.get_notetype(note.notetype_id).unwrap().unwrap();
        let ord = match nt.kind {
            NoteTypeKind::Cloze => c.template_key[1..].parse::<i64>().unwrap() - 1,
            _ => nt.templates.iter().position(|x| x.key == c.template_key).unwrap() as i64,
        };
        let logs = store.review_logs_for_card(c.id).unwrap();
        let s = &c.scheduling;
        let ivl = match (s.state, s.due, s.last_review) {
            (CardState::Review, Some(d), Some(l)) => (d.0 - l.0 + DAY_MS / 2).div_euclid(DAY_MS),
            _ => 0,
        };
        let key = format!("{}/{ord}", note.created_at.0);
        out.insert(
            key,
            serde_json::json!({
                "deck": decks[&c.deck_id],
                "state": format!("{:?}", s.state),
                "ivl": ivl,
                "reps": s.reps,
                "lapses": s.lapses,
                "flags": c.flags,
                "suspended": c.suspended,
                "buried": c.buried_until.is_some(),
                "fsrs": s.difficulty.map(|d| [
                    (s.stability.unwrap() * 1000.0).round() / 1000.0,
                    (d * 1000.0).round() / 1000.0,
                ]),
                "revlog": logs.len(),
                "manual": logs.iter().filter(|l| l.mode == MANUAL_MODE).count(),
            }),
        );
    }
    out
}

/// What Anki's report says, in `our_cards`' shape.
fn anki_cards(report: &Value) -> HashMap<String, Value> {
    let mut out = HashMap::new();
    for (key, c) in report["cards"].as_object().unwrap() {
        let state = match c["type"].as_i64().unwrap() {
            1 => "Learning",
            2 => "Review",
            3 => "Relearning",
            _ => "New",
        };
        let queue = c["queue"].as_i64().unwrap();
        let revlog = c["revlog"].as_array().unwrap();
        out.insert(
            key.clone(),
            serde_json::json!({
                "deck": c["deck"],
                "state": state,
                "ivl": if state == "Review" { c["ivl"].as_i64().unwrap() } else { 0 },
                "reps": c["reps"],
                "lapses": c["lapses"],
                // Anki's flag number → our bit per flag.
                "flags": match c["flags"].as_i64().unwrap() { 0 => 0, n => 1 << (n - 1) },
                "suspended": queue == -1,
                "buried": queue == -2 || queue == -3,
                "fsrs": c["fsrs"].as_object().map(|f| [f["stability"].clone(), f["difficulty"].clone()]),
                "revlog": revlog.len(),
                "manual": revlog.iter().filter(|r| r[1] == 0).count(),
            }),
        );
    }
    out
}

/// Imports a wide package through the app and compares every card with Anki's own report.
fn import_and_compare(name: &str) -> TestApp {
    let t = TestApp::new(Arc::new(TempSqlite::new()));
    let report = json(&fixture(&format!("{name}.json")));
    let r = import_file(&t, &fixture(&format!("{name}.apkg")));
    assert!(r.issues.is_empty(), "{name}: {:?}", r.issues);
    assert_eq!(r.added_notes as usize, report["notes"].as_object().unwrap().len());
    assert_eq!(r.added_reviews as u64, report["revlog_total"].as_u64().unwrap(), "{name}: every row, manual ones too");
    assert_eq!(r.added_media as usize, report["media"].as_object().unwrap().len());
    let ours = our_cards(&t);
    let anki = anki_cards(&report);
    assert_eq!(ours.len(), anki.len(), "{name}: {:?}", ours.keys().collect::<Vec<_>>());
    for (key, a) in &anki {
        let o = ours.get(key).unwrap_or_else(|| panic!("{name}: card {key} missing"));
        assert_eq!(o, a, "{name}: card {key}");
    }
    // Tags (hierarchy kept) and media names (non-ASCII) as Anki wrote them.
    let store = t.store.read().unwrap();
    let mut tags: Vec<String> = store
        .list_cards(&CardFilter::default())
        .unwrap()
        .iter()
        .flat_map(|c| store.get_note(c.note_id).unwrap().unwrap().tags)
        .map(|t| t.as_str().to_owned())
        .collect();
    tags.sort();
    tags.dedup();
    let mut exp_tags: Vec<String> = report["notes"]
        .as_object()
        .unwrap()
        .values()
        .flat_map(|n| n["tags"].as_array().unwrap().iter().map(|t| t.as_str().unwrap().to_owned()))
        .collect();
    exp_tags.sort();
    exp_tags.dedup();
    assert_eq!(tags, exp_tags, "{name}");
    drop(store);
    t
}

#[test]
fn wide_real_anki_packages_import_like_anki_reports_them() {
    let t = import_and_compare("wide-fsrs");
    // Media with non-ASCII names keep their names (export writes them back under them).
    let dir = tempfile::tempdir().unwrap();
    let out = dir.path().join("back.apkg");
    export(&t, ExportFormat::Apkg, &out);
    let back = fc_interop::anki::read_package(&std::fs::read(&out).unwrap()).unwrap();
    let mut names: Vec<&str> = back.media.iter().map(|(n, _)| n.as_str()).collect();
    names.sort_unstable();
    assert_eq!(names, ["çiçek.png", "şarkı ğ.wav"]);
    // "Set due date" / "forget" go back to Anki as manual rows.
    assert_eq!(back.revlog.iter().filter(|r| r.ease == 0 && r.rtype == 4).count(), 3);
    import_and_compare("wide-sm2");
}

/// Differences between Anki's report of its own package and of our export of it that are
/// accepted, with the reason (docs/16-anki-uyum.md). Each entry is a prefix of
/// `tools/anki/wide.py compare` lines after the card key.
const ACCEPTED: &[(&str, &str)] = &[
    // SM-2 ease is not kept: our scheduler is FSRS; Anki gets its default 250 %.
    ("factor ", "SM-2 ease factor is not stored"),
    // Filtered decks are not carried: the card comes back in its home deck.
    ("filtered ", "filtered decks are not imported"),
    ("deck Filtre", "filtered decks are not imported"),
    // A day-learning card (Anki queue 3) goes back as an intraday learning card (queue 1)
    // due at the same moment.
    ("queue 3 -> 1", "learning due counted in seconds"),
];

fn accepted(diff: &str) -> bool {
    let tail = diff.split_once(": ").map_or(diff, |(_, t)| t);
    ACCEPTED.iter().any(|(p, _)| tail.starts_with(p) || diff.starts_with(p))
}

/// Anki → fc → Anki: our export of each wide package (plus a generated large review history)
/// opened in **real Anki**, compared with Anki's report of its own package. Needs
/// `FC_ANKI_PYTHON` (see `real_anki_tests::our_exports_open_in_real_anki`).
#[test]
#[ignore = "needs FC_ANKI_PYTHON (Python with the official anki package)"]
fn wide_packages_round_trip_through_real_anki() {
    let python = std::env::var("FC_ANKI_PYTHON").expect("FC_ANKI_PYTHON");
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/anki/wide.py");
    let dir = tempfile::tempdir().unwrap();
    let run = |args: &[&str]| {
        let out = std::process::Command::new(&python)
            .arg("-I")
            .arg(&script)
            .args(args)
            .current_dir(dir.path())
            .output()
            .expect("run python");
        assert!(out.status.success(), "{args:?}: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).into_owned()
    };
    // Fresh packages, today (due dates are compared relative to Anki's today), with a large
    // review history: 400 cards × 8 answers.
    let made = dir.path().join("made");
    run(&["make", made.to_str().unwrap(), "--large", "400"]);
    for name in ["wide-fsrs", "wide-sm2"] {
        let t = TestApp::new(Arc::new(TempSqlite::new()));
        let r = import_file(&t, &made.join(format!("{name}.apkg")));
        let anki = made.join(format!("{name}.json"));
        assert_eq!(r.added_reviews as u64, json(&anki)["revlog_total"].as_u64().unwrap(), "{name}");
        let ours = dir.path().join(format!("{name}-ours.apkg"));
        export(&t, ExportFormat::Apkg, &ours);
        let report = dir.path().join(format!("{name}-ours.json"));
        run(&["report", ours.to_str().unwrap(), report.to_str().unwrap()]);
        let diffs: Vec<String> =
            serde_json::from_str(&run(&["compare", anki.to_str().unwrap(), report.to_str().unwrap()]))
                .unwrap();
        eprintln!("{name}: {} differences\n{}", diffs.len(), diffs.join("\n"));
        let unexpected: Vec<&String> = diffs.iter().filter(|d| !accepted(d)).collect();
        assert!(unexpected.is_empty(), "{name}: unexpected differences {unexpected:#?}");
    }
}
