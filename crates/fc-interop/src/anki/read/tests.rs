//! Hostile collections: views, faked schema rows and generated columns are rejected, and
//! every query is interruptible (abort callback, time budget).
#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::super::fixtures::{sample, NOW};
use super::super::write::collection_bytes;
use super::*;
use crate::zipio::ZipOut;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Never terminates on its own.
const ENDLESS: &str = "WITH RECURSIVE r(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM r) \
                       SELECT count(*) FROM r";

/// The sample collection after running `sql` on it, as a legacy `.apkg`.
fn package_after(sql: &str) -> Vec<u8> {
    let bytes = collection_bytes(&sample(), NOW).unwrap();
    let mut db = Connection::open_in_memory().unwrap();
    db.deserialize_read_exact(rusqlite::MAIN_DB, &bytes[..], bytes.len(), false)
        .unwrap();
    db.execute_batch(sql).unwrap();
    let bytes = db.serialize(rusqlite::MAIN_DB).unwrap().to_vec();
    let mut zip = ZipOut::new("apkg");
    zip.add("collection.anki2", &bytes, true).unwrap();
    zip.add("media", b"{}", true).unwrap();
    zip.finish().unwrap()
}

fn assert_rejected_quickly(pkg: &[u8]) {
    let started = Instant::now();
    let e = read_package(pkg).unwrap_err();
    assert!(matches!(e, InteropError::Invalid { .. }), "{e}");
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "took {:?}",
        started.elapsed()
    );
}

#[test]
fn a_view_named_like_a_table_is_rejected_without_running_it() {
    for table in BASE_TABLES {
        let pkg = package_after(&format!(
            "DROP TABLE {table}; CREATE VIEW {table} AS \
             WITH RECURSIVE r(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM r) SELECT x FROM r;"
        ));
        assert_rejected_quickly(&pkg);
    }
}

#[test]
fn a_view_disguised_as_a_table_in_sqlite_master_is_rejected() {
    // `sqlite_master` claims `type = 'table'`, but the SQL it holds is a view (SQLite
    // itself refuses this schema; `pragma_table_list` would catch it otherwise).
    let pkg = package_after(
        "DROP TABLE notes;
         CREATE TABLE notes (id INTEGER PRIMARY KEY);
         PRAGMA writable_schema = ON;
         UPDATE sqlite_master SET sql = 'CREATE VIEW notes AS WITH RECURSIVE \
           r(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM r) SELECT x AS id FROM r' \
           WHERE name = 'notes';
         PRAGMA writable_schema = OFF;",
    );
    assert_rejected_quickly(&pkg);
}

#[test]
fn a_virtual_table_is_rejected() {
    // Its `sqlite_master` row legitimately says `type = 'table'`.
    let pkg = package_after(
        "DROP TABLE revlog;
         CREATE VIRTUAL TABLE revlog USING fts5(id, cid, ease, ivl, lastIvl, factor, time, type);",
    );
    assert_rejected_quickly(&pkg);
}

#[test]
fn generated_columns_are_rejected() {
    let pkg = package_after("ALTER TABLE notes ADD COLUMN boom GENERATED ALWAYS AS (1) VIRTUAL;");
    assert_rejected_quickly(&pkg);
}

#[test]
fn plain_collections_still_read() {
    let col = read_package_with_abort(&package_after("SELECT 1;"), || false).unwrap();
    assert_eq!(col.notes.len(), sample().notes.len());
}

#[test]
fn the_abort_callback_interrupts_a_running_query() {
    let db = collection_bytes(&sample(), NOW).unwrap();
    // Already aborted: the endless query stops at the first check.
    let conn = open_guarded(&db, || true, QUERY_TIME_BUDGET).unwrap();
    let started = Instant::now();
    assert!(conn.query_row(ENDLESS, [], |r| r.get::<_, i64>(0)).is_err());
    assert!(started.elapsed() < Duration::from_secs(10));

    // Cancelled from another thread while the query runs.
    let flag = Arc::new(AtomicBool::new(false));
    let seen = Arc::clone(&flag);
    let conn = open_guarded(&db, move || seen.load(Ordering::SeqCst), QUERY_TIME_BUDGET).unwrap();
    let canceller = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(50));
        flag.store(true, Ordering::SeqCst);
    });
    let started = Instant::now();
    let e = conn
        .query_row(ENDLESS, [], |r| r.get::<_, i64>(0))
        .unwrap_err();
    canceller.join().unwrap();
    assert_eq!(
        e.sqlite_error_code(),
        Some(rusqlite::ErrorCode::OperationInterrupted)
    );
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[test]
fn the_time_budget_interrupts_a_running_query() {
    let db = collection_bytes(&sample(), NOW).unwrap();
    let conn = open_guarded(&db, || false, Duration::from_millis(50)).unwrap();
    let started = Instant::now();
    let e = conn
        .query_row(ENDLESS, [], |r| r.get::<_, i64>(0))
        .unwrap_err();
    assert_eq!(
        e.sqlite_error_code(),
        Some(rusqlite::ErrorCode::OperationInterrupted)
    );
    assert!(started.elapsed() < Duration::from_secs(10));
}
