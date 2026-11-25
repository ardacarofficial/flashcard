//! v1.4.2 K1 (review finding 11): two processes open the collection at the same moment and
//! both find the same migration pending. The second must wait for the first and then see the
//! migration done — not run it again and fail to open.
//!
//! Two connections to one file lock each other exactly as two processes do.

use crate::migrate::{self, Migration};
use crate::store::{SqliteStore, Store};
use rusqlite::Connection;
use std::sync::mpsc;
use std::sync::{Arc, Barrier};

const LIST: &[Migration] = &[
    Migration {
        version: 1,
        name: "0001_first",
        sql: "CREATE TABLE first (id INTEGER PRIMARY KEY) STRICT;",
    },
    Migration {
        version: 2,
        name: "0002_second",
        sql: "CREATE TABLE second (id INTEGER PRIMARY KEY) STRICT; INSERT INTO second VALUES (1);",
    },
];

fn connect(path: &std::path::Path) -> Connection {
    let conn = Connection::open(path).unwrap();
    migrate::configure_connection(&conn).unwrap();
    conn
}

/// Deterministic: the second opener has decided what is pending, then the first applies
/// everything, then the second goes on with its stale list.
#[test]
fn a_migration_another_opener_applied_meanwhile_is_not_run_again() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("c.db");
    let mut first = connect(&path);
    let mut second = connect(&path);
    // The second pauses right after its "what is pending?" transaction: committed, the
    // writer released (that is when SQLite calls the WAL hook).
    let (paused_tx, paused_rx) = mpsc::channel::<()>();
    let (resume_tx, resume_rx) = mpsc::channel::<()>();
    let mut once = Some((paused_tx, resume_rx));
    second
        .wal_hook(Some(move |_: &mut rusqlite::hooks::Wal, _| {
            if let Some((paused, resume)) = once.take() {
                let _ = paused.send(());
                let _ = resume.recv();
            }
            Ok(())
        }))
        .unwrap();
    let late = std::thread::spawn(move || {
        let applied = migrate::migrate_with(&mut second, LIST);
        (applied, second)
    });
    paused_rx.recv().unwrap();
    assert_eq!(migrate::migrate_with(&mut first, LIST).unwrap(), vec![1, 2]);
    resume_tx.send(()).unwrap();
    let (applied, second) = late.join().unwrap();
    assert_eq!(
        applied.unwrap(),
        Vec::<u32>::new(),
        "nothing was left for the second"
    );
    assert_eq!(migrate::current_version(&second).unwrap(), 2);
    let rows: i64 = second
        .query_row("SELECT count(*) FROM second", [], |r| r.get(0))
        .unwrap();
    assert_eq!(rows, 1, "the migration ran once");
    let recorded: i64 = second
        .query_row("SELECT count(*) FROM schema_migrations", [], |r| r.get(0))
        .unwrap();
    assert_eq!(recorded, 2);
}

/// The first opener's migration takes longer than the second's busy timeout: the second
/// waits instead of failing to open.
#[test]
fn the_second_opener_waits_for_a_migration_longer_than_the_busy_timeout() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("c.db");
    let first = connect(&path);
    let mut second = connect(&path);
    // Far below the time the first holds the writer; the real timeout is 5 s.
    second.pragma_update(None, "busy_timeout", 20).unwrap();
    first.execute_batch("BEGIN IMMEDIATE").unwrap();
    let late = std::thread::spawn(move || {
        let applied = migrate::migrate_with(&mut second, LIST);
        (applied, second)
    });
    // The "long migration" of the first: it holds the writer for many busy timeouts.
    std::thread::sleep(std::time::Duration::from_millis(600));
    first
        .execute_batch("CREATE TABLE unrelated (id INTEGER PRIMARY KEY) STRICT; COMMIT")
        .unwrap();
    let (applied, second) = late.join().unwrap();
    assert_eq!(applied.unwrap(), vec![1, 2]);
    assert_eq!(migrate::current_version(&second).unwrap(), 2);
}

/// The real thing: several openers of one new collection at once, each running the whole
/// migration list. Every one of them must end up with an open, current store.
#[test]
fn many_openers_of_one_new_collection_all_succeed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("collection.db");
    const OPENERS: usize = 6;
    let gate = Arc::new(Barrier::new(OPENERS));
    let handles: Vec<_> = (0..OPENERS)
        .map(|_| {
            let (gate, path) = (gate.clone(), path.clone());
            std::thread::spawn(move || {
                gate.wait();
                SqliteStore::open(&path).map(|s| s.schema_version().unwrap())
            })
        })
        .collect();
    for h in handles {
        let version = h.join().unwrap().expect("every opener gets the collection");
        assert_eq!(version, migrate::latest_version());
    }
    // An existing collection that needs migrating: each opener may take its snapshot.
    let old = dir.path().join("old.db");
    drop(SqliteStore::open_with_migrations(&old, &migrate::MIGRATIONS[..1]).unwrap());
    let gate = Arc::new(Barrier::new(OPENERS));
    let handles: Vec<_> = (0..OPENERS)
        .map(|_| {
            let (gate, old) = (gate.clone(), old.clone());
            std::thread::spawn(move || {
                gate.wait();
                SqliteStore::open(&old).map(|s| s.schema_version().unwrap())
            })
        })
        .collect();
    for h in handles {
        let version = h
            .join()
            .unwrap()
            .expect("every opener gets the migrated collection");
        assert_eq!(version, migrate::latest_version());
    }
}
