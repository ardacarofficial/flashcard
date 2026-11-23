//! `typed_answers` (v1.4 Z1): migration `1421`, round trip, explicit ids, the `correct` check.

use super::{cid, Fx};
use crate::migrate::MIGRATIONS;
use crate::repo::TypedAnswerRecord;
use crate::store::{SqliteStore, Store};
use fc_core::{ReviewLogId, Timestamp};

fn rid(n: u128) -> ReviewLogId {
    ReviewLogId(super::u(0x7100 + n))
}

fn typed(card: u128, review: Option<u128>, text: &str, at: i64) -> TypedAnswerRecord {
    TypedAnswerRecord {
        id: 0,
        card_id: cid(card),
        review_log_id: review.map(rid),
        mode: "normal".into(),
        typed: text.into(),
        correct: Some(false),
        created_at: Timestamp(at),
    }
}

/// An existing collection gets the table; its data is untouched.
#[test]
fn migration_1421_adds_typed_answers_to_an_existing_collection() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("collection.db");
    let before: Vec<_> = MIGRATIONS
        .iter()
        .filter(|m| m.version < 1421)
        .copied()
        .collect();
    drop(SqliteStore::open_with_migrations(&path, &before).unwrap());
    {
        let conn = rusqlite::Connection::open(&path).unwrap();
        conn.execute(
            "INSERT INTO settings (scope, key, value, updated_at) VALUES ('app', 'app.locale', '\"tr\"', 1)",
            [],
        )
        .unwrap();
        let n: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE name = 'typed_answers'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 0);
    }
    let store = SqliteStore::open(&path).unwrap();
    let id = {
        let mut tx = store.write().unwrap();
        let id = tx
            .insert_typed_answer(&typed(1, Some(1), "kitap", 100))
            .unwrap();
        tx.commit().unwrap();
        id
    };
    let rows = store.read().unwrap().typed_answers_of_card(cid(1)).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].id, id);
    let conn = rusqlite::Connection::open(&path).unwrap();
    let locale: String = conn
        .query_row(
            "SELECT value FROM settings WHERE key = 'app.locale'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(locale, "\"tr\"");
}

/// Insert, list by card (newest first) and by review, get, delete.
#[test]
fn typed_answers_round_trip() {
    let fx = Fx::new();
    let ids = fx.write(|tx| {
        [
            typed(1, Some(1), "first", 100),
            typed(2, Some(2), "other card", 150),
            typed(1, None, "no review", 300),
            TypedAnswerRecord {
                correct: None,
                mode: "test".into(),
                ..typed(1, Some(3), "third", 200)
            },
        ]
        .iter()
        .map(|r| tx.insert_typed_answer(r).unwrap())
        .collect::<Vec<_>>()
    });
    assert!(ids.iter().all(|id| *id > 0));
    let of_1 = fx.read(|r| r.typed_answers_of_card(cid(1)).unwrap());
    let texts: Vec<&str> = of_1.iter().map(|r| r.typed.as_str()).collect();
    assert_eq!(texts, vec!["no review", "third", "first"]);
    assert_eq!(
        of_1[1],
        TypedAnswerRecord {
            id: ids[3],
            correct: None,
            mode: "test".into(),
            ..typed(1, Some(3), "third", 200)
        }
    );
    assert_eq!(of_1[0].review_log_id, None);
    assert_eq!(of_1[2].correct, Some(false));

    let of_reviews = fx.read(|r| {
        r.typed_answers_of_reviews(&[rid(2), rid(3), rid(9)])
            .unwrap()
    });
    let texts: Vec<&str> = of_reviews.iter().map(|r| r.typed.as_str()).collect();
    assert_eq!(texts, vec!["other card", "third"]);
    assert!(fx
        .read(|r| r.typed_answers_of_reviews(&[]).unwrap())
        .is_empty());

    assert_eq!(
        fx.read(|r| r.get_typed_answer(ids[0]).unwrap())
            .unwrap()
            .typed,
        "first"
    );
    fx.write(|tx| tx.delete_typed_answer(ids[0]).unwrap());
    assert_eq!(fx.read(|r| r.get_typed_answer(ids[0]).unwrap()), None);
    // Missing: no-op.
    fx.write(|tx| tx.delete_typed_answer(ids[0]).unwrap());
    assert_eq!(
        fx.read(|r| r.typed_answers_of_card(cid(1)).unwrap()).len(),
        2
    );
}

/// An explicit id (restore after undo) is kept; a taken one is refused; `correct` is 0/1/NULL.
#[test]
fn typed_answers_keep_an_explicit_id_and_check_correct() {
    let fx = Fx::new();
    let restored = fx.write(|tx| {
        tx.insert_typed_answer(&TypedAnswerRecord {
            id: 42,
            ..typed(1, Some(1), "back", 100)
        })
        .unwrap()
    });
    assert_eq!(restored, 42);
    let next = fx.write(|tx| {
        tx.insert_typed_answer(&typed(1, Some(2), "new", 200))
            .unwrap()
    });
    assert!(next > 42);
    let mut tx = fx.store.write().unwrap();
    assert!(tx
        .insert_typed_answer(&TypedAnswerRecord {
            id: 42,
            ..typed(1, Some(3), "again", 300)
        })
        .is_err());
    drop(tx);
    assert!(fx
        .raw()
        .execute(
            "INSERT INTO typed_answers (card_id, mode, typed, correct, created_at)
             VALUES ('x', 'normal', 't', 2, 1)",
            [],
        )
        .is_err());
}
