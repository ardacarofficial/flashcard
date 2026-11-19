//! Synthetic Anki packages for tests (no downloads): one collection with cloze, image and
//! sound notes, every card state, a filtered deck and review history, written in each package
//! layout. Enabled by the `fixtures` feature (and in this crate's tests).
#![allow(clippy::unwrap_used, clippy::expect_used, missing_docs)]

use super::proto::Enc;
use super::read::open_db;
use super::schema::{SCHEMA11, SCHEMA18_TABLES};
use super::write::collection_bytes;
use super::*;
use crate::zipio::ZipOut;
use fc_core::Timestamp;
use rusqlite::{params, Connection};

pub const CRT: i64 = 1_600_000_000;
pub const NOW: Timestamp = Timestamp(1_700_000_000_000);
pub const BASIC: i64 = 1_600_000_000_001;
pub const CLOZE: i64 = 1_600_000_000_002;
pub const WORDS: i64 = 1_600_000_000_100;
pub const LANG: i64 = 1_600_000_000_101;
pub const FILTERED: i64 = 1_600_000_000_200;
pub const CAT: &[u8] = b"\xff\xd8\xff\xe0JPEG cat";
pub const HELLO: &[u8] = b"ID3 hello mp3";

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
pub fn sample() -> AnkiCollection {
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

pub fn legacy2_package(col: &AnkiCollection) -> Vec<u8> {
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
pub fn latest_package(col: &AnkiCollection) -> Vec<u8> {
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

pub fn sample_stub() -> AnkiCollection {
    AnkiCollection {
        notes: vec![],
        cards: vec![],
        revlog: vec![],
        media: vec![],
        ..sample()
    }
}
