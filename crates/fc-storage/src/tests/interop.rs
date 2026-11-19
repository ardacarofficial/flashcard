//! Import/export storage: migrations 0400/0401, media bytes, review log rows, dump/load,
//! unfinished undo groups (import crash markers).

use super::*;
use crate::migrate;
use crate::repo::{MediaRecord, ReviewLogRecord, SqlValue};
use fc_core::card::{CardState, Rating};
use fc_core::{ReviewLogId, Timestamp};

fn media(hash: &str) -> MediaRecord {
    MediaRecord {
        hash: hash.into(),
        mime: "image/png".into(),
        size: 3,
        original_name: Some("a.png".into()),
        created_at: Timestamp(7),
    }
}

fn review(n: u128, card: CardId, at: i64) -> ReviewLogRecord {
    ReviewLogRecord {
        id: ReviewLogId(u(0x9000 + n)),
        card_id: card,
        session_id: None,
        rating: Rating::Good,
        state_before: CardState::Learning,
        state_after: CardState::Review,
        scheduled_days: 3,
        elapsed_days: 1,
        duration_ms: 4200,
        answer_duration_ms: Some(900),
        hints_used: 0,
        mode: "normal".into(),
        stability_after: Some(0.1 + 0.2),
        difficulty_after: None,
        created_at: Timestamp(at),
    }
}

#[test]
fn migration_0400_adds_media_blobs_with_cascade() {
    let fx = Fx::new();
    let conn = fx.raw();
    assert!(migrate::schema_object_exists(&conn, "media_blobs").unwrap());
    assert!(migrate::current_version(&conn).unwrap() >= 400);
    conn.execute_batch("PRAGMA foreign_keys = ON").unwrap();
    // A blob needs its media row.
    assert!(conn
        .execute("INSERT INTO media_blobs VALUES ('x', x'00')", [])
        .is_err());
    conn.execute(
        "INSERT INTO media (hash, mime, size, created_at) VALUES ('x', 'a/b', 1, 0)",
        [],
    )
    .unwrap();
    conn.execute("INSERT INTO media_blobs VALUES ('x', x'00')", [])
        .unwrap();
    conn.execute("DELETE FROM media WHERE hash = 'x'", [])
        .unwrap();
    let left: i64 = conn
        .query_row("SELECT count(*) FROM media_blobs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(left, 0);
}

#[test]
fn migration_0401_unfinished_batches_hold_parts_and_stay_out_of_dumps() {
    use crate::repo::UnfinishedBatchRecord;
    use fc_core::{BatchId, JobId};
    let fx = Fx::new();
    {
        let conn = fx.raw();
        assert!(migrate::current_version(&conn).unwrap() >= 401);
        conn.execute_batch("PRAGMA foreign_keys = ON").unwrap();
        // A part needs its marker; the state is constrained.
        assert!(conn
            .execute(
                "INSERT INTO unfinished_batch_parts VALUES ('x', 0, '[]')",
                []
            )
            .is_err());
        assert!(conn
            .execute(
                "INSERT INTO unfinished_batches VALUES ('x', NULL, 'done', '{}', 0, 0)",
                []
            )
            .is_err());
    }
    let batch = BatchId(u(0x77));
    let mut marker = UnfinishedBatchRecord {
        batch_id: batch,
        job_id: Some(JobId(u(0x78))),
        state: "started".into(),
        rollback: serde_json::json!({"method": "v1.x", "params": {"since": 3}}),
        created_at: Timestamp(1),
        updated_at: Timestamp(1),
    };
    fx.write(|tx| tx.upsert_unfinished_batch(&marker).unwrap());
    marker.state = "writing".into();
    marker.updated_at = Timestamp(2);
    fx.write(|tx| {
        tx.upsert_unfinished_batch(&marker).unwrap();
        tx.add_unfinished_batch_part(batch, 1, &serde_json::json!([["b"]]))
            .unwrap();
        tx.add_unfinished_batch_part(batch, 0, &serde_json::json!([["a"]]))
            .unwrap();
    });
    fx.read(|r| {
        assert_eq!(r.list_unfinished_batches().unwrap(), vec![marker.clone()]);
        assert_eq!(
            r.unfinished_batch_parts(batch).unwrap(),
            vec![serde_json::json!([["a"]]), serde_json::json!([["b"]])]
        );
        let dump = r.dump_collection().unwrap();
        assert!(!dump
            .tables
            .iter()
            .any(|t| t.name.starts_with("unfinished_batch")));
    });
    assert!(fx.write(|tx| tx.delete_unfinished_batch(batch).unwrap()));
    fx.read(|r| {
        assert!(r.list_unfinished_batches().unwrap().is_empty());
        assert!(r.unfinished_batch_parts(batch).unwrap().is_empty());
    });
    assert!(!fx.write(|tx| tx.delete_unfinished_batch(batch).unwrap()));
}

#[test]
fn media_is_content_addressed() {
    let fx = Fx::new();
    assert!(fx.write(|tx| tx.put_media(&media("h1"), b"abc").unwrap()));
    assert!(!fx.write(|tx| tx.put_media(&media("h1"), b"abc").unwrap()));
    fx.read(|r| {
        assert_eq!(r.get_media("h1").unwrap(), Some(media("h1")));
        assert_eq!(r.media_bytes("h1").unwrap().as_deref(), Some(&b"abc"[..]));
        assert_eq!(r.list_media().unwrap().len(), 1);
        assert!(r.get_media("nope").unwrap().is_none());
    });
}

#[test]
fn review_logs_insert_once_and_read_by_card() {
    let fx = Fx::seeded();
    fx.write(|tx| {
        ensure_home(tx);
        tx.insert_deck(&deck(1, None, "D")).unwrap();
        add_note(tx, &note(1, "a", "b", &[], 1), 1);
        add_note(tx, &note(2, "c", "d", &[], 1), 1);
    });
    let (c1, c2) = (CardId(nid(1).0), CardId(nid(2).0));
    let rows = vec![review(2, c1, 20), review(1, c1, 10), review(3, c2, 5)];
    assert_eq!(fx.write(|tx| tx.insert_review_logs(&rows).unwrap()), 3);
    assert_eq!(fx.write(|tx| tx.insert_review_logs(&rows).unwrap()), 0);
    fx.read(|r| {
        let got = r.review_logs_for_cards(&[c1]).unwrap();
        assert_eq!(got, vec![review(1, c1, 10), review(2, c1, 20)]);
        assert_eq!(r.review_logs_for_cards(&[c1, c2]).unwrap().len(), 3);
    });
}

#[test]
fn dump_then_load_is_bit_exact() {
    let fx = Fx::seeded();
    fx.write(|tx| {
        ensure_home(tx);
        tx.insert_deck(&deck(1, None, "Deck")).unwrap();
        add_note(tx, &note(1, "front <b>x</b>", "back", &["t1"], 1), 1);
        let mut c = tx.get_card(CardId(nid(1).0)).unwrap().unwrap();
        c.scheduling.stability = Some(0.1 + 0.2);
        tx.update_card(&c).unwrap();
        tx.insert_review_logs(&[review(1, c.id, 10)]).unwrap();
        tx.put_media(&media("h1"), &[0, 255, 7]).unwrap();
    });
    let before = fx.read(|r| r.dump_collection().unwrap());
    assert!(before.schema_versions.contains(&1) && before.schema_versions.contains(&400));
    let names: Vec<&str> = before.tables.iter().map(|t| t.name.as_str()).collect();
    assert!(names.contains(&"notes") && names.contains(&"notes_fts") && names.contains(&"oplog"));
    assert!(!names
        .iter()
        .any(|n| n.starts_with("notes_fts_") || *n == "media_blobs"));
    let cards = before.tables.iter().find(|t| t.name == "cards").unwrap();
    assert_eq!(cards.columns[0], "rowid");
    let stab = cards.columns.iter().position(|c| c == "stability").unwrap();
    assert_eq!(
        cards.rows[0][stab],
        SqlValue::Real("0.30000000000000004".into())
    );

    // JSON round trip, then wipe and load.
    let json = serde_json::to_string(&before).unwrap();
    let dump = serde_json::from_str(&json).unwrap();
    fx.write(|tx| {
        tx.insert_deck(&deck(2, None, "Other")).unwrap();
    });
    fx.write(|tx| tx.load_collection(&dump).unwrap());
    let after = fx.read(|r| r.dump_collection().unwrap());
    assert_eq!(after, before);
    // Media bytes are the caller's job after a load.
    fx.read(|r| assert!(r.media_bytes("h1").unwrap().is_none()));
    // FTS still finds the note.
    let hits: i64 = fx
        .raw()
        .query_row(
            "SELECT count(*) FROM notes_fts WHERE notes_fts MATCH 'front'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(hits, 1);
}

#[test]
fn load_rejects_unknown_tables_and_columns_without_touching_data() {
    let fx = Fx::seeded();
    let mut dump = fx.read(|r| r.dump_collection().unwrap());
    let good = dump.clone();
    dump.tables[0].columns.push("nope".into());
    let mut tx = fx.store.write().unwrap();
    assert!(tx.load_collection(&dump).is_err());
    drop(tx);
    let mut bad_table = good.clone();
    bad_table.tables.push(crate::repo::TableDump {
        name: "nope".into(),
        columns: vec![],
        rows: vec![],
    });
    let mut tx = fx.store.write().unwrap();
    assert!(tx.load_collection(&bad_table).is_err());
    drop(tx);
    assert_eq!(fx.read(|r| r.dump_collection().unwrap()), good);
}

#[test]
fn purge_rows_removes_an_import_without_a_trace() {
    let fx = Fx::seeded();
    fx.write(|tx| {
        ensure_home(tx);
        tx.insert_deck(&deck(1, None, "Keep")).unwrap();
        add_note(tx, &note(1, "keep", "me", &["old"], 1), 1);
    });
    let before = fx.read(|r| r.dump_collection().unwrap());
    // An "import": a deck, two notes (one reusing tag `old`, one with a new tag), review
    // history, a classification with a value.
    fx.write(|tx| {
        tx.insert_deck(&deck(2, None, "Imported")).unwrap();
        add_note(tx, &note(2, "new one", "x", &["old", "fresh"], 50), 2);
        add_note(tx, &note(3, "new two", "y", &[], 50), 2);
        tx.insert_review_logs(&[review(1, CardId(nid(2).0), 60)])
            .unwrap();
    });
    let after_import = fx.read(|r| r.dump_collection().unwrap());
    assert_ne!(after_import, before);
    fx.write(|tx| {
        tx.purge_rows(&crate::repo::PurgeRows {
            decks: vec![did(2)],
            notetypes: vec![],
            notes: vec![nid(2), nid(3)],
            cards: vec![CardId(nid(2).0), CardId(nid(3).0)],
            attribute_defs: vec![],
            tags_created_since: Some(Timestamp(50)),
        })
        .unwrap()
    });
    let after = fx.read(|r| r.dump_collection().unwrap());
    assert_eq!(after, before);
    let hits: i64 = fx
        .raw()
        .query_row(
            "SELECT count(*) FROM notes_fts WHERE notes_fts MATCH 'new'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(hits, 0);
}

#[test]
fn upgrade_dump_runs_later_data_migrations_on_restored_rows() {
    let fx = Fx::seeded();
    fx.write(|tx| {
        ensure_home(tx);
        tx.insert_deck(&deck(1, None, "Deck")).unwrap();
        add_note(tx, &note(1, "Işık", "back", &[], 1), 1);
    });
    let old = fx.read(|r| r.dump_collection().unwrap());
    // Same schema: unchanged.
    assert_eq!(crate::upgrade_dump(old.clone()).unwrap(), old);

    // A later build with a data migration (like v0.4's 0300 re-folding search text).
    let mut newer: Vec<migrate::Migration> = migrate::MIGRATIONS.to_vec();
    newer.push(migrate::Migration {
        version: 499,
        name: "0499_test_fold",
        sql: "UPDATE notes_fts SET sort_field = replace(sort_field, 'I', 'i');
              CREATE TABLE fold_marker (n INTEGER NOT NULL);
              INSERT INTO fold_marker SELECT count(*) FROM notes;",
    });
    let up = crate::upgrade_dump_with(old.clone(), &newer).unwrap();
    assert_eq!(*up.schema_versions.last().unwrap(), 499);
    let fts = up.tables.iter().find(|t| t.name == "notes_fts").unwrap();
    let col = fts.columns.iter().position(|c| c == "sort_field").unwrap();
    assert_eq!(fts.rows[0][col], SqlValue::Text("işık".into()));
    let marker = up.tables.iter().find(|t| t.name == "fold_marker").unwrap();
    assert_eq!(marker.rows[0].last(), Some(&SqlValue::Integer(1)));
    // Unknown versions are refused.
    let mut alien = old;
    alien.schema_versions.push(9999);
    assert!(crate::upgrade_dump(alien).is_err());
}
