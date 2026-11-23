//! `leech_repairs` (v1.4 W3): migration `1470`, round trip, the kind check, explicit ids.

use super::{cid, nid, Fx};
use crate::migrate::MIGRATIONS;
use crate::repo::LeechRepairRecord;
use crate::store::{SqliteStore, Store};
use fc_core::Timestamp;

fn repair(card: u128, kind: &str, at: i64) -> LeechRepairRecord {
    LeechRepairRecord {
        id: 0,
        card_id: cid(card),
        note_id: nid(card),
        kind: kind.into(),
        detail: format!(r#"{{"at":{at}}}"#),
        lapses: 8,
        applied_at: Timestamp(at),
    }
}

/// An existing collection gets the table; its data is untouched.
#[test]
fn migration_1470_adds_leech_repairs_to_an_existing_collection() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("collection.db");
    let before: Vec<_> = MIGRATIONS
        .iter()
        .filter(|m| m.version < 1470)
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
                "SELECT count(*) FROM sqlite_master WHERE name = 'leech_repairs'",
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
            .insert_leech_repair(&repair(1, "switch_example", 100))
            .unwrap();
        tx.commit().unwrap();
        id
    };
    let rows = store.read().unwrap().list_leech_repairs(None).unwrap();
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

/// Insert, list (all and by card, oldest first), get and delete.
#[test]
fn leech_repairs_round_trip() {
    let fx = Fx::new();
    let ids = fx.write(|tx| {
        [
            repair(1, "ai_rewrite", 300),
            repair(2, "add_accepted_answers", 100),
            repair(1, "switch_example", 200),
            repair(3, "move_meaning", 200),
        ]
        .iter()
        .map(|r| tx.insert_leech_repair(r).unwrap())
        .collect::<Vec<_>>()
    });
    assert!(ids.iter().all(|id| *id > 0));
    let all = fx.read(|r| r.list_leech_repairs(None).unwrap());
    let order: Vec<i64> = all.iter().map(|r| r.id).collect();
    // applied_at, then id (ids[2] and ids[3] share 200).
    assert_eq!(order, vec![ids[1], ids[2], ids[3], ids[0]]);
    assert_eq!(
        all[0],
        LeechRepairRecord {
            id: ids[1],
            ..repair(2, "add_accepted_answers", 100)
        }
    );

    let of_1 = fx.read(|r| r.list_leech_repairs(Some(&[cid(1)])).unwrap());
    let kinds: Vec<&str> = of_1.iter().map(|r| r.kind.as_str()).collect();
    assert_eq!(kinds, vec!["switch_example", "ai_rewrite"]);
    let of_2_3 = fx.read(|r| r.list_leech_repairs(Some(&[cid(3), cid(2)])).unwrap());
    assert_eq!(of_2_3.len(), 2);
    assert!(fx
        .read(|r| r.list_leech_repairs(Some(&[])).unwrap())
        .is_empty());
    assert!(fx
        .read(|r| r.list_leech_repairs(Some(&[cid(9)])).unwrap())
        .is_empty());

    let got = fx.read(|r| r.get_leech_repair(ids[0]).unwrap()).unwrap();
    assert_eq!(got.kind, "ai_rewrite");
    assert_eq!(got.detail, r#"{"at":300}"#);
    assert_eq!(got.lapses, 8);
    assert_eq!(fx.read(|r| r.get_leech_repair(9_999).unwrap()), None);

    fx.write(|tx| tx.delete_leech_repair(ids[0]).unwrap());
    assert_eq!(fx.read(|r| r.get_leech_repair(ids[0]).unwrap()), None);
    // Missing: no-op.
    fx.write(|tx| tx.delete_leech_repair(ids[0]).unwrap());
    assert_eq!(fx.read(|r| r.list_leech_repairs(None).unwrap()).len(), 3);
}

/// The table refuses a kind it does not know.
#[test]
fn leech_repairs_refuse_an_unknown_kind() {
    let fx = Fx::new();
    let mut tx = fx.store.write().unwrap();
    assert!(tx.insert_leech_repair(&repair(1, "magic", 100)).is_err());
    drop(tx);
    assert!(fx
        .raw()
        .execute(
            "INSERT INTO leech_repairs (card_id, note_id, kind, lapses, applied_at)
             VALUES ('x', 'y', 'lost', 0, 1)",
            [],
        )
        .is_err());
}

/// An explicit id (restore after undo) is kept; a taken one is refused; new ids continue
/// after it.
#[test]
fn leech_repairs_insert_with_an_explicit_id_keeps_it() {
    let fx = Fx::new();
    let first = fx.write(|tx| {
        tx.insert_leech_repair(&repair(1, "switch_example", 100))
            .unwrap()
    });
    let restored = fx.write(|tx| {
        tx.insert_leech_repair(&LeechRepairRecord {
            id: 42,
            ..repair(2, "move_meaning", 200)
        })
        .unwrap()
    });
    assert_eq!(restored, 42);
    assert_eq!(
        fx.read(|r| r.get_leech_repair(42).unwrap())
            .unwrap()
            .card_id,
        cid(2)
    );
    let mut tx = fx.store.write().unwrap();
    assert!(tx
        .insert_leech_repair(&LeechRepairRecord {
            id: first,
            ..repair(3, "ai_rewrite", 300)
        })
        .is_err());
    drop(tx);
    let next = fx.write(|tx| {
        tx.insert_leech_repair(&repair(4, "ai_rewrite", 400))
            .unwrap()
    });
    assert!(next > 42);
}
