//! `inbox` + `api_clients` (v0.9): migration `0800` and the repository round trips.

use super::{u, Fx};
use crate::migrate::MIGRATIONS;
use crate::repo::{ApiClientRecord, InboxRecord};
use crate::store::{SqliteStore, Store};
use crate::StorageError;
use fc_core::paging::PageRequest;
use fc_core::{ApiClientId, InboxItemId, NoteId, Timestamp};
use serde_json::json;

fn item(n: u128, at: i64, client: Option<(u128, &str)>) -> InboxRecord {
    InboxRecord {
        id: InboxItemId(u(n)),
        source: json!({"kind": "user"}),
        payload: json!({"front": format!("w{n}")}),
        status: "pending".into(),
        note_ids: vec![],
        client_id: client.map(|(c, _)| ApiClientId(u(c))),
        client_ref: client.map(|(_, r)| r.to_owned()),
        created_at: Timestamp(at),
        updated_at: Timestamp(at),
    }
}

fn client(n: u128, hash: &str) -> ApiClientRecord {
    ApiClientRecord {
        id: ApiClientId(u(n)),
        name: "Chrome".into(),
        kind: "browser_extension".into(),
        origin: Some("chrome-extension://abc".into()),
        requested_scopes: vec!["inbox:write".into(), "decks:read".into()],
        scopes: vec![],
        status: "pending".into(),
        token_hash: hash.into(),
        created_at: Timestamp(1),
        updated_at: Timestamp(1),
        last_used_at: None,
    }
}

#[test]
fn migration_0800_extends_old_inbox_rows_and_adds_api_clients() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("collection.db");
    let before: Vec<_> = MIGRATIONS
        .iter()
        .filter(|m| m.version < 800)
        .copied()
        .collect();
    drop(SqliteStore::open_with_migrations(&path, &before).unwrap());
    // A row written to the v0.1 inbox table.
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute(
            "INSERT INTO inbox (id, source, payload, status, created_at)
             VALUES (?1, '{\"kind\":\"user\"}', '{\"front\":\"old\"}', 'pending', 5)",
            [InboxItemId(u(9)).to_string()],
        )
        .unwrap();

    let store = SqliteStore::open(&path).unwrap();
    let old = store
        .read()
        .unwrap()
        .get_inbox_item(InboxItemId(u(9)))
        .unwrap()
        .unwrap();
    assert_eq!(old.updated_at, Timestamp(5));
    assert!(old.note_ids.is_empty());
    assert_eq!(old.client_id, None);
    let mut tx = store.write().unwrap();
    tx.upsert_api_client(&client(1, "h1")).unwrap();
    tx.commit().unwrap();
    assert_eq!(store.read().unwrap().list_api_clients().unwrap().len(), 1);
}

#[test]
fn inbox_round_trip_paging_counts_and_client_refs() {
    let fx = Fx::new();
    fx.write(|tx| {
        for n in 1..=5 {
            tx.upsert_inbox_item(&item(n, i64::try_from(n).unwrap() * 10, None))
                .unwrap();
        }
        tx.upsert_inbox_item(&item(6, 60, Some((1, "r1")))).unwrap();
    });
    let first = fx.read(|r| {
        r.list_inbox(
            "pending",
            &PageRequest {
                cursor: None,
                limit: Some(4),
            },
        )
        .unwrap()
    });
    let ids: Vec<_> = first.items.iter().map(|i| i.id).collect();
    assert_eq!(ids, [6, 5, 4, 3].map(|n| InboxItemId(u(n))));
    let rest = fx.read(|r| {
        r.list_inbox(
            "pending",
            &PageRequest {
                cursor: first.next_cursor.clone(),
                limit: Some(4),
            },
        )
        .unwrap()
    });
    assert_eq!(rest.items.len(), 2);
    assert_eq!(rest.next_cursor, None);
    assert_eq!(fx.read(|r| r.count_inbox("pending").unwrap()), 6);

    // Accepting keeps note ids; the count follows the status.
    let mut accepted = item(2, 20, None);
    accepted.status = "accepted".into();
    accepted.note_ids = vec![NoteId(u(77))];
    accepted.updated_at = Timestamp(99);
    fx.write(|tx| tx.upsert_inbox_item(&accepted).unwrap());
    assert_eq!(
        fx.read(|r| r.get_inbox_item(accepted.id).unwrap()),
        Some(accepted.clone())
    );
    assert_eq!(fx.read(|r| r.count_inbox("pending").unwrap()), 5);
    assert_eq!(fx.read(|r| r.count_inbox("accepted").unwrap()), 1);

    // A client reference is unique per client.
    assert_eq!(
        fx.read(|r| r.inbox_item_by_client_ref(ApiClientId(u(1)), "r1").unwrap())
            .map(|i| i.id),
        Some(InboxItemId(u(6)))
    );
    let mut tx = fx.store.write().unwrap();
    assert!(matches!(
        tx.upsert_inbox_item(&item(7, 70, Some((1, "r1")))),
        Err(StorageError::Constraint { constraint }) if constraint == "inbox_client_ref"
    ));
    // Another client may use the same reference.
    tx.upsert_inbox_item(&item(8, 80, Some((2, "r1")))).unwrap();
    drop(tx);

    assert!(fx.write(|tx| tx.delete_inbox_item(accepted.id).unwrap()));
    assert!(!fx.write(|tx| tx.delete_inbox_item(accepted.id).unwrap()));
    let bad = PageRequest {
        cursor: Some("x".into()),
        limit: None,
    };
    assert!(matches!(
        fx.read(|r| r.list_inbox("pending", &bad)),
        Err(StorageError::InvalidCursor(_))
    ));
}

#[test]
fn api_clients_round_trip_and_token_hashes_are_unique() {
    let fx = Fx::new();
    let a = client(1, "aaa");
    let mut b = client(2, "bbb");
    b.created_at = Timestamp(5);
    fx.write(|tx| {
        tx.upsert_api_client(&a).unwrap();
        tx.upsert_api_client(&b).unwrap();
    });
    assert_eq!(
        fx.read(|r| r.list_api_clients().unwrap()),
        vec![b.clone(), a.clone()]
    );
    assert_eq!(
        fx.read(|r| r.api_client_by_token_hash("aaa").unwrap()),
        Some(a.clone())
    );
    assert_eq!(
        fx.read(|r| r.api_client_by_token_hash("zzz").unwrap()),
        None
    );

    let mut approved = a.clone();
    approved.status = "approved".into();
    approved.scopes = vec!["inbox:write".into()];
    approved.last_used_at = Some(Timestamp(42));
    fx.write(|tx| tx.upsert_api_client(&approved).unwrap());
    assert_eq!(fx.read(|r| r.get_api_client(a.id).unwrap()), Some(approved));

    let mut tx = fx.store.write().unwrap();
    assert!(matches!(
        tx.upsert_api_client(&client(3, "aaa")),
        Err(StorageError::Constraint { constraint }) if constraint == "api_clients_token"
    ));
    drop(tx);
    assert!(fx.write(|tx| tx.delete_api_client(b.id).unwrap()));
    assert_eq!(fx.read(|r| r.list_api_clients().unwrap()).len(), 1);
}
