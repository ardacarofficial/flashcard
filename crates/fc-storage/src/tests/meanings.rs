//! `note_meanings` / `note_accepted_answers` (v1.4 W2): migration `1450`, round trips, and
//! that a note deleted for good takes its rows with it.

use super::{nid, note, Fx};
use crate::migrate::MIGRATIONS;
use crate::repo::{AcceptedAnswerRecord, NoteMeaningRecord};
use crate::store::{SqliteStore, Store};
use fc_core::{NoteId, Timestamp};

fn meaning(note_id: NoteId) -> NoteMeaningRecord {
    NoteMeaningRecord {
        note_id,
        field_key: "meaning".into(),
        status: "applied".into(),
        meaning: "almak (okuldan)".into(),
        phrase: "pick up the kids".into(),
        expression: true,
        also_correct: vec!["alıp getirmek".into()],
        not_this: vec!["kaldırmak".into(), "öğrenmek".into()],
        confidence: Some(0.9),
        edited: false,
        original: "kaldırmak, almak".into(),
        model: "claude-sonnet-5-5".into(),
        created_at: Timestamp(10),
        updated_at: Timestamp(20),
    }
}

fn answer(note_id: NoteId, field: &str, a: &str, pos: i64) -> AcceptedAnswerRecord {
    AcceptedAnswerRecord {
        note_id,
        field_key: field.into(),
        answer: a.into(),
        position: pos,
    }
}

/// An existing collection gets both tables; its data is untouched.
#[test]
fn migration_1450_adds_meanings_and_accepted_answers_to_an_existing_collection() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("collection.db");
    let before: Vec<_> = MIGRATIONS
        .iter()
        .filter(|m| m.version < 1450)
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
                "SELECT count(*) FROM sqlite_master WHERE name IN ('note_meanings', 'note_accepted_answers')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 0);
    }
    let store = SqliteStore::open(&path).unwrap();
    {
        let mut tx = store.write().unwrap();
        tx.put_note_meaning(&meaning(nid(1))).unwrap();
        tx.set_accepted_answers(nid(1), &[answer(nid(1), "meaning", "bırakmak", 0)])
            .unwrap();
        tx.commit().unwrap();
    }
    assert_eq!(
        store.read().unwrap().get_note_meaning(nid(1)).unwrap(),
        Some(meaning(nid(1)))
    );
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

#[test]
fn meanings_and_accepted_answers_round_trip_and_follow_a_purged_note() {
    let fx = Fx::seeded();
    fx.write(|tx| {
        super::ensure_home(tx);
        tx.insert_note(&note(1, "give up", "vazgeçmek", &[], 1), "give up")
            .unwrap();
        tx.insert_note(&note(2, "pick up", "almak", &[], 1), "pick up")
            .unwrap();
        for n in [1, 2] {
            tx.put_note_meaning(&meaning(nid(n))).unwrap();
        }
        tx.set_accepted_answers(
            nid(1),
            &[
                answer(nid(1), "meaning", "bırakmak", 1),
                answer(nid(1), "meaning", "vazgeçmek", 0),
                answer(nid(1), "back", "x", 0),
            ],
        )
        .unwrap();
        tx.set_accepted_answers(nid(2), &[answer(nid(2), "meaning", "almak", 0)])
            .unwrap();
    });
    let got = fx.read(|r| r.list_accepted_answers(nid(1)).unwrap());
    let words: Vec<_> = got
        .iter()
        .map(|a| (a.field_key.as_str(), a.answer.as_str()))
        .collect();
    assert_eq!(
        words,
        [
            ("back", "x"),
            ("meaning", "vazgeçmek"),
            ("meaning", "bırakmak")
        ]
    );

    // Replacing is exact; an edit of the suggestion keeps one row per note.
    fx.write(|tx| {
        tx.set_accepted_answers(nid(1), &[answer(nid(1), "meaning", "bırakmak", 0)])
            .unwrap();
        let mut m = meaning(nid(1));
        m.status = "suggested".into();
        m.confidence = None;
        tx.put_note_meaning(&m).unwrap();
    });
    assert_eq!(
        fx.read(|r| r.list_accepted_answers(nid(1)).unwrap()).len(),
        1
    );
    let m = fx.read(|r| r.get_note_meaning(nid(1)).unwrap()).unwrap();
    assert_eq!((m.status.as_str(), m.confidence), ("suggested", None));

    // Trashing keeps the rows; a purge (the row leaves `notes`) removes them.
    fx.write(|tx| {
        tx.set_notes_deleted(&[nid(1)], Some(Timestamp(5))).unwrap();
    });
    assert!(fx.read(|r| r.get_note_meaning(nid(1)).unwrap()).is_some());
    fx.raw()
        .execute("DELETE FROM notes WHERE id = ?1", [nid(1).to_string()])
        .unwrap();
    assert!(fx.read(|r| r.get_note_meaning(nid(1)).unwrap()).is_none());
    assert!(fx
        .read(|r| r.list_accepted_answers(nid(1)).unwrap())
        .is_empty());
    // The other note is untouched.
    assert!(fx.read(|r| r.get_note_meaning(nid(2)).unwrap()).is_some());
    fx.write(|tx| tx.delete_note_meaning(nid(2)).unwrap());
    assert!(fx.read(|r| r.get_note_meaning(nid(2)).unwrap()).is_none());
    assert_eq!(
        fx.read(|r| r.list_accepted_answers(nid(2)).unwrap()).len(),
        1
    );
}
