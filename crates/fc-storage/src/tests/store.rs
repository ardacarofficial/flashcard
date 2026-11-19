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
