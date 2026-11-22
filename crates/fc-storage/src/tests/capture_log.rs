//! `subtitle_capture_log` (v1.4 A5): migration `1430`, order and the bound.

use super::{cid, Fx};
use crate::migrate::MIGRATIONS;
use crate::repo::CaptureLogRecord;
use crate::store::{SqliteStore, Store};
use fc_core::Timestamp;

fn attempt(n: i64, result: &str) -> CaptureLogRecord {
    CaptureLogRecord {
        seq: 0,
        capture_id: format!("0190a000-0000-7000-8000-{n:012}"),
        at: Timestamp(1_000 + n),
        result: result.into(),
        reason: (result == "rejected").then(|| "invalid_capture:term.lemma".into()),
        card_id: (result != "rejected").then(|| cid(n as u128)),
        duration_ms: n,
    }
}

/// An existing v1.3 collection gets the table; its data is untouched.
#[test]
fn migration_1430_adds_the_capture_log_to_an_existing_collection() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("collection.db");
    let before: Vec<_> = MIGRATIONS
        .iter()
        .filter(|m| m.version < 1430)
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
                "SELECT count(*) FROM sqlite_master WHERE name = 'subtitle_capture_log'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 0);
    }
    let store = SqliteStore::open(&path).unwrap();
    {
        let mut tx = store.write().unwrap();
        tx.append_capture_log(&attempt(1, "created"), 500).unwrap();
        tx.commit().unwrap();
    }
    let rows = store.read().unwrap().recent_capture_log(10).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].result, "created");
    let conn = rusqlite::Connection::open(&path).unwrap();
    let locale: String = conn
        .query_row(
            "SELECT value FROM settings WHERE key = 'app.locale'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(locale, "\"tr\"");
    // Unknown results are refused by the table itself.
    assert!(conn
        .execute(
            "INSERT INTO subtitle_capture_log (capture_id, at, result, duration_ms) VALUES ('x', 1, 'lost', 0)",
            [],
        )
        .is_err());
}

/// Newest first; only the newest `keep` rows survive.
#[test]
fn capture_log_keeps_the_newest_rows() {
    let fx = Fx::new();
    fx.write(|tx| {
        for n in 1..=12 {
            let r = if n % 3 == 0 { "rejected" } else { "created" };
            tx.append_capture_log(&attempt(n, r), 5).unwrap();
        }
    });
    let rows = fx.read(|r| r.recent_capture_log(100).unwrap());
    let ns: Vec<i64> = rows.iter().map(|r| r.duration_ms).collect();
    assert_eq!(ns, vec![12, 11, 10, 9, 8]);
    assert_eq!(
        rows[0].reason.as_deref(),
        Some("invalid_capture:term.lemma")
    );
    assert_eq!(rows[0].card_id, None);
    assert_eq!(rows[1].card_id, Some(cid(11)));
    assert!(rows.windows(2).all(|w| w[0].seq > w[1].seq));
    assert_eq!(fx.read(|r| r.recent_capture_log(2).unwrap()).len(), 2);
    // keep 0 keeps none.
    fx.write(|tx| tx.append_capture_log(&attempt(13, "duplicate"), 0).unwrap());
    assert!(fx.read(|r| r.recent_capture_log(100).unwrap()).is_empty());
}
