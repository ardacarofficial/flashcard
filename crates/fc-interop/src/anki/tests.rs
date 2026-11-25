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
    assert_eq!(
        report.skipped_reviews, 0,
        "a manual reschedule is a `manual` row"
    );
    assert!(report.missing_media.is_empty());
    assert_eq!(pack.notes.len(), 4);
    assert_eq!(pack.cards.len(), 5);
    assert_eq!(pack.review_logs.len(), 5);
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

    // Stock note types land on the v0.3 built-ins (matched by structure).
    let ids: Vec<_> = pack.notetypes.iter().map(|n| n.id).collect();
    let builtin = |key: &str| {
        fc_core::content::builtin::ALL
            .iter()
            .find(|b| b.key == key)
            .unwrap()
            .id
    };
    assert_eq!(ids, vec![builtin("classic"), builtin("cloze")]);
    let cloze = &pack.notetypes[1];
    assert_eq!(cloze.kind, NoteTypeKind::Cloze);
    assert_eq!(cloze.templates[0].card_type, "cloze");
    let card_types: HashSet<&str> = pack.cards.iter().map(|c| c.card_type.as_str()).collect();
    assert_eq!(card_types, HashSet::from(["classic", "cloze"]));

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
    // `[sound:x]` becomes v0.3's audio element.
    assert_eq!(
        hello.fields[&fc_core::notetype::FieldKey::new("front")],
        format!(
            "hello <audio controls src=\"{}\"></audio>",
            media_url(&sha256_hex(HELLO))
        )
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
    // Anki flag 7 (purple) → bit 6.
    assert_eq!(dog_card.flags, 1 << 6);
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
    let modes = |p: &Pack| {
        let mut v: Vec<_> = p
            .review_logs
            .iter()
            .map(|r| (r.created_at, r.mode.clone(), r.scheduled_days))
            .collect();
        v.sort();
        v
    };
    assert_eq!(
        modes(&back),
        modes(&pack),
        "manual rows go back as manual rows"
    );
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

#[test]
fn buried_cards_keep_their_state_both_ways() {
    let mut col = sample();
    // Card 1 buried by sibling, card 2 by hand.
    col.cards[0].queue = -2;
    col.cards[1].queue = -3;
    let (pack, _) = to_pack(&col, NOW);
    let buried: Vec<_> = pack
        .cards
        .iter()
        .filter(|c| c.buried_until.is_some())
        .collect();
    assert_eq!(buried.len(), 2);
    assert!(buried
        .iter()
        .all(|c| c.buried_until == Some(Timestamp(NOW.0 + 86_400_000)) && !c.suspended));
    // Back out: buried cards are manually buried (−3) while the bury lasts.
    let out = from_pack(&pack, true, NOW);
    assert_eq!(out.cards.iter().filter(|c| c.queue == -3).count(), 2);
    let later = from_pack(&pack, true, Timestamp(NOW.0 + 2 * 86_400_000));
    assert_eq!(later.cards.iter().filter(|c| c.queue == -3).count(), 0);
    // Without scheduling nothing is buried.
    assert!(from_pack(&pack, false, NOW)
        .cards
        .iter()
        .all(|c| c.queue >= -1));
}

fn notetype(id: i64, kind: i64, fields: &[&str], templates: &[(&str, &str)]) -> AnkiNotetype {
    AnkiNotetype {
        id,
        name: format!("nt{id}"),
        kind,
        sort_field: 0,
        css: String::new(),
        fields: fields.iter().map(|s| s.to_string()).collect(),
        templates: templates
            .iter()
            .map(|(q, a)| AnkiTemplate {
                name: "Card".into(),
                qfmt: (*q).into(),
                afmt: (*a).into(),
            })
            .collect(),
    }
}

#[test]
fn stock_note_types_map_onto_builtins_by_structure() {
    use fc_core::content::builtin::ALL;
    let id_of = |key: &str| ALL.iter().find(|b| b.key == key).unwrap().id;
    let mapped = |a: AnkiNotetype| super::convert::import_notetype_for_tests(&a, NOW);
    // Localized names do not matter; structure does.
    let rev = mapped(notetype(
        1,
        0,
        &["Ön", "Arka"],
        &[
            ("{{Ön}}", "{{FrontSide}}<hr id=answer>{{Arka}}"),
            ("{{Arka}}", "{{FrontSide}}<hr id=answer>{{Ön}}"),
        ],
    ));
    assert_eq!(rev.id, id_of("classic_reverse"));
    let typing = mapped(notetype(
        2,
        0,
        &["Front", "Back"],
        &[(
            "{{Front}}\n\n{{type:Back}}",
            "{{Front}}<hr id=answer>{{type:Back}}",
        )],
    ));
    assert_eq!(typing.id, id_of("typing"));
    let optional = mapped(notetype(
        3,
        0,
        &["Front", "Back", "Add Reverse"],
        &[
            ("{{Front}}", "{{FrontSide}}{{Back}}"),
            (
                "{{#Add Reverse}}{{Back}}{{/Add Reverse}}",
                "{{FrontSide}}{{Front}}",
            ),
        ],
    ));
    assert_eq!(optional.id, id_of("optional_reverse"));
    // Anything else keeps its own fields and templates.
    let custom = mapped(notetype(
        4,
        0,
        &["Word", "Meaning", "Example"],
        &[("{{Word}}", "{{Meaning}}")],
    ));
    assert!(custom.builtin.is_none());
    assert_eq!(custom.fields.len(), 3);
}

#[test]
fn anki_image_occlusion_becomes_v03_masks() {
    let io = notetype(
        1_600_000_000_009,
        1,
        &["Occlusion", "Image", "Header", "Back Extra", "Comments"],
        &[(
            "{{#Header}}{{Header}}{{/Header}}<div>{{cloze:Occlusion}}</div><div id=\"image-occlusion-container\">{{Image}}</div>",
            "{{cloze:Occlusion}}{{Back Extra}}",
        )],
    );
    let png = b"\x89PNG\r\n\x1a\nmap".to_vec();
    let mut col = sample();
    col.notetypes.push(io.clone());
    col.media.push(("map.png".into(), png.clone()));
    let nid = 1_610_000_000_090;
    col.notes.push(AnkiNote {
        id: nid,
        guid: "io-note-1".into(),
        mid: io.id,
        modified: 1_650_000_000,
        tags: vec![],
        fields: vec![
            "{{c1::image-occlusion:rect:left=.1:top=.2:width=.3:height=.1:oi=1}}<br>\
             {{c2::image-occlusion:ellipse:left=.5:top=.5:rx=.1:ry=.05:oi=1}}<br>\
             {{c2::image-occlusion:polygon:points=0.4,0.4 0.45,0.3 0.5,0.45:oi=1}}"
                .into(),
            r#"<img src="map.png">"#.into(),
            "Türkiye".into(),
            "extra".into(),
            "comment".into(),
        ],
    });
    for (i, ord) in [0, 1].iter().enumerate() {
        col.cards.push(AnkiCard {
            id: 1_610_000_000_091 + i as i64,
            nid,
            did: WORDS,
            ord: *ord,
            modified: 1_650_000_000,
            ctype: 0,
            queue: 0,
            due: 1,
            ivl: 0,
            factor: 0,
            reps: 0,
            lapses: 0,
            left: 0,
            odue: 0,
            odid: 0,
            flags: 0,
            data: String::new(),
        });
    }
    let (pack, _) = to_pack(&col, NOW);
    let note = pack
        .notes
        .iter()
        .find(|n| n.sort_field.contains("media:"))
        .unwrap();
    let nt = pack
        .notetypes
        .iter()
        .find(|n| n.id == note.notetype_id)
        .unwrap();
    assert_eq!(nt.builtin.as_deref(), Some("image_occlusion"));
    let field = |k: &str| note.fields[&fc_core::notetype::FieldKey::new(k)].clone();
    assert_eq!(field("image"), media_url(&sha256_hex(&png)));
    assert_eq!(field("header"), "Türkiye");
    let masks: serde_json::Value = serde_json::from_str(&field("masks")).unwrap();
    assert_eq!(masks["mode"], "hide_all");
    let m = masks["masks"].as_array().unwrap();
    assert_eq!(m.len(), 2);
    assert_eq!(
        (m[0]["n"].as_u64(), m[0]["x"].as_f64(), m[0]["w"].as_f64()),
        (Some(1), Some(0.1), Some(0.3))
    );
    // c2: ellipse (0.5–0.7 × 0.5–0.6) ∪ polygon (0.4–0.5 × 0.3–0.45).
    let close = |v: &serde_json::Value, want: f64| (v.as_f64().unwrap() - want).abs() < 1e-9;
    assert!(close(&m[1]["x"], 0.4) && close(&m[1]["y"], 0.3));
    assert!(close(&m[1]["w"], 0.3) && close(&m[1]["h"], 0.3));
    let keys: Vec<_> = pack
        .cards
        .iter()
        .filter(|c| c.note_id == note.id)
        .map(|c| (c.template_key.as_str(), c.card_type.as_str()))
        .collect();
    assert_eq!(
        keys,
        vec![("m1", "image_occlusion"), ("m2", "image_occlusion")]
    );
}

/// v1.4.2: Anki's "set due date" / "forget" rows are imported as `manual` rows (not answers),
/// the last *answer* stays the card's last review (so the interval exported back = Anki's),
/// and they go back to Anki as "Manual" rows with ease 0.
#[test]
fn manual_reschedules_are_kept_but_are_not_answers() {
    let mut col = sample();
    // The review card `c1` (due day 100, interval 15) was answered on day 85, then "set due
    // date" moved it ten days later on day 90: Anki keeps the interval counted from day 85.
    let day = |d: i64| (CRT + d * 86_400) * 1000;
    let c1 = col
        .cards
        .iter_mut()
        .find(|c| c.id == 1_610_000_000_001)
        .unwrap();
    c1.due = 110;
    c1.ivl = 25;
    col.revlog.retain(|r| r.cid != 1_610_000_000_001);
    col.revlog
        .push(rev(day(85), 1_610_000_000_001, 3, 15, 4, 1));
    col.revlog
        .push(rev(day(90), 1_610_000_000_001, 0, 25, 15, 4));
    // A "forget": the card is new again.
    let c5 = col
        .cards
        .iter_mut()
        .find(|c| c.id == 1_610_000_000_032)
        .unwrap();
    (c5.ctype, c5.queue, c5.due, c5.ivl, c5.odid, c5.odue) = (0, 0, 7, 0, 0, 0);
    // Anki marks a reset with ease factor 0 (a "set due date" row carries the card's).
    let mut forget = rev(day(240), 1_610_000_000_032, 0, 0, 10, 4);
    forget.factor = 0;
    col.revlog.push(forget);

    let (pack, report) = to_pack(&col, NOW);
    assert_eq!(report.skipped_reviews, 0);
    let card = pack
        .cards
        .iter()
        .find(|c| {
            pack.review_logs
                .iter()
                .any(|r| r.card_id == c.id && r.created_at.0 == day(85))
        })
        .unwrap();
    assert_eq!(
        card.scheduling.last_review,
        Some(Timestamp(day(85))),
        "the answer, not the manual row"
    );
    let manual: Vec<_> = pack
        .review_logs
        .iter()
        .filter(|r| r.mode == crate::pack::MANUAL_MODE)
        .map(|r| {
            (
                r.created_at.0,
                r.state_before,
                r.state_after,
                r.scheduled_days,
            )
        })
        .collect();
    assert_eq!(
        manual,
        [
            (day(90), CardState::Review, CardState::Review, 25),
            (1_620_700_000_000, CardState::New, CardState::Review, 3),
            (day(240), CardState::Review, CardState::New, 0),
        ]
    );

    let back = from_pack(&pack, true, NOW);
    let exported = back
        .cards
        .iter()
        .find(|c| c.due > 100 && c.ctype == 2)
        .unwrap();
    assert_eq!(
        exported.ivl, 25,
        "Anki's interval: last answer (day 85) to due (day 110)"
    );
    // Ease factor 0 only on the reset: Anki reads a manual row with factor 0 as "forget".
    let mut rows: Vec<(i64, i64, i64, i64)> = back
        .revlog
        .iter()
        .filter(|r| r.ease == 0)
        .map(|r| (r.rtype, r.ivl, r.last_ivl, r.factor))
        .collect();
    rows.sort_unstable();
    assert_eq!(rows, [(4, 0, 10, 0), (4, 3, 3, 2500), (4, 25, 15, 2500)]);

    // A card with nothing but a manual row ("set due date" on a new card) was never
    // answered: no last review time goes to Anki (its "Check Database" would remove it).
    let never_answered = back
        .revlog
        .iter()
        .find(|r| r.ease == 0 && r.ivl == 3)
        .map(|r| r.cid)
        .unwrap();
    let data = |cid: i64| &back.cards.iter().find(|c| c.id == cid).unwrap().data;
    assert!(
        !data(never_answered).contains("lrt"),
        "{}",
        data(never_answered)
    );
    assert!(
        data(exported.id).contains("\"lrt\""),
        "{}",
        data(exported.id)
    );
}

/// v1.4.2: the interval sent to Anki is counted in days (day of the last answer → due day),
/// whatever the time of day of the answer; a relearning card sends the interval it returns to.
#[test]
fn exported_interval_is_in_days_whatever_the_time_of_the_answer() {
    let (mut pack, _) = to_pack(&sample(), NOW);
    let day = 86_400_000_i64;
    let due = Timestamp(NOW.0.div_euclid(day) * day + 10 * day);
    let mut expect = Vec::new();
    let mut hours = [13_i64, 23, 1, 11].into_iter().cycle();
    let mut relearning = false;
    for c in &mut pack.cards {
        let s = &mut c.scheduling;
        if s.state == CardState::Review && !relearning {
            // Lapsed a few minutes ago.
            relearning = true;
            s.state = CardState::Relearning;
            s.last_review = Some(Timestamp(NOW.0 - 60_000));
            s.due = Some(Timestamp(NOW.0 + 540_000));
        }
        match s.state {
            CardState::Review => {
                // Answered `h` hours into the day, seven days before the due day.
                let h = hours.next().unwrap();
                s.due = Some(due);
                s.last_review = Some(Timestamp(due.0 - 7 * day + h * 3_600_000));
                expect.push((2, 7));
            }
            CardState::Relearning => {
                s.stability = Some(5.6);
                expect.push((3, 6));
            }
            _ => {}
        }
    }
    assert!(expect.contains(&(3, 6)) && expect.len() >= 3, "{expect:?}");
    let back = from_pack(&pack, true, NOW);
    let mut got: Vec<(i64, i64)> = back
        .cards
        .iter()
        .filter(|c| matches!(c.ctype, 2 | 3))
        .map(|c| (c.ctype, c.ivl))
        .collect();
    got.sort_unstable();
    expect.sort_unstable();
    assert_eq!(got, expect);
}
