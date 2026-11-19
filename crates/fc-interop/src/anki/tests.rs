//! Anki fixtures are built synthetically (no downloads): the same collection is written in the
//! three package layouts and must read back identically, convert losslessly, and survive an
//! export → import round trip.

use super::fixtures::*;
use super::read::open_db;
use super::write::field_checksum;
use super::*;
use crate::media::{media_url, references, sha256_hex};
use crate::pack::Pack;
use crate::zipio::{ZipIn, ZipOut};
use fc_core::card::{CardState, Rating};
use fc_core::notetype::NoteTypeKind;
use fc_core::Timestamp;
use std::collections::{HashMap, HashSet};

#[test]
fn all_layouts_read_the_same_collection() {
    let col = sample();
    let legacy1 = read_package(&write_package(&col, NOW).unwrap()).unwrap();
    let legacy2 = read_package(&legacy2_package(&col)).unwrap();
    let latest = read_package(&latest_package(&col)).unwrap();
    assert_eq!(legacy1.layout, AnkiLayout::Legacy1);
    assert_eq!(legacy2.layout, AnkiLayout::Legacy2);
    assert_eq!(latest.layout, AnkiLayout::Latest);
    let strip = |c: AnkiCollection| AnkiCollection {
        layout: AnkiLayout::Legacy1,
        ..c
    };
    assert_eq!(strip(legacy1.clone()), col);
    assert_eq!(strip(legacy2), col);
    assert_eq!(strip(latest), col);
}

fn by_sort<'a>(pack: &'a Pack, text: &str) -> &'a fc_core::note::Note {
    pack.notes
        .iter()
        .find(|n| n.sort_field.contains(text))
        .unwrap_or_else(|| panic!("note {text}"))
}

#[test]
fn converts_cloze_image_sound_scheduling_and_history() {
    let (pack, report) = to_pack(&sample(), NOW);
    assert_eq!(report.skipped_notes, 0);
    assert_eq!(report.skipped_cards, 0);
    assert_eq!(report.skipped_reviews, 1, "manual reschedule");
    assert!(report.missing_media.is_empty());
    assert_eq!(pack.notes.len(), 4);
    assert_eq!(pack.cards.len(), 5);
    assert_eq!(pack.review_logs.len(), 4);
    assert_eq!(pack.media.len(), 2);

    // Decks: only those holding cards (+ ancestors); filtered decks resolve to the original.
    let paths: Vec<String> = pack
        .decks
        .iter()
        .map(|d| pack.deck_path(d.id).join("::"))
        .collect();
    assert_eq!(paths, vec!["Lang", "Lang::Words"]);
    let words = pack.decks[1].id;
    assert_eq!(pack.decks[1].description, "Everyday words");

    // Note types: keys, templates rewritten to keys, cloze kind.
    let basic = pack.notetypes.iter().find(|n| n.name == "Basic").unwrap();
    let keys: Vec<&str> = basic.fields.iter().map(|f| f.key.as_str()).collect();
    assert_eq!(keys, vec!["front", "back"]);
    assert_eq!(basic.templates[0].front, "{{front}}");
    assert_eq!(
        basic.templates[0].back,
        "{{FrontSide}}\n\n<hr id=answer>\n\n{{back}}"
    );
    assert_eq!(basic.css, ".card { font-family: arial; }");
    let cloze = pack.notetypes.iter().find(|n| n.name == "Cloze").unwrap();
    assert_eq!(cloze.kind, NoteTypeKind::Cloze);
    assert_eq!(
        cloze.templates[0].back,
        "{{cloze:text}}<br>\n{{back_extra}}"
    );

    // Fields, tags, media.
    let dog = by_sort(&pack, "dog");
    assert_eq!(
        dog.fields[&fc_core::notetype::FieldKey::new("back")],
        "köpek"
    );
    let tags: Vec<&str> = dog.tags.iter().map(|t| t.as_str()).collect();
    assert_eq!(tags, vec!["animals", "lang::tr"]);
    let cat = pack
        .notes
        .iter()
        .find(|n| n.fields.values().any(|v| v == "kedi"))
        .unwrap();
    let front = &cat.fields[&fc_core::notetype::FieldKey::new("front")];
    assert_eq!(
        *front,
        format!("<img src=\"{}\">", media_url(&sha256_hex(CAT)))
    );
    let hello = by_sort(&pack, "hello");
    assert_eq!(
        references(&hello.fields[&fc_core::notetype::FieldKey::new("front")]),
        vec![media_url(&sha256_hex(HELLO))]
    );
    let geo = pack
        .notes
        .iter()
        .find(|n| n.notetype_id == cloze.id)
        .unwrap();
    assert_eq!(
        geo.fields[&fc_core::notetype::FieldKey::new("text")],
        "{{c1::Ankara}} is the capital of {{c2::Türkiye}}"
    );
    assert!(pack
        .media
        .iter()
        .any(|m| m.bytes == CAT && m.mime == "image/jpeg"));

    // Scheduling.
    let dog_card = pack.cards.iter().find(|c| c.note_id == dog.id).unwrap();
    let s = &dog_card.scheduling;
    assert_eq!(s.state, CardState::Review);
    assert_eq!(s.due, Some(Timestamp((CRT + 100 * 86_400) * 1000)));
    assert_eq!(s.stability, Some(15.0));
    assert_eq!((s.reps, s.lapses), (5, 1));
    assert_eq!(s.last_review, Some(Timestamp(1_620_500_000_000)));
    assert_eq!(dog_card.flags, 1);
    let hello_card = pack.cards.iter().find(|c| c.note_id == hello.id).unwrap();
    assert!(hello_card.suspended);
    let geo_cards: Vec<_> = pack.cards.iter().filter(|c| c.note_id == geo.id).collect();
    let keys: Vec<&str> = geo_cards.iter().map(|c| c.template_key.as_str()).collect();
    assert_eq!(keys, vec!["c1", "c2"]);
    assert_eq!(geo_cards[0].scheduling.state, CardState::Learning);
    assert_eq!(
        geo_cards[0].scheduling.due,
        Some(Timestamp(1_600_100_000_000))
    );
    let filtered = geo_cards[1];
    assert_eq!(filtered.deck_id, words);
    assert_eq!(
        filtered.scheduling.due,
        Some(Timestamp((CRT + 120 * 86_400) * 1000))
    );
    assert_eq!(filtered.scheduling.stability, Some(10.5));
    assert_eq!(filtered.scheduling.difficulty, Some(5.25));

    // History.
    let dog_reviews: Vec<_> = pack
        .review_logs
        .iter()
        .filter(|r| r.card_id == dog_card.id)
        .collect();
    let ratings: Vec<Rating> = dog_reviews.iter().map(|r| r.rating).collect();
    assert_eq!(ratings, vec![Rating::Good, Rating::Good, Rating::Again]);
    assert_eq!(dog_reviews[2].state_before, CardState::Review);
    assert_eq!(dog_reviews[2].state_after, CardState::Relearning);
    assert_eq!(dog_reviews[0].scheduled_days, 0);

    // Re-converting yields the same ids (re-import finds the same notes).
    let (again, _) = to_pack(&sample(), Timestamp(NOW.0 + 1));
    assert_eq!(
        again.notes.iter().map(|n| n.id).collect::<Vec<_>>(),
        pack.notes.iter().map(|n| n.id).collect::<Vec<_>>()
    );
}

#[test]
fn export_then_import_roundtrips() {
    let (pack, _) = to_pack(&sample(), NOW);
    let bytes = write_package(&from_pack(&pack, true, NOW), NOW).unwrap();
    let (back, report) = to_pack(&read_package(&bytes).unwrap(), NOW);
    assert_eq!(report, AnkiImportReport::default());
    assert_eq!(back.notes.len(), pack.notes.len());
    for n in &pack.notes {
        // Our note id travels as the Anki guid.
        let m = back
            .notes
            .iter()
            .find(|m| m.id == n.id)
            .expect("same note id");
        assert_eq!(m.fields, n.fields);
        assert_eq!(m.tags, n.tags);
    }
    let shape = |p: &Pack| {
        let mut v: Vec<_> = p
            .notetypes
            .iter()
            .map(|t| {
                (
                    t.name.clone(),
                    t.kind,
                    t.fields.clone(),
                    t.templates.clone(),
                    t.css.clone(),
                )
            })
            .collect();
        v.sort_by(|a, b| a.0.cmp(&b.0));
        v
    };
    assert_eq!(shape(&back), shape(&pack));
    let card_shape = |p: &Pack| {
        let mut v: Vec<_> = p
            .cards
            .iter()
            .map(|c| {
                let s = &c.scheduling;
                (
                    c.note_id,
                    c.template_key.clone(),
                    p.deck_path(c.deck_id),
                    s.state,
                    s.due.map(|d| d.0.div_euclid(86_400_000)),
                    s.stability,
                    s.difficulty,
                    s.reps,
                    s.lapses,
                    c.flags,
                    c.suspended,
                )
            })
            .collect();
        v.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
        v
    };
    assert_eq!(card_shape(&back), card_shape(&pack));
    let ratings = |p: &Pack| {
        let mut v: Vec<_> = p
            .review_logs
            .iter()
            .map(|r| (r.created_at, r.rating))
            .collect();
        v.sort_by_key(|x| x.0);
        v
    };
    assert_eq!(ratings(&back), ratings(&pack));
    let mut media: Vec<_> = back
        .media
        .iter()
        .map(|m| (m.hash.clone(), m.name.clone()))
        .collect();
    media.sort();
    let mut orig: Vec<_> = pack
        .media
        .iter()
        .map(|m| (m.hash.clone(), m.name.clone()))
        .collect();
    orig.sort();
    assert_eq!(media, orig);

    // Without scheduling every card is new and there is no history.
    let plain = write_package(&from_pack(&pack, false, NOW), NOW).unwrap();
    let (plain, _) = to_pack(&read_package(&plain).unwrap(), NOW);
    assert!(plain
        .cards
        .iter()
        .all(|c| c.scheduling.state == CardState::New && c.scheduling.reps == 0));
    assert!(plain.review_logs.is_empty());
}

/// What Anki needs to open a legacy package: the schema-11 tables and columns, `ver = 11`,
/// complete model/deck/config JSON, consistent references, checksums and media list.
#[test]
fn exported_package_is_a_valid_anki_collection() {
    let (pack, _) = to_pack(&sample(), NOW);
    let bytes = write_package(&from_pack(&pack, true, NOW), NOW).unwrap();
    let mut zip = ZipIn::open(&bytes, "apkg").unwrap();
    let db = open_db(&zip.require("collection.anki2").unwrap()).unwrap();

    let columns = |t: &str| -> Vec<String> {
        let mut st = db.prepare(&format!("PRAGMA table_info({t})")).unwrap();
        st.query_map([], |r| r.get::<_, String>(1))
            .unwrap()
            .map(Result::unwrap)
            .collect()
    };
    assert_eq!(
        columns("col"),
        [
            "id", "crt", "mod", "scm", "ver", "dty", "usn", "ls", "conf", "models", "decks",
            "dconf", "tags"
        ]
    );
    assert_eq!(
        columns("notes"),
        ["id", "guid", "mid", "mod", "usn", "tags", "flds", "sfld", "csum", "flags", "data"]
    );
    assert_eq!(
        columns("cards"),
        [
            "id", "nid", "did", "ord", "mod", "usn", "type", "queue", "due", "ivl", "factor",
            "reps", "lapses", "left", "odue", "odid", "flags", "data"
        ]
    );
    assert_eq!(
        columns("revlog"),
        ["id", "cid", "usn", "ease", "ivl", "lastIvl", "factor", "time", "type"]
    );
    assert_eq!(columns("graves"), ["usn", "oid", "type"]);

    let (ver, models, decks, dconf, conf): (i64, String, String, String, String) = db
        .query_row("SELECT ver, models, decks, dconf, conf FROM col", [], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })
        .unwrap();
    assert_eq!(ver, 11);
    let models: serde_json::Value = serde_json::from_str(&models).unwrap();
    let decks: serde_json::Value = serde_json::from_str(&decks).unwrap();
    let dconf: serde_json::Value = serde_json::from_str(&dconf).unwrap();
    let conf: serde_json::Value = serde_json::from_str(&conf).unwrap();
    assert!(conf["curModel"].is_i64());
    assert!(dconf["1"]["new"]["delays"].is_array());
    assert_eq!(decks["1"]["name"], "Default");
    let mut model_fields: HashMap<i64, (usize, usize, i64)> = HashMap::new();
    for (k, m) in models.as_object().unwrap() {
        for key in [
            "id",
            "name",
            "type",
            "mod",
            "usn",
            "sortf",
            "tmpls",
            "flds",
            "css",
            "latexPre",
            "latexPost",
            "req",
        ] {
            assert!(m.get(key).is_some(), "model {k} lacks {key}");
        }
        assert_eq!(m["id"].as_i64().unwrap().to_string(), *k);
        for t in m["tmpls"].as_array().unwrap() {
            for key in ["name", "ord", "qfmt", "afmt", "bqfmt", "bafmt"] {
                assert!(t.get(key).is_some());
            }
        }
        model_fields.insert(
            m["id"].as_i64().unwrap(),
            (
                m["flds"].as_array().unwrap().len(),
                m["tmpls"].as_array().unwrap().len(),
                m["type"].as_i64().unwrap(),
            ),
        );
    }
    for (k, d) in decks.as_object().unwrap() {
        for key in [
            "id",
            "name",
            "mod",
            "usn",
            "desc",
            "dyn",
            "conf",
            "collapsed",
            "newToday",
            "revToday",
            "lrnToday",
            "timeToday",
        ] {
            assert!(d.get(key).is_some(), "deck {k} lacks {key}");
        }
    }
    // Templates name existing fields.
    let fields_of: Vec<String> = models["1600000000001"]["flds"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|f| f["name"].as_str().unwrap().to_owned())
                .collect()
        })
        .unwrap_or_default();
    if !fields_of.is_empty() {
        assert_eq!(fields_of, ["Front", "Back"]);
    }

    let media: serde_json::Value = serde_json::from_slice(&zip.require("media").unwrap()).unwrap();
    let names: HashSet<String> = media
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| {
            assert!(zip.has(k), "media entry {k}");
            v.as_str().unwrap().to_owned()
        })
        .collect();

    let mut guids = HashSet::new();
    let mut st = db
        .prepare("SELECT id, guid, mid, flds, csum FROM notes")
        .unwrap();
    let notes: Vec<(i64, String, i64, String, i64)> = st
        .query_map([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    let note_ids: HashSet<i64> = notes.iter().map(|n| n.0).collect();
    for (_, guid, mid, flds, csum) in &notes {
        assert!(guids.insert(guid.clone()));
        let (nfields, _, _) = model_fields[mid];
        let parts: Vec<&str> = flds.split('\x1f').collect();
        assert_eq!(parts.len(), nfields);
        assert_eq!(*csum, field_checksum(parts[0]));
        for r in references(flds) {
            assert!(names.contains(&r), "media {r} listed");
        }
    }
    let deck_ids: HashSet<i64> = decks
        .as_object()
        .unwrap()
        .values()
        .map(|d| d["id"].as_i64().unwrap())
        .collect();
    let mut st = db
        .prepare(
            "SELECT c.id, c.nid, c.did, c.ord, n.mid FROM cards c JOIN notes n ON n.id = c.nid",
        )
        .unwrap();
    let cards: Vec<(i64, i64, i64, i64, i64)> = st
        .query_map([], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
        })
        .unwrap()
        .map(Result::unwrap)
        .collect();
    assert_eq!(cards.len(), 5);
    let card_ids: HashSet<i64> = cards.iter().map(|c| c.0).collect();
    for (_, nid, did, ord, mid) in cards {
        assert!(note_ids.contains(&nid));
        assert!(deck_ids.contains(&did));
        let (_, ntmpl, kind) = model_fields[&mid];
        if kind == 0 {
            assert!((ord as usize) < ntmpl);
        }
    }
    let mut st = db.prepare("SELECT cid FROM revlog").unwrap();
    for cid in st.query_map([], |r| r.get::<_, i64>(0)).unwrap() {
        assert!(card_ids.contains(&cid.unwrap()));
    }
}

#[test]
fn rejects_non_packages() {
    assert!(read_package(b"nope").is_err());
    let mut zip = ZipOut::new("apkg");
    zip.add("readme.txt", b"x", true).unwrap();
    assert!(read_package(&zip.finish().unwrap()).is_err());
}
