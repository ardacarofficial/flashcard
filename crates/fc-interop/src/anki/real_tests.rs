//! Packages made by **real Anki** (the official `anki` library, 26.09; built by
//! `tools/anki/make_fixtures.py`) — not by our writer. `real.json` is what Anki itself
//! reported about the collection before exporting it; every package must import to exactly
//! that: notes, fields, tags, note types, decks, card states, FSRS memory, flags,
//! suspension/burial, media and review history.

use super::*;
use crate::media::{media_url, sha256_hex};
use crate::pack::{derived_id, Pack};
use fc_core::card::CardState;
use fc_core::content::builtin;
use fc_core::notetype::FieldKey;
use fc_core::Timestamp;
use serde_json::Value;

const NOW: Timestamp = Timestamp(1_800_000_000_000);

fn fixture(name: &str) -> Vec<u8> {
    let path = format!("{}/tests/fixtures/anki/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn expected() -> Value {
    serde_json::from_slice(&fixture("real.json")).unwrap()
}

/// The built-in each of Anki's stock note types must land on (`None` = custom type).
fn builtin_for(anki_name: &str) -> Option<&'static str> {
    match anki_name {
        "Basic" => Some("classic"),
        "Basic (and reversed card)" => Some("classic_reverse"),
        "Basic (optional reversed card)" => Some("optional_reverse"),
        "Basic (type in the answer)" => Some("typing"),
        "Cloze" => Some("cloze"),
        "Image Occlusion" => Some("image_occlusion"),
        _ => None,
    }
}

fn builtin_id(key: &str) -> fc_core::NoteTypeId {
    builtin::ALL
        .iter()
        .find(|b| b.key == key)
        .unwrap_or_else(|| panic!("built-in {key}"))
        .id
}

fn field(note: &fc_core::note::Note, key: &str) -> String {
    note.fields
        .get(&FieldKey::new(key))
        .cloned()
        .unwrap_or_default()
}

/// `media:<sha256>` of a media file in the package, by its Anki name.
fn media_ref(col: &AnkiCollection, name: &str) -> String {
    let (_, bytes) = col
        .media
        .iter()
        .find(|(n, _)| n == name)
        .unwrap_or_else(|| panic!("media {name}"));
    media_url(&sha256_hex(bytes))
}

fn check(name: &str, layout: AnkiLayout, scheduling: bool) -> (AnkiCollection, Pack) {
    let exp = expected();
    let col = read_package(&fixture(name)).unwrap_or_else(|e| panic!("{name}: {e}"));
    assert_eq!(col.layout, layout, "{name}");
    let (pack, report) = to_pack(&col, NOW);
    let notes = exp["notes"].as_array().unwrap();

    // Nothing lost.
    assert_eq!(report.skipped_notes, 0, "{name}");
    assert_eq!(report.skipped_cards, 0, "{name}");
    assert_eq!(report.skipped_tags, 0, "{name}");
    assert!(report.missing_media.is_empty(), "{name}: {report:?}");
    assert_eq!(pack.notes.len(), notes.len(), "{name}");
    let mut names: Vec<&str> = pack
        .media
        .iter()
        .filter_map(|m| m.name.as_deref())
        .collect();
    names.sort_unstable();
    let exp_media: Vec<&str> = exp["media"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(names, exp_media, "{name}");

    // Notes and cards, in Anki's note id order (both sides), cards by template order.
    for (an, note) in notes.iter().zip(&pack.notes) {
        let nt_name = an["notetype"].as_str().unwrap();
        match builtin_for(nt_name) {
            Some(key) => assert_eq!(note.notetype_id, builtin_id(key), "{name}: {nt_name}"),
            None => {
                let nt = pack
                    .notetypes
                    .iter()
                    .find(|n| n.id == note.notetype_id)
                    .unwrap();
                assert_eq!(nt.name, nt_name, "{name}");
            }
        }
        let tags: Vec<&str> = note.tags.iter().map(|t| t.as_str()).collect();
        let exp_tags: Vec<&str> = an["tags"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(tags, exp_tags, "{name}: tags of {nt_name}");
        let cards: Vec<&fc_core::card::Card> =
            pack.cards.iter().filter(|c| c.note_id == note.id).collect();
        let exp_cards = an["cards"].as_array().unwrap();
        assert_eq!(cards.len(), exp_cards.len(), "{name}: cards of {nt_name}");
        for (ac, card) in exp_cards.iter().zip(cards) {
            let what = format!("{name}: {nt_name} card {}", card.template_key);
            let deck = pack.deck_path(card.deck_id).join("::");
            assert_eq!(deck, ac["deck"].as_str().unwrap(), "{what}");
            let s = &card.scheduling;
            if !scheduling {
                assert_eq!(s.state, CardState::New, "{what}");
                assert_eq!((card.flags, card.suspended), (0, false), "{what}");
                assert!(card.buried_until.is_none(), "{what}");
                continue;
            }
            let state = match ac["type"].as_i64().unwrap() {
                0 => CardState::New,
                1 => CardState::Learning,
                2 => CardState::Review,
                _ => CardState::Relearning,
            };
            assert_eq!(s.state, state, "{what}");
            assert_eq!(i64::from(s.reps), ac["reps"].as_i64().unwrap(), "{what}");
            assert_eq!(
                i64::from(s.lapses),
                ac["lapses"].as_i64().unwrap(),
                "{what}"
            );
            let queue = ac["queue"].as_i64().unwrap();
            assert_eq!(card.suspended, queue == -1, "{what}");
            assert_eq!(
                card.buried_until.is_some(),
                matches!(queue, -2 | -3),
                "{what}"
            );
            let flag = ac["flags"].as_i64().unwrap();
            let bit = if flag == 0 { 0 } else { 1 << (flag - 1) };
            assert_eq!(i64::from(card.flags), bit, "{what}");
            match &ac["fsrs"] {
                Value::Null => assert!(s.difficulty.is_none(), "{what}"),
                m => {
                    let close =
                        |a: Option<f64>, b: &Value| (a.unwrap() - b.as_f64().unwrap()).abs() < 1e-3;
                    assert!(close(s.stability, &m["stability"]), "{what}: {s:?}");
                    assert!(close(s.difficulty, &m["difficulty"]), "{what}: {s:?}");
                }
            }
            if state != CardState::New {
                assert!(s.due.is_some() && s.last_review.is_some(), "{what}: {s:?}");
            }
        }
    }

    // History: every Anki review except the manual "set due date" one.
    let reviews = exp["revlog"].as_u64().unwrap() as usize;
    if scheduling {
        assert_eq!(pack.review_logs.len(), reviews - 1, "{name}");
        assert_eq!(report.skipped_reviews, 1, "{name}");
    } else {
        assert!(pack.review_logs.is_empty(), "{name}");
    }
    (col, pack)
}

/// Field contents, with media turned into our references.
fn check_content(col: &AnkiCollection, pack: &Pack) {
    let by_nt = |key: &str| -> Vec<&fc_core::note::Note> {
        pack.notes
            .iter()
            .filter(|n| n.notetype_id == builtin_id(key))
            .collect()
    };
    let apple = by_nt("classic")
        .into_iter()
        .find(|n| field(n, "front") == "apple")
        .unwrap();
    assert_eq!(
        field(apple, "back"),
        format!("elma <img src=\"{}\">", media_ref(col, "apple.png"))
    );
    let hello = pack
        .notes
        .iter()
        .find(|n| field(n, "back") == "merhaba")
        .unwrap();
    assert_eq!(
        field(hello, "front"),
        format!(
            "hello <audio controls src=\"{}\"></audio>",
            media_ref(col, "hello.wav")
        )
    );
    let cloze = by_nt("cloze")[0];
    assert_eq!(
        field(cloze, "text"),
        "{{c1::Gitmek}} to go, {{c2::gelmek}} to come"
    );
    assert_eq!(field(cloze, "extra"), "Back <i>extra</i>");
    let custom = pack
        .notes
        .iter()
        .find(|n| field(n, "word") == "badger")
        .unwrap();
    assert_eq!(field(custom, "example"), "The badger dug a hole.");

    // Image occlusion: rectangle, ellipse and polygon → three masks (bounding boxes).
    let io = by_nt("image_occlusion")[0];
    assert_eq!(field(io, "image"), media_ref(col, "map.png"));
    assert_eq!(field(io, "header"), "Map");
    let masks: Value = serde_json::from_str(&field(io, "masks")).unwrap();
    assert_eq!(masks["mode"], "hide_all");
    let boxes: Vec<(u64, f64, f64, f64, f64)> = masks["masks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| {
            let f = |k: &str| (m[k].as_f64().unwrap() * 1000.0).round() / 1000.0;
            (m["n"].as_u64().unwrap(), f("x"), f("y"), f("w"), f("h"))
        })
        .collect();
    assert_eq!(
        boxes,
        vec![
            (1, 0.1, 0.2, 0.3, 0.4),
            (2, 0.5, 0.1, 0.2, 0.3),
            (3, 0.1, 0.7, 0.2, 0.25)
        ]
    );
    let keys: Vec<&str> = pack
        .cards
        .iter()
        .filter(|c| c.note_id == io.id)
        .map(|c| c.template_key.as_str())
        .collect();
    assert_eq!(keys, ["m1", "m2", "m3"]);
}

#[test]
fn real_anki_latest_apkg_imports_exactly() {
    let (col, pack) = check("real-latest.apkg", AnkiLayout::Latest, true);
    check_content(&col, &pack);
}

#[test]
fn real_anki_legacy_apkg_imports_exactly() {
    let (col, pack) = check("real-legacy.apkg", AnkiLayout::Legacy2, true);
    check_content(&col, &pack);
}

#[test]
fn real_anki_colpkg_imports_exactly() {
    let (col, pack) = check("real.colpkg", AnkiLayout::Latest, true);
    check_content(&col, &pack);
}

#[test]
fn real_anki_apkg_without_scheduling_imports_as_new_cards() {
    let (col, pack) = check("real-noscheduling.apkg", AnkiLayout::Latest, false);
    check_content(&col, &pack);
}

/// The same collection read from each layout converts to the same notes and cards.
#[test]
fn real_anki_layouts_agree() {
    let packs: Vec<Pack> = ["real-latest.apkg", "real-legacy.apkg", "real.colpkg"]
        .iter()
        .map(|n| to_pack(&read_package(&fixture(n)).unwrap(), NOW).0)
        .collect();
    for p in &packs[1..] {
        assert_eq!(p.notes, packs[0].notes);
        assert_eq!(p.cards, packs[0].cards);
        assert_eq!(p.review_logs, packs[0].review_logs);
    }
}

/// Our export of a real Anki collection reads back the same (structure + content), image
/// occlusion included (written in Anki's own format since v1.0).
#[test]
fn real_anki_collection_survives_our_export() {
    let (_, pack) = check("real-latest.apkg", AnkiLayout::Latest, true);
    let bytes = write_package(&from_pack(&pack, true, NOW), NOW).unwrap();
    let col = read_package(&bytes).unwrap();
    let io = col
        .notetypes
        .iter()
        .find(|n| occlusion::is_occlusion(n))
        .unwrap();
    assert_eq!(io.fields, occlusion::FIELDS);
    let (again, report) = to_pack(&col, NOW);
    assert_eq!(report, AnkiImportReport::default());
    // Same notes; deck ids are new Anki ids, so decks compare by path.
    let notes = |p: &Pack| -> Vec<fc_core::note::Note> {
        p.notes
            .iter()
            .map(|n| fc_core::note::Note {
                home_deck_id: fc_core::DeckId::from_uuid(derived_id(
                    0,
                    &p.deck_path(n.home_deck_id).join("::"),
                )),
                ..n.clone()
            })
            .collect()
    };
    assert_eq!(notes(&again), notes(&pack));
    let strip = |p: &Pack| -> Vec<(fc_core::NoteId, String, CardState, u32, bool, u32)> {
        let mut v: Vec<_> = p
            .cards
            .iter()
            .map(|c| {
                (
                    c.note_id,
                    c.template_key.clone(),
                    c.scheduling.state,
                    c.scheduling.reps,
                    c.suspended,
                    c.flags,
                )
            })
            .collect();
        v.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
        v
    };
    assert_eq!(strip(&again), strip(&pack));
    assert_eq!(again.review_logs.len(), pack.review_logs.len());
}
