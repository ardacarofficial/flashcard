use super::{deck, did, Fx};
use crate::error::StorageError;
use crate::migrate;
use crate::store::{SqliteStore, Store, READ_POOL_SIZE};
use std::sync::{Arc, Barrier};

#[test]
fn in_memory_store_works_end_to_end() {
    let store = SqliteStore::open_in_memory().unwrap();
    assert_eq!(store.schema_version().unwrap(), migrate::latest_version());
    assert!(store.path().is_none());
    let mut tx = store.write().unwrap();
    tx.insert_deck(&deck(1, None, "A")).unwrap();
    tx.commit().unwrap();
    let r = store.read().unwrap();
    assert_eq!(r.list_decks(false).unwrap().len(), 1);
}

#[test]
fn dropping_a_write_tx_rolls_back() {
    let fx = Fx::new();
    {
        let mut tx = fx.store.write().unwrap();
        tx.insert_deck(&deck(1, None, "A")).unwrap();
        assert!(
            tx.get_deck(did(1)).unwrap().is_some(),
            "visible inside the tx"
        );
    }
    fx.read(|r| assert!(r.get_deck(did(1)).unwrap().is_none()));
    // The writer is usable again afterwards.
    fx.write(|tx| tx.insert_deck(&deck(1, None, "A")).unwrap());
    fx.read(|r| assert!(r.get_deck(did(1)).unwrap().is_some()));
}

#[test]
fn failed_statement_leaves_tx_usable_and_constraint_is_classified() {
    let fx = Fx::new();
    fx.write(|tx| {
        tx.insert_deck(&deck(1, None, "A")).unwrap();
        let dup = tx.insert_deck(&deck(2, None, "a")).unwrap_err();
        match dup {
            StorageError::Constraint { constraint } => {
                assert_eq!(constraint, "decks_sibling_name")
            }
            other => panic!("{other:?}"),
        }
        let fk = tx.insert_deck(&deck(3, Some(99), "B")).unwrap_err();
        assert!(
            matches!(&fk, StorageError::Constraint { constraint } if constraint == "foreign_key"),
            "{fk:?}"
        );
        tx.insert_deck(&deck(4, None, "C")).unwrap();
    });
    fx.read(|r| assert_eq!(r.list_decks(false).unwrap().len(), 2));
}

#[test]
fn read_snapshot_is_isolated_from_later_commits() {
    let fx = Fx::new();
    fx.write(|tx| tx.insert_deck(&deck(1, None, "A")).unwrap());
    let snapshot = fx.store.read().unwrap();
    assert_eq!(snapshot.list_decks(false).unwrap().len(), 1);
    // A write commits while the snapshot is open (WAL: no blocking).
    fx.write(|tx| tx.insert_deck(&deck(2, None, "B")).unwrap());
    assert_eq!(snapshot.list_decks(false).unwrap().len(), 1);
    drop(snapshot);
    fx.read(|r| assert_eq!(r.list_decks(false).unwrap().len(), 2));
}

#[test]
fn readers_run_while_a_write_is_open() {
    let fx = Fx::new();
    fx.write(|tx| tx.insert_deck(&deck(1, None, "A")).unwrap());
    let mut w = fx.store.write().unwrap();
    w.insert_deck(&deck(2, None, "B")).unwrap();
    // All pool readers can be held at once, alongside the writer.
    let readers: Vec<_> = (0..READ_POOL_SIZE)
        .map(|_| fx.store.read().unwrap())
        .collect();
    for r in &readers {
        assert_eq!(
            r.list_decks(false).unwrap().len(),
            1,
            "uncommitted not visible"
        );
    }
    drop(readers);
    w.commit().unwrap();
    fx.read(|r| assert_eq!(r.list_decks(false).unwrap().len(), 2));
}

#[test]
fn writers_are_serialized_across_threads() {
    let fx = Fx::new();
    let store = Arc::new(fx.store);
    let threads = 4;
    let per = 10;
    let barrier = Arc::new(Barrier::new(threads));
    let handles: Vec<_> = (0..threads)
        .map(|t| {
            let store = store.clone();
            let barrier = barrier.clone();
            std::thread::spawn(move || {
                barrier.wait();
                for i in 0..per {
                    let n = (t * per + i) as u128;
                    let mut tx = store.write().unwrap();
                    tx.insert_deck(&deck(n, None, &format!("D{n}"))).unwrap();
                    tx.commit().unwrap();
                    let r = store.read().unwrap();
                    assert!(r.get_deck(did(n)).unwrap().is_some());
                }
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    let r = store.read().unwrap();
    assert_eq!(r.list_decks(false).unwrap().len(), threads * per);
}

#[test]
fn reader_connections_are_query_only() {
    let fx = Fx::new();
    // Through the API a ReadTx exposes no write methods; the pragma is a second guard.
    let conn = rusqlite::Connection::open(fx.db_path()).unwrap();
    migrate::configure_reader(&conn).unwrap();
    assert!(conn
        .execute(
            "INSERT INTO module_state (id, enabled, updated_at) VALUES ('x', 1, 0)",
            []
        )
        .is_err());
}

/// v1.4 Z2: WAL checkpoints run on the store's own thread, not inside a commit. Small
/// commits never trigger one (a complete checkpoint makes the next commit start the WAL over
/// and sync it, so checkpointing after every pause put a sync into the next command); a WAL
/// past [`BACKGROUND_CHECKPOINT_PAGES`] is checkpointed by the thread, every frame; dropping
/// the store stops it (the WAL is removed).
#[test]
fn wal_checkpoints_run_off_the_write_path() {
    use crate::store::{BACKGROUND_CHECKPOINT_PAGES, SAFETY_CHECKPOINT_PAGES};
    use std::time::{Duration, Instant};
    const { assert!(SAFETY_CHECKPOINT_PAGES > BACKGROUND_CHECKPOINT_PAGES) };
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("collection.db");
    let store = SqliteStore::open(&path).unwrap();
    let put = |key: &str, value: serde_json::Value| {
        let mut tx = store.write().unwrap();
        tx.set_ux_state(key, &value, fc_core::Timestamp(1)).unwrap();
        tx.commit().unwrap();
    };
    // Bursts of small commits with pauses: no checkpoint (the WAL is not started over).
    for round in 0..3 {
        for n in 0..5 {
            put(&format!("k{round}.{n}"), serde_json::json!(n));
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    assert_eq!(
        store.checkpoints_run(),
        Some(0),
        "small commits: no checkpoint"
    );
    // A WAL past the threshold: the thread checkpoints it.
    let big = serde_json::json!("x".repeat(64 * 1024));
    for n in 0..100 {
        put(&format!("big{n}"), big.clone());
    }
    let deadline = Instant::now() + Duration::from_secs(30);
    while store.checkpoints_run() == Some(0) {
        assert!(Instant::now() < deadline, "no checkpoint ran");
        std::thread::sleep(Duration::from_millis(20));
    }
    // Everything written so far is in the database file.
    let raw = rusqlite::Connection::open(&path).unwrap();
    let (log, done): (i64, i64) = raw
        .query_row("PRAGMA wal_checkpoint(PASSIVE)", [], |r| {
            Ok((r.get(1)?, r.get(2)?))
        })
        .unwrap();
    assert_eq!(log, done, "every WAL frame was checkpointed");
    drop(raw);
    drop(store);
    assert!(
        !path.with_extension("db-wal").exists(),
        "the last close checkpoints and removes the WAL"
    );
    // In memory: no thread.
    assert!(SqliteStore::open_in_memory()
        .unwrap()
        .checkpoints_run()
        .is_none());
}

/// v1.4.2 review (J1): SQLite rolls the WHOLE transaction back, and leaves the connection in
/// autocommit mode, when a write statement is stopped (a caller's deadline, a full disk, an
/// I/O error). Writes made afterwards through the same transaction handle were then committed
/// one by one, outside any transaction: half a command stayed in the collection although its
/// commit failed. After such a stop the handle refuses every further write.
#[test]
fn nothing_is_written_after_sqlite_rolled_the_transaction_back() {
    use fc_core::tag::Tag;
    use fc_core::Timestamp;
    let fx = Fx::new();
    fx.write(|tx| {
        for i in 0..3000 {
            let name = super::tag(&format!("t{i}"));
            tx.upsert_tag(&Tag {
                id: super::tid_of(&name),
                name,
                color: None,
                created_at: Timestamp(1),
                updated_at: Timestamp(1),
            })
            .unwrap();
        }
    });
    let mut tx = fx.store.write().unwrap();
    tx.insert_deck(&deck(1, None, "before")).unwrap();
    let stopped = {
        let _g = crate::interrupt::deadline(std::time::Instant::now());
        tx.delete_unused_tags().unwrap_err()
    };
    assert!(matches!(stopped, StorageError::Interrupted), "{stopped:?}");
    // A caller that carries on (a plugin's next host call, a best-effort write).
    let after = tx.insert_deck(&deck(2, None, "after"));
    assert!(after.is_err(), "a write after the rollback must fail");
    assert!(
        tx.commit().is_err(),
        "the transaction is gone: it cannot commit"
    );
    fx.read(|r| {
        assert_eq!(
            r.list_decks(false).unwrap().len(),
            0,
            "nothing of it is kept"
        );
        assert_eq!(r.list_tags().unwrap().len(), 3000);
    });
    // The next transaction on the same connection works as before.
    fx.write(|tx| tx.insert_deck(&deck(3, None, "next")).unwrap());
    fx.read(|r| assert_eq!(r.list_decks(false).unwrap().len(), 1));
}
