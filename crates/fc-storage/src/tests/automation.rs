//! `automation_rules` / `automation_runs` (v0.9): migration `0850` and the repository round
//! trip.

use super::{u, Fx};
use crate::migrate::MIGRATIONS;
use crate::repo::{AutomationRuleRecord, AutomationRunRecord};
use crate::store::{SqliteStore, Store};
use fc_core::{AutomationRuleId, Timestamp};
use serde_json::json;

fn rule(n: u128, position: u32) -> AutomationRuleRecord {
    AutomationRuleRecord {
        id: AutomationRuleId(u(n)),
        name: format!("rule {n}"),
        enabled: true,
        position,
        trigger: json!({"kind": "card_answered", "ratings": ["again"]}),
        condition: "lapses>=8".into(),
        actions: json!([{"method": "bulk.add_tags", "params": {"tags": ["leech"]}}]),
        last_run_at: None,
        created_at: Timestamp(1),
        updated_at: Timestamp(2),
        deleted_at: None,
    }
}

fn run(rule: AutomationRuleId, at: i64) -> AutomationRunRecord {
    AutomationRunRecord {
        seq: 0,
        rule_id: rule,
        at: Timestamp(at),
        trigger: "card_answered".into(),
        depth: 1,
        matched: 3,
        status: "applied".into(),
        batch_id: None,
        error: Some(json!({"code": "PERMISSION_DENIED"})),
    }
}

#[test]
fn migration_0850_adds_automation_tables_to_an_existing_database() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("collection.db");
    // A Phase-3 database (everything before the v0.9 range).
    let before: Vec<_> = MIGRATIONS
        .iter()
        .filter(|m| m.version < 800)
        .copied()
        .collect();
    drop(SqliteStore::open_with_migrations(&path, &before).unwrap());
    let tables = |conn: &rusqlite::Connection| -> i64 {
        conn.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table'
             AND name IN ('automation_rules', 'automation_runs')",
            [],
            |r| r.get(0),
        )
        .unwrap()
    };
    assert_eq!(tables(&rusqlite::Connection::open(&path).unwrap()), 0);

    let store = SqliteStore::open(&path).unwrap();
    assert_eq!(tables(&rusqlite::Connection::open(&path).unwrap()), 2);
    let mut tx = store.write().unwrap();
    tx.upsert_automation_rule(&rule(1, 0)).unwrap();
    tx.commit().unwrap();
    assert_eq!(
        store.read().unwrap().list_automation_rules(false).unwrap(),
        vec![rule(1, 0)]
    );
}

#[test]
fn rules_round_trip_order_and_trash() {
    let fx = Fx::new();
    let mut tx = fx.store.write().unwrap();
    tx.upsert_automation_rule(&rule(1, 1)).unwrap();
    tx.upsert_automation_rule(&rule(2, 0)).unwrap();
    let mut trashed = rule(3, 2);
    trashed.deleted_at = Some(Timestamp(9));
    tx.upsert_automation_rule(&trashed).unwrap();
    tx.set_automation_rule_last_run(AutomationRuleId(u(1)), Timestamp(77))
        .unwrap();
    tx.commit().unwrap();

    let r = fx.store.read().unwrap();
    let live: Vec<_> = r
        .list_automation_rules(false)
        .unwrap()
        .into_iter()
        .map(|x| x.id)
        .collect();
    assert_eq!(live, vec![AutomationRuleId(u(2)), AutomationRuleId(u(1))]);
    assert_eq!(r.list_automation_rules(true).unwrap().len(), 3);
    assert_eq!(
        r.get_automation_rule(AutomationRuleId(u(3))).unwrap(),
        Some(trashed)
    );
    let one = r
        .get_automation_rule(AutomationRuleId(u(1)))
        .unwrap()
        .unwrap();
    assert_eq!(one.last_run_at, Some(Timestamp(77)));
    assert_eq!(one.actions, rule(1, 1).actions);
}

#[test]
fn run_log_is_newest_first_trimmed_and_removed_with_its_rule() {
    let fx = Fx::new();
    let (a, b) = (AutomationRuleId(u(1)), AutomationRuleId(u(2)));
    let mut tx = fx.store.write().unwrap();
    tx.upsert_automation_rule(&rule(1, 0)).unwrap();
    let mut seqs = Vec::new();
    for at in 0..5 {
        seqs.push(tx.insert_automation_run(&run(a, at), 3).unwrap());
    }
    tx.insert_automation_run(&run(b, 10), 3).unwrap();
    tx.commit().unwrap();
    assert!(seqs.windows(2).all(|w| w[0] < w[1]));

    let r = fx.store.read().unwrap();
    let all = r.list_automation_runs(None, 50).unwrap();
    // Trimmed to the newest 3, newest first.
    assert_eq!(
        all.iter().map(|x| x.at.0).collect::<Vec<_>>(),
        vec![10, 4, 3]
    );
    let of_a = r.list_automation_runs(Some(a), 1).unwrap();
    assert_eq!(of_a.len(), 1);
    assert_eq!(of_a[0].at, Timestamp(4));
    assert_eq!(of_a[0].error, Some(json!({"code": "PERMISSION_DENIED"})));
    drop(r);

    let mut tx = fx.store.write().unwrap();
    assert!(tx.delete_automation_rule(a).unwrap());
    assert!(!tx.delete_automation_rule(a).unwrap());
    tx.commit().unwrap();
    let r = fx.store.read().unwrap();
    assert!(r.list_automation_runs(Some(a), 50).unwrap().is_empty());
    assert_eq!(r.list_automation_runs(None, 50).unwrap().len(), 1);
}
