//! Reading mode (v0.10): migration `0900` and the repository round trips.

use super::{add_note, deck, ensure_home, note, u, Fx, HOME};
use crate::migrate::MIGRATIONS;
use crate::repo::{ReadingItemRecord, WordMarkRecord};
use crate::store::{SqliteStore, Store};
use fc_core::card::CardState;
use fc_core::notetype::FieldKey;
use fc_core::{ReadingItemId, Timestamp};

fn rid(n: u128) -> ReadingItemId {
    ReadingItemId(u(n))
}

fn item(n: u128, body: &str, at: i64) -> ReadingItemRecord {
    ReadingItemRecord {
        id: rid(n),
        kind: "text".into(),
        parent_id: None,
        title: format!("Text {n}"),
        language: "en".into(),
        source: Some("https://example.org/a".into()),
        body: body.into(),
        length: body.encode_utf16().count() as i64,
        cues: None,
        cue_count: 0,
        media_path: None,
        extract_range: None,
        position: 0,
        queued: false,
        priority: "normal".into(),
        due: None,
        interval_days: 0,
        read_count: 0,
        last_read: None,
        created_at: Timestamp(at),
        updated_at: Timestamp(at),
        deleted_at: None,
    }
}

#[test]
fn migration_0900_adds_reading_tables_to_an_existing_database() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("collection.db");
    let before: Vec<_> = MIGRATIONS
        .iter()
        .filter(|m| m.version < 900)
        .copied()
        .collect();
    drop(SqliteStore::open_with_migrations(&path, &before).unwrap());
    let tables = |conn: &rusqlite::Connection| -> i64 {
        conn.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table'
               AND name IN ('reading_items', 'word_marks')",
            [],
            |r| r.get(0),
        )
        .unwrap()
    };
    assert_eq!(tables(&rusqlite::Connection::open(&path).unwrap()), 0);
    let store = SqliteStore::open(&path).unwrap();
    assert_eq!(tables(&rusqlite::Connection::open(&path).unwrap()), 2);
    let mut tx = store.write().unwrap();
    tx.upsert_reading_item(&item(1, "Hello", 1)).unwrap();
    tx.set_word_mark("en", "hello", Some("known"), Timestamp(2))
        .unwrap();
    tx.commit().unwrap();
    assert_eq!(
        store
            .read()
            .unwrap()
            .list_reading_items(false)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn items_round_trip_list_without_bodies_trash_and_purge() {
    let fx = Fx::new();
    let a = item(1, "Kediler koşuyor.", 1);
    let mut b = item(2, "Second", 2);
    b.kind = "subtitle".into();
    b.cues = Some(r#"[{"start_ms":0,"end_ms":10,"text":"Second"}]"#.into());
    b.media_path = Some("C:\\videos\\film.mp4".into());
    b.queued = true;
    b.priority = "high".into();
    b.due = Some(Timestamp(99));
    b.interval_days = 4;
    b.read_count = 2;
    b.last_read = Some(Timestamp(50));
    let mut c = item(3, "koşuyor", 3);
    c.kind = "extract".into();
    c.parent_id = Some(a.id);
    c.extract_range = Some((8, 16));
    fx.write(|tx| {
        tx.upsert_reading_item(&a).unwrap();
        tx.upsert_reading_item(&b).unwrap();
        tx.upsert_reading_item(&c).unwrap();
    });
    let full = |r: &ReadingItemRecord| ReadingItemRecord {
        cue_count: i64::from(r.cues.is_some()),
        ..r.clone()
    };
    assert_eq!(
        fx.read(|r| r.get_reading_item(b.id).unwrap()),
        Some(full(&b))
    );
    assert_eq!(
        fx.read(|r| r.get_reading_item(c.id).unwrap()),
        Some(full(&c))
    );
    let list = fx.read(|r| r.list_reading_items(false).unwrap());
    assert_eq!(
        list.iter().map(|i| i.id).collect::<Vec<_>>(),
        [c.id, b.id, a.id],
        "newest first"
    );
    assert!(list.iter().all(|i| i.body.is_empty() && i.cues.is_none()));
    assert_eq!(list[1].cue_count, 1);
    assert_eq!(list[2].length, "Kediler koşuyor.".chars().count() as i64);

    // Trash, restore, trash again and purge: the extract loses its parent link.
    assert_eq!(
        fx.write(|tx| tx
            .set_reading_items_deleted(&[a.id], Some(Timestamp(100)))
            .unwrap()),
        1
    );
    assert_eq!(fx.read(|r| r.list_reading_items(false).unwrap()).len(), 2);
    assert_eq!(fx.read(|r| r.list_reading_items(true).unwrap()).len(), 3);
    assert_eq!(
        fx.write(|tx| tx.purge_reading_items(Timestamp(100)).unwrap()),
        0
    );
    assert_eq!(
        fx.write(|tx| tx.purge_reading_items(Timestamp(101)).unwrap()),
        1
    );
    assert_eq!(fx.read(|r| r.get_reading_item(a.id).unwrap()), None);
    assert_eq!(
        fx.read(|r| r.get_reading_item(c.id).unwrap())
            .unwrap()
            .parent_id,
        None
    );
}

#[test]
fn word_marks_set_replace_and_clear() {
    let fx = Fx::new();
    fx.write(|tx| {
        tx.set_word_mark("en", "run", Some("known"), Timestamp(1))
            .unwrap();
        tx.set_word_mark("en", "john", Some("ignored"), Timestamp(2))
            .unwrap();
        tx.set_word_mark("tr", "ev", Some("known"), Timestamp(3))
            .unwrap();
        tx.set_word_mark("en", "run", Some("ignored"), Timestamp(4))
            .unwrap();
    });
    let en = fx.read(|r| r.list_word_marks("en").unwrap());
    assert_eq!(
        en,
        vec![
            WordMarkRecord {
                language: "en".into(),
                lemma: "john".into(),
                status: "ignored".into(),
                updated_at: Timestamp(2)
            },
            WordMarkRecord {
                language: "en".into(),
                lemma: "run".into(),
                status: "ignored".into(),
                updated_at: Timestamp(4)
            },
        ]
    );
    fx.write(|tx| tx.set_word_mark("en", "run", None, Timestamp(5)).unwrap());
    assert_eq!(fx.read(|r| r.list_word_marks("en").unwrap()).len(), 1);
    assert_eq!(fx.read(|r| r.list_word_marks("tr").unwrap()).len(), 1);
}

#[test]
fn vocabulary_rows_prefer_the_word_field_and_skip_trash() {
    let fx = Fx::seeded();
    fx.write(|tx| {
        ensure_home(tx);
        tx.insert_deck(&deck(7, None, "English")).unwrap();
        add_note(tx, &note(1, "<b>running</b>", "koşmak", &[], 1), 7);
        let mut with_word = note(2, "front text", "back", &[], 2);
        with_word
            .fields
            .insert(FieldKey::new("word"), "cats".into());
        add_note(tx, &with_word, 7);
        let mut empty_word = note(3, "dog", "köpek", &[], 3);
        empty_word
            .fields
            .insert(FieldKey::new("word"), String::new());
        add_note(tx, &empty_word, HOME);
        add_note(tx, &note(4, "gone", "", &[], 4), 7);
        tx.set_notes_deleted(&[super::nid(4)], Some(Timestamp(9)))
            .unwrap();
    });
    let rows = fx.read(|r| r.vocabulary_rows().unwrap());
    let words: Vec<(&str, CardState)> = rows.iter().map(|r| (r.word.as_str(), r.state)).collect();
    assert_eq!(
        words,
        [
            ("running", CardState::New),
            ("cats", CardState::New),
            ("dog", CardState::New)
        ]
    );
    assert_eq!(rows[1].note_id, super::nid(2));
}
