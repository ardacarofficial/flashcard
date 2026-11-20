//! `drafts` + integrity check (v1.0): migration `1050` and the repository round trip.

use super::Fx;
use crate::migrate::MIGRATIONS;
use crate::repo::DraftRecord;
use crate::store::{SqliteStore, Store};
use fc_core::Timestamp;
use serde_json::json;

fn draft(key: &str, at: i64) -> DraftRecord {
    DraftRecord {
        key: key.into(),
        payload: json!({"fields": {"front": "kılıç", "back": "sword"}, "deck_id": null}),
        created_at: Timestamp(at),
        updated_at: Timestamp(at),
    }
}

#[test]
fn migration_1050_adds_drafts_to_an_existing_database() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("collection.db");
    // A Phase-4 (v0.10) database: everything before the v1.0 range.
    let before: Vec<_> = MIGRATIONS
        .iter()
        .filter(|m| m.version < 1000)
        .copied()
        .collect();
    drop(SqliteStore::open_with_migrations(&path, &before).unwrap());
    let has_drafts = |p: &std::path::Path| -> i64 {
        rusqlite::Connection::open(p)
            .unwrap()
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'drafts'",
                [],
                |r| r.get(0),
            )
            .unwrap()
    };
    assert_eq!(has_drafts(&path), 0);
    let store = SqliteStore::open(&path).unwrap();
    assert_eq!(has_drafts(&path), 1);
    let mut tx = store.write().unwrap();
    tx.upsert_draft(&draft("notes.add", 5)).unwrap();
    tx.commit().unwrap();
    assert_eq!(
        store.read().unwrap().get_draft("notes.add").unwrap(),
        Some(draft("notes.add", 5))
    );
    // The pre-migration copy was taken (docs/10 §6).
    let backups = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().contains(".pre-v"))
        .count();
    assert_eq!(backups, 1);
}

#[test]
fn drafts_round_trip_newest_first_and_keep_their_creation_time() {
    let fx = Fx::new();
    let mut tx = fx.store.write().unwrap();
    tx.upsert_draft(&draft("a", 1)).unwrap();
    tx.upsert_draft(&draft("b", 2)).unwrap();
    let mut again = draft("a", 9);
    again.payload = json!({"fields": {"front": "x"}});
    tx.upsert_draft(&again).unwrap();
    tx.commit().unwrap();

    let r = fx.store.read().unwrap();
    let list = r.list_drafts().unwrap();
    assert_eq!(
        list.iter().map(|d| d.key.as_str()).collect::<Vec<_>>(),
        ["a", "b"]
    );
    assert_eq!(
        list[0].created_at,
        Timestamp(1),
        "creation time survives an update"
    );
    assert_eq!(list[0].updated_at, Timestamp(9));
    assert_eq!(list[0].payload, json!({"fields": {"front": "x"}}));
    drop(r);

    let mut tx = fx.store.write().unwrap();
    assert!(tx.delete_draft("a").unwrap());
    assert!(!tx.delete_draft("a").unwrap());
    tx.commit().unwrap();
    assert_eq!(fx.store.read().unwrap().list_drafts().unwrap().len(), 1);
}

#[test]
fn integrity_check_is_ok_on_a_healthy_collection_and_finds_damage() {
    let fx = Fx::seeded();
    for quick in [true, false] {
        let r = fx.store.read().unwrap().integrity_check(quick, 10).unwrap();
        assert_eq!(r.rows, vec!["ok".to_owned()], "quick={quick}");
        assert_eq!(r.foreign_key_violations, 0);
    }
    // A dangling reference written behind the app's back (foreign keys off) is reported.
    {
        let conn = rusqlite::Connection::open(fx.db_path()).unwrap();
        conn.execute_batch(
            "PRAGMA foreign_keys = OFF;
             INSERT INTO note_tags (note_id, tag_id, position)
             VALUES ('00000000-0000-7000-8000-000000000001', '00000000-0000-7000-8000-000000000002', 0);",
        )
        .unwrap();
    }
    let r = fx.store.read().unwrap().integrity_check(true, 10).unwrap();
    assert!(r.foreign_key_violations >= 1, "{r:?}");
}
