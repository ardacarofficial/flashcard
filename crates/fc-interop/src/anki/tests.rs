//! Anki fixtures are built synthetically (no downloads): the same collection is written in the
//! three package layouts and must read back identically, convert losslessly, and survive an
//! export → import round trip.

use super::proto::Enc;
use super::read::open_db;
use super::schema::{SCHEMA11, SCHEMA18_TABLES};
use super::write::{collection_bytes, field_checksum};
use super::*;
use crate::media::{media_url, references, sha256_hex};
use crate::pack::Pack;
use crate::zipio::{ZipIn, ZipOut};
use fc_core::card::{CardState, Rating};
use fc_core::notetype::NoteTypeKind;
use fc_core::Timestamp;
use rusqlite::{params, Connection};
use std::collections::{HashMap, HashSet};

const CRT: i64 = 1_600_000_000;
const NOW: Timestamp = Timestamp(1_700_000_000_000);
const BASIC: i64 = 1_600_000_000_001;
const CLOZE: i64 = 1_600_000_000_002;
const WORDS: i64 = 1_600_000_000_100;
const LANG: i64 = 1_600_000_000_101;
const FILTERED: i64 = 1_600_000_000_200;
const CAT: &[u8] = b"\xff\xd8\xff\xe0JPEG cat";
const HELLO: &[u8] = b"ID3 hello mp3";

fn card(id: i64, nid: i64, did: i64, ord: i64) -> AnkiCard {
    AnkiCard {
        id,
        nid,
        did,
        ord,
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
    }
}

fn rev(id: i64, cid: i64, ease: i64, ivl: i64, last: i64, rtype: i64) -> AnkiRevlog {
    AnkiRevlog {
        id,
        cid,
        ease,
        ivl,
        last_ivl: last,
        factor: 2500,
        time: 4200,
        rtype,
    }
}

/// Cloze, image and sound notes, every card state, a filtered deck and review history.
pub(crate) fn sample() -> AnkiCollection {
    let note = |id: i64, guid: &str, mid: i64, tags: &[&str], fields: &[&str]| AnkiNote {
        id,
        guid: guid.into(),
        mid,
        modified: 1_650_000_000,
        tags: tags.iter().map(|s| s.to_string()).collect(),
        fields: fields.iter().map(|s| s.to_string()).collect(),
    };
    let mut c1 = card(1_610_000_000_001, 1_610_000_000_000, WORDS, 0);
    (
        c1.ctype, c1.queue, c1.due, c1.ivl, c1.factor, c1.reps, c1.lapses, c1.flags,
    ) = (2, 2, 100, 15, 2500, 5, 1, 1);
    let c2 = card(1_610_000_000_011, 1_610_000_000_010, WORDS, 0);
    let mut c3 = card(1_610_000_000_021, 1_610_000_000_020, LANG, 0);
    (c3.ctype, c3.queue, c3.due, c3.ivl, c3.factor, c3.reps) = (2, -1, 50, 3, 2300, 2);
    let mut c4 = card(1_610_000_000_031, 1_610_000_000_030, WORDS, 0);
    (c4.ctype, c4.queue, c4.due, c4.reps, c4.left) = (1, 1, 1_600_100_000, 1, 2);
    let mut c5 = card(1_610_000_000_032, 1_610_000_000_030, FILTERED, 1);
    (
        c5.ctype, c5.queue, c5.due, c5.ivl, c5.reps, c5.odid, c5.odue,
    ) = (2, 2, 999, 10, 3, WORDS, 120);
    c5.data = r#"{"s":10.5,"d":5.25}"#.into();
    AnkiCollection {
        layout: AnkiLayout::Legacy1,
        crt: CRT,
        notetypes: vec![
            AnkiNotetype {
                id: BASIC,
                name: "Basic".into(),
                kind: 0,
                sort_field: 0,
                css: ".card { font-family: arial; }".into(),
                fields: vec!["Front".into(), "Back".into()],
                templates: vec![AnkiTemplate {
                    name: "Card 1".into(),
                    qfmt: "{{Front}}".into(),
                    afmt: "{{FrontSide}}\n\n<hr id=answer>\n\n{{Back}}".into(),
                }],
            },
            AnkiNotetype {
                id: CLOZE,
                name: "Cloze".into(),
                kind: 1,
                sort_field: 0,
                css: ".cloze { color: blue; }".into(),
                fields: vec!["Text".into(), "Back Extra".into()],
                templates: vec![AnkiTemplate {
                    name: "Cloze".into(),
                    qfmt: "{{cloze:Text}}".into(),
                    afmt: "{{cloze:Text}}<br>\n{{Back Extra}}".into(),
                }],
            },
        ],
        decks: vec![
            AnkiDeck {
                id: 1,
                name: "Default".into(),
                description: String::new(),
                filtered: false,
            },
            AnkiDeck {
                id: WORDS,
                name: "Lang::Words".into(),
                description: "Everyday words".into(),
                filtered: false,
            },
            AnkiDeck {
                id: LANG,
                name: "Lang".into(),
                description: String::new(),
                filtered: false,
            },
            AnkiDeck {
                id: FILTERED,
                name: "Filtered Deck 1".into(),
                description: String::new(),
                filtered: true,
            },
        ],
        notes: vec![
            note(
                1_610_000_000_000,
                "aB3$xyZ!q",
                BASIC,
                &["animals", "lang::tr"],
                &["dog", "köpek"],
            ),
            note(
                1_610_000_000_010,
                "c&T9;kLm0p",
                BASIC,
                &["image"],
                &[r#"<img src="cat.jpg">"#, "kedi"],
            ),
            note(
                1_610_000_000_020,
                "Hh2#nn8&1z",
                BASIC,
                &[],
                &["hello [sound:hello.mp3]", "merhaba"],
            ),
            note(
                1_610_000_000_030,
                "Qq0_cloze1",
                CLOZE,
                &["geo"],
                &["{{c1::Ankara}} is the capital of {{c2::Türkiye}}", "geo"],
            ),
        ],
        cards: vec![c1, c2, c3, c4, c5],
        revlog: vec![
            rev(1_620_000_000_000, 1_610_000_000_001, 3, -600, 0, 0),
            rev(1_620_100_000_000, 1_610_000_000_001, 3, 4, 0, 0),
            rev(1_620_500_000_000, 1_610_000_000_001, 1, 15, 4, 1),
            rev(1_620_600_000_000, 1_610_000_000_032, 4, 10, 3, 1),
            rev(1_620_700_000_000, 1_610_000_000_021, 0, 3, 3, 4),
        ],
        media: vec![
            ("cat.jpg".into(), CAT.to_vec()),
            ("hello.mp3".into(), HELLO.to_vec()),
        ],
    }
}

fn legacy2_package(col: &AnkiCollection) -> Vec<u8> {
    let mut zip = ZipOut::new("apkg");
    let db = collection_bytes(col, NOW).unwrap();
    // Anki 2.1 writes a stub collection.anki2 next to the real collection.anki21.
    let stub = collection_bytes(
        &AnkiCollection {
            notes: vec![],
            cards: vec![],
            revlog: vec![],
            media: vec![],
            ..col.clone()
        },
        NOW,
    )
    .unwrap();
    zip.add("collection.anki2", &stub, true).unwrap();
    zip.add("collection.anki21", &db, true).unwrap();
    let list: serde_json::Map<String, serde_json::Value> = col
        .media
        .iter()
        .enumerate()
        .map(|(i, (n, _))| (i.to_string(), n.clone().into()))
        .collect();
    zip.add(
        "media",
        serde_json::Value::Object(list).to_string().as_bytes(),
        true,
    )
    .unwrap();
    for (i, (_, b)) in col.media.iter().enumerate() {
        zip.add(&i.to_string(), b, false).unwrap();
    }
    zip.finish().unwrap()
}

fn zstd(b: &[u8]) -> Vec<u8> {
    zstd::stream::encode_all(b, 3).unwrap()
}

/// The schema-18 package layout (`collection.anki21b`, protobuf configs, zstd everywhere).
fn latest_package(col: &AnkiCollection) -> Vec<u8> {
    let mut conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(SCHEMA11).unwrap();
    conn.execute_batch(SCHEMA18_TABLES).unwrap();
    conn.execute(
        "INSERT INTO col VALUES (1, ?1, 0, 0, 18, 0, 0, 0, '', '', '', '', '')",
        [col.crt],
    )
    .unwrap();
    for nt in &col.notetypes {
        let cfg = Enc::default()
            .varint(1, nt.kind as u64)
            .varint(2, nt.sort_field as u64)
            .string(3, &nt.css)
            .0;
        conn.execute(
            "INSERT INTO notetypes VALUES (?1, ?2, 0, 0, ?3)",
            params![nt.id, nt.name, cfg],
        )
        .unwrap();
        for (i, f) in nt.fields.iter().enumerate() {
            conn.execute(
                "INSERT INTO fields VALUES (?1, ?2, ?3, x'')",
                params![nt.id, i as i64, f],
            )
            .unwrap();
        }
        for (i, t) in nt.templates.iter().enumerate() {
            let cfg = Enc::default().string(1, &t.qfmt).string(2, &t.afmt).0;
            conn.execute(
                "INSERT INTO templates VALUES (?1, ?2, ?3, 0, 0, ?4)",
                params![nt.id, i as i64, t.name, cfg],
            )
            .unwrap();
        }
    }
    for d in &col.decks {
        let kind = if d.filtered {
            Enc::default().bytes(2, &Enc::default().varint(1, 1).0).0
        } else {
            let normal = Enc::default().varint(1, 1).string(4, &d.description).0;
            Enc::default().bytes(1, &normal).0
        };
        conn.execute(
            "INSERT INTO decks VALUES (?1, ?2, 0, 0, x'', ?3)",
            params![d.id, d.name.replace("::", "\x1f"), kind],
        )
        .unwrap();
    }
    let legacy = open_db(&collection_bytes(col, NOW).unwrap()).unwrap();
    for table in ["notes", "cards", "revlog"] {
        let mut st = legacy.prepare(&format!("SELECT * FROM {table}")).unwrap();
        let n = st.column_count();
        let rows: Vec<Vec<rusqlite::types::Value>> = st
            .query_map([], |r| (0..n).map(|i| r.get(i)).collect())
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        let marks = vec!["?"; n].join(",");
        for row in rows {
            conn.execute(
                &format!("INSERT INTO {table} VALUES ({marks})"),
                rusqlite::params_from_iter(row),
            )
            .unwrap();
        }
    }
    let db = conn.serialize(rusqlite::MAIN_DB).unwrap().to_vec();
    let mut entries = Enc::default();
    for (name, bytes) in &col.media {
        let e = Enc::default()
            .string(1, name)
            .varint(2, bytes.len() as u64)
            .bytes(3, &[0u8; 20])
            .0;
        entries = entries.bytes(1, &e);
    }
    let mut zip = ZipOut::new("apkg");
    zip.add(
        "collection.anki2",
        &collection_bytes(&sample_stub(), NOW).unwrap(),
        true,
    )
    .unwrap();
    zip.add("collection.anki21b", &zstd(&db), false).unwrap();
    zip.add("media", &zstd(&entries.0), false).unwrap();
    zip.add("meta", &Enc::default().varint(1, 3).0, false)
        .unwrap();
    for (i, (_, b)) in col.media.iter().enumerate() {
        zip.add(&i.to_string(), &zstd(b), false).unwrap();
    }
    zip.finish().unwrap()
}

fn sample_stub() -> AnkiCollection {
    AnkiCollection {
        notes: vec![],
        cards: vec![],
        revlog: vec![],
        media: vec![],
        ..sample()
    }
}

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
