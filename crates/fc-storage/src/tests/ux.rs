//! `ux_state` (v1.0): migration `1000` on an existing database and the key/value round trip.

use super::Fx;
use crate::migrate::MIGRATIONS;
use crate::repo::{UX_ONBOARDING_DONE, UX_REMINDER_LAST_DAY};
use crate::store::{SqliteStore, Store};
use fc_core::Timestamp;
use serde_json::json;

#[test]
fn migration_1000_adds_ux_state_to_a_v010_database() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("collection.db");
    // A v0.10 database (everything before the v1.0 range).
    let before: Vec<_> = MIGRATIONS
        .iter()
        .filter(|m| m.version < 1000)
        .copied()
        .collect();
    drop(SqliteStore::open_with_migrations(&path, &before).unwrap());
    let tables = |p: &std::path::Path| -> i64 {
        rusqlite::Connection::open(p)
            .unwrap()
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type = 'table' AND name = 'ux_state'",
                [],
                |r| r.get(0),
            )
            .unwrap()
    };
    assert_eq!(tables(&path), 0);
    let store = SqliteStore::open(&path).unwrap();
    assert_eq!(tables(&path), 1);
    let mut tx = store.write().unwrap();
    tx.set_ux_state(UX_ONBOARDING_DONE, &json!(true), Timestamp(5))
        .unwrap();
    tx.commit().unwrap();
    assert_eq!(
        store.read().unwrap().ux_state(UX_ONBOARDING_DONE).unwrap(),
        Some(json!(true))
    );
}

#[test]
fn ux_state_round_trips_and_overwrites() {
    let fx = Fx::new();
    let mut tx = fx.store.write().unwrap();
    assert_eq!(tx.ux_state(UX_REMINDER_LAST_DAY).unwrap(), None);
    tx.set_ux_state(UX_REMINDER_LAST_DAY, &json!(20_777), Timestamp(1))
        .unwrap();
    tx.set_ux_state(UX_REMINDER_LAST_DAY, &json!(20_778), Timestamp(2))
        .unwrap();
    tx.commit().unwrap();
    let r = fx.store.read().unwrap();
    assert_eq!(
        r.ux_state(UX_REMINDER_LAST_DAY).unwrap(),
        Some(json!(20_778))
    );
    // Rolled-back writes leave nothing behind.
    drop(r);
    let mut tx = fx.store.write().unwrap();
    tx.set_ux_state(UX_ONBOARDING_DONE, &json!(true), Timestamp(3))
        .unwrap();
    drop(tx);
    assert_eq!(
        fx.store
            .read()
            .unwrap()
            .ux_state(UX_ONBOARDING_DONE)
            .unwrap(),
        None
    );
}
