//! v1.4 migration 1410: the word index tables and the triggers that queue changes.

use super::*;
use crate::migrate::MIGRATIONS;
use crate::repo::{WordIndexNoteRecord, WordIndexRecord};
use fc_core::card::CardState;
use fc_core::settings::SettingScope;

fn dirty(fx: &Fx) -> (Vec<NoteId>, Vec<(String, String)>) {
    fx.write(|tx| tx.take_word_index_dirty().unwrap())
}

/// A collection at schema 1409 with a note, a card and a mark, then opened by this build:
/// 1410 queues every existing note and mark, so the app builds the index on first open.
#[test]
fn migration_1410_queues_the_existing_collection() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("collection.db");
    let upto = MIGRATIONS.iter().position(|m| m.version >= 1410).unwrap();
    {
        let old = SqliteStore::open_with_migrations(&path, &MIGRATIONS[..upto]).unwrap();
        let mut tx = old.write().unwrap();
        tx.upsert_notetype(&classic()).unwrap();
        put_note(&mut *tx, &note(1, "went", "gitti", &[], 5), "went").unwrap();
        tx.insert_card(&card(1, 1, HOME)).unwrap();
        tx.set_word_mark("en", "door", Some("known"), Timestamp(7))
            .unwrap();
        tx.commit().unwrap();
    }
    let store = SqliteStore::open(&path).unwrap();
    let fx = Fx { store, dir };
    assert_eq!(fx.read(|r| r.word_index_dirty_count().unwrap()), 2);
    let (notes, keys) = dirty(&fx);
    assert_eq!(notes, vec![nid(1)]);
    assert_eq!(keys, vec![("en".to_owned(), "door".to_owned())]);
    assert_eq!(fx.read(|r| r.word_index_dirty_count().unwrap()), 0);
}

#[test]
fn triggers_queue_every_source_of_a_word() {
    let fx = Fx::seeded();
    fx.write(|tx| {
        put_note(tx, &note(1, "go", "git", &[], 5), "go").unwrap();
        tx.insert_card(&card(1, 1, HOME)).unwrap();
    });
    assert_eq!(dirty(&fx).0, vec![nid(1)]);
    // A review (card update) queues its note.
    fx.write(|tx| {
        let mut c = tx.get_card(cid(1)).unwrap().unwrap();
        c.scheduling.state = CardState::Review;
        tx.update_card(&c).unwrap();
    });
    assert_eq!(dirty(&fx).0, vec![nid(1)]);
    // A mark set twice (an upsert updates the row: OLD and NEW are the same key) and cleared.
    fx.write(|tx| {
        tx.set_word_mark("en", "go", Some("known"), Timestamp(1))
            .unwrap();
        tx.set_word_mark("en", "go", Some("ignored"), Timestamp(2))
            .unwrap();
    });
    assert_eq!(dirty(&fx).1, vec![("en".to_owned(), "go".to_owned())]);
    fx.write(|tx| tx.set_word_mark("en", "go", None, Timestamp(3)).unwrap());
    assert_eq!(dirty(&fx).1.len(), 1);
    // The deck language setting queues the deck's notes (deck scope) or all (app scope).
    fx.write(|tx| {
        tx.set_setting(
            &SettingScope::Deck(did(HOME)),
            "words.language",
            &serde_json::json!("en"),
            Timestamp(4),
        )
        .unwrap();
    });
    assert_eq!(dirty(&fx).0, vec![nid(1)]);
    fx.write(|tx| {
        tx.set_setting(
            &SettingScope::Deck(did(9)),
            "words.language",
            &serde_json::json!("en"),
            Timestamp(4),
        )
        .unwrap();
    });
    assert!(
        dirty(&fx).0.is_empty(),
        "another deck's language leaves this note alone"
    );
    // A producer's language queues the note.
    fx.write(|tx| tx.set_word_note_language(nid(1), "en").unwrap());
    assert_eq!(dirty(&fx).0, vec![nid(1)]);
    // Deleting the card queues the note.
    fx.write(|tx| {
        tx.set_cards_deleted(&[cid(1)], Some(Timestamp(9)))
            .map(|_| ())
            .unwrap()
    });
    assert_eq!(dirty(&fx).0, vec![nid(1)]);
}

#[test]
fn rows_round_trip_and_status_is_derived_on_read() {
    let fx = Fx::seeded();
    let n = WordIndexNoteRecord {
        note_id: nid(1),
        language: "en".into(),
        key: "go".into(),
        cards: 2,
        max_interval_days: Some(30),
        last_review: Some(Timestamp(9)),
        created_at: Timestamp(1),
    };
    let w = WordIndexRecord {
        language: "en".into(),
        key: "go".into(),
        mark: None,
        mark_at: None,
        notes: 1,
        cards: 2,
        max_interval_days: Some(30),
        last_review: Some(Timestamp(9)),
        first_seen: Some(Timestamp(1)),
    };
    let marked = WordIndexRecord {
        key: "act".into(),
        mark: Some("ignored".into()),
        mark_at: Some(Timestamp(3)),
        notes: 0,
        cards: 0,
        max_interval_days: None,
        last_review: None,
        first_seen: None,
        ..w.clone()
    };
    fx.write(|tx| {
        tx.put_word_index_note(&n).unwrap();
        tx.put_word_index_entry(&w).unwrap();
        tx.put_word_index_entry(&marked).unwrap();
    });
    fx.read(|r| {
        assert_eq!(
            r.word_index_notes_by_id(&[nid(1)]).unwrap(),
            vec![n.clone()]
        );
        assert_eq!(
            r.word_index_get("en", &["go".into(), "x".into()]).unwrap(),
            vec![w.clone()]
        );
        let counts = r.word_index_counts(21).unwrap();
        assert_eq!(
            counts,
            vec![
                ("en".into(), "ignored".into(), 1),
                ("en".into(), "known".into(), 1)
            ]
        );
        let learning = r.word_index_counts(31).unwrap();
        assert!(learning.contains(&("en".into(), "learning".into(), 1)));
    });
}
