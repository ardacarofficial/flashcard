//! Hostile and broken Anki packages (v1.4.1 H3): what a real user's old, hand-edited or
//! damaged `.apkg`/`.colpkg` can look like. Each case is a small named package built from
//! [`super::fixtures::sample`]; the readers must refuse it with a message or read what makes
//! sense, never panic, and the app must never leave half an import behind
//! (`fc-app` `io::fuzz_tests`). Enabled by the `fixtures` feature (and in this crate's tests).
#![allow(clippy::unwrap_used, clippy::expect_used, missing_docs)]

use super::fixtures::{legacy2_package, sample, CAT, NOW};
use super::write::collection_bytes;
use super::*;
use crate::zipio::ZipOut;

/// One hostile package.
pub struct Case {
    /// What is wrong with it.
    pub name: &'static str,
    /// The package.
    pub bytes: Vec<u8>,
    /// `Some(false)`: must be refused; `Some(true)`: must be read (issues reported); `None`:
    /// either, as long as a refusal says why.
    pub read: Option<bool>,
}

/// A legacy (`collection.anki21`) package whose database went through `sql` first, with the
/// given media map and files (`None` = those of [`sample`]).
pub fn legacy_with_sql(col: &AnkiCollection, sql: &str) -> Vec<u8> {
    let db = collection_bytes(col, NOW).unwrap();
    let mut conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.create_collation("unicase", |a, b| a.to_lowercase().cmp(&b.to_lowercase()))
        .unwrap();
    conn.deserialize_read_exact(rusqlite::MAIN_DB, &db[..], db.len(), false)
        .unwrap();
    conn.execute_batch(sql).unwrap();
    let db = conn.serialize(rusqlite::MAIN_DB).unwrap().to_vec();
    zip(&[("collection.anki21", &db), ("media", b"{}")])
}

/// A zip of the given `(name, bytes)` entries (deflated).
pub fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut z = ZipOut::new("apkg");
    for (name, bytes) in entries {
        z.add(name, bytes, true).unwrap();
    }
    z.finish().unwrap()
}

fn with(f: impl FnOnce(&mut AnkiCollection)) -> AnkiCollection {
    let mut col = sample();
    f(&mut col);
    col
}

/// Every hostile case.
pub fn cases() -> Vec<Case> {
    let ok = |name, bytes| Case {
        name,
        bytes,
        read: None,
    };
    let read = |name, bytes| Case {
        name,
        bytes,
        read: Some(true),
    };
    let bad = |name, bytes| Case {
        name,
        bytes,
        read: Some(false),
    };
    let base = sample();
    let db = collection_bytes(&base, NOW).unwrap();
    let huge = "ü".repeat(3 * 1024 * 1024);
    vec![
        // ---- not a package
        bad("empty file", Vec::new()),
        bad("text file", b"front\tback\n".to_vec()),
        bad("zip without a collection", zip(&[("media", b"{}")])),
        bad(
            "collection is not sqlite",
            zip(&[("collection.anki21", b"SQLite format 3\0garbage")]),
        ),
        bad(
            "anki21b is not zstd",
            zip(&[("collection.anki21b", &db), ("media", b"")]),
        ),
        // ---- wrong schema versions / missing tables
        read(
            "col.ver from the future",
            legacy_with_sql(&base, "UPDATE col SET ver = 99"),
        ),
        read(
            "col.ver negative",
            legacy_with_sql(&base, "UPDATE col SET ver = -1, crt = -5"),
        ),
        bad(
            "notes table missing",
            legacy_with_sql(&base, "DROP TABLE notes"),
        ),
        bad(
            "notes is a view",
            legacy_with_sql(
                &base,
                "ALTER TABLE notes RENAME TO n2; CREATE VIEW notes AS SELECT * FROM n2",
            ),
        ),
        bad("col table empty", legacy_with_sql(&base, "DELETE FROM col")),
        bad(
            "models json broken",
            legacy_with_sql(&base, "UPDATE col SET models = '{\"1\": [nope'"),
        ),
        read(
            "models json of the wrong shape",
            legacy_with_sql(&base, "UPDATE col SET models = '[1,2,3]', decks = '\"x\"'"),
        ),
        // ---- broken text
        read(
            "invalid UTF-8 in fields and tags",
            legacy_with_sql(
                &base,
                "UPDATE notes SET flds = CAST(x'fffe411fc328' AS TEXT), \
                 tags = CAST(x'20808120' AS TEXT)",
            ),
        ),
        read(
            "blob fields",
            legacy_with_sql(&base, "UPDATE notes SET flds = x'00ff001f00'"),
        ),
        read(
            "numbers where text is",
            legacy_with_sql(&base, "UPDATE notes SET flds = 12.5, guid = 7, tags = ''"),
        ),
        ok(
            "text where numbers are",
            legacy_with_sql(
                &base,
                "UPDATE cards SET ivl = 'many', due = 'x', factor = '', odid = 'y'; \
                 UPDATE revlog SET ease = 'z', ivl = 1e300",
            ),
        ),
        read(
            "huge field",
            legacy2_package(&with(|c| c.notes[0].fields[1] = huge.clone())),
        ),
        read(
            "more fields than the note type",
            legacy2_package(&with(|c| {
                c.notes[0].fields = (0..500).map(|i| i.to_string()).collect();
                c.notes[1].fields.clear();
            })),
        ),
        // ---- duplicates and dangling references
        read(
            "duplicate guids",
            legacy2_package(&with(|c| {
                let g = c.notes[0].guid.clone();
                for n in &mut c.notes {
                    n.guid = g.clone();
                }
            })),
        ),
        read(
            "duplicate deck and note type names, duplicate field names",
            legacy2_package(&with(|c| {
                for d in &mut c.decks {
                    d.name = "Same".into();
                }
                c.notetypes[1].name = c.notetypes[0].name.clone();
                c.notetypes[0].fields = vec!["Front".into(), "Front".into()];
                let t = c.notetypes[0].templates[0].clone();
                c.notetypes[0].templates.push(t);
            })),
        ),
        read(
            "cards of missing notes, decks and templates",
            legacy2_package(&with(|c| {
                c.cards[0].nid = 42;
                c.cards[1].did = 4242;
                c.cards[2].ord = 99;
                c.cards[3].ord = -1;
                c.notes[0].mid = 777;
                c.revlog[0].cid = 31337;
            })),
        ),
        read(
            "filtered deck pointing at itself and at nothing",
            legacy2_package(&with(|c| {
                c.cards[4].odid = super::fixtures::FILTERED;
                c.cards[0].odid = 999;
                c.cards[0].did = super::fixtures::FILTERED;
            })),
        ),
        read(
            "deck names that do not make a tree",
            legacy2_package(&with(|c| {
                c.decks[0].name = String::new();
                c.decks[1].name = "::Lang::::Words::".into();
                c.decks[2].name = "Lang::Words::Lang::Words".into();
                c.decks[3].name = "   ".into();
            })),
        ),
        read(
            "extreme scheduling numbers",
            legacy2_package(&with(|c| {
                for (i, card) in c.cards.iter_mut().enumerate() {
                    card.due = [i64::MIN, i64::MAX, -1, 0, 1 << 62][i % 5];
                    card.ivl = [i64::MAX, i64::MIN, -100_000, 0, 1 << 40][i % 5];
                    card.factor = [i64::MAX, -1, 0, 1, 99_999][i % 5];
                    card.reps = -3;
                    card.lapses = i64::MAX;
                    card.queue = 17;
                    card.ctype = -9;
                    card.data = "{\"s\": 1e999, \"d\": \"x\"".into();
                }
                for (k, r) in c.revlog.iter_mut().enumerate() {
                    r.id = i64::MIN + k as i64;
                    r.time = i64::MAX;
                    r.ivl = i64::MIN;
                }
                c.crt = i64::MAX;
            })),
        ),
        // ---- media
        read(
            "media listed but missing",
            zip(&[
                ("collection.anki21", &db),
                ("media", br#"{"0": "cat.jpg", "1": "hello.mp3", "7": "ghost.png"}"#),
                ("0", CAT),
            ]),
        ),
        read(
            "media names that try to escape",
            zip(&[
                ("collection.anki21", &db),
                (
                    "media",
                    br#"{"0": "../../evil.jpg", "1": "C:\\Windows\\x.dll", "2": "", "3": "a/b.png"}"#,
                ),
                ("0", CAT),
                ("1", CAT),
                ("2", CAT),
                ("3", CAT),
            ]),
        ),
        read(
            "two numbers, one name",
            zip(&[
                ("collection.anki21", &db),
                ("media", br#"{"0": "cat.jpg", "1": "cat.jpg"}"#),
                ("0", CAT),
                ("1", b"other bytes"),
            ]),
        ),
        bad(
            "media map is not JSON",
            zip(&[("collection.anki21", &db), ("media", b"{not json")]),
        ),
        read(
            "media map of the wrong shape",
            zip(&[("collection.anki21", &db), ("media", br#"{"0": 5, "x": null}"#)]),
        ),
    ]
}
