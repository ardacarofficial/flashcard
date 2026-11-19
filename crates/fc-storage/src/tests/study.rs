//! v0.2 study storage: migration 0100, sessions, review log, study-queue candidates.

use super::{add_note, cid, deck, did, nid, note, Fx};
use crate::migrate::{self, MIGRATIONS};
use crate::repo::{ReviewLogRecord, SessionRecord, StudyCardFilter};
use crate::store::{SqliteStore, Store};
use fc_core::attribute::{AttributeDef, AttributeKind, AttributeOption, AttributeTarget};
use fc_core::attribute::{AttributeValue, EntityRef};
use fc_core::card::{CardState, Rating};
use fc_core::{AttributeDefId, AttributeOptionId, ReviewLogId, SessionId, Timestamp};
use rusqlite::Connection;
use serde_json::json;

fn sid(n: u128) -> SessionId {
    SessionId(super::u(0x5000 + n))
}

fn rid(n: u128) -> ReviewLogId {
    ReviewLogId(super::u(0x6000 + n))
}

fn session(n: u128, started: i64, ended: Option<i64>) -> SessionRecord {
    SessionRecord {
        id: sid(n),
        kind: "normal".into(),
        query: None,
        config: json!({ "mode": "scheduled" }),
        state: json!({ "queue": [] }),
        started_at: Timestamp(started),
        ended_at: ended.map(Timestamp),
    }
}

fn review(n: u128, at: i64) -> ReviewLogRecord {
    ReviewLogRecord {
        id: rid(n),
        card_id: cid(0),
        session_id: Some(sid(1)),
        rating: Rating::Good,
        state_before: CardState::New,
        state_after: CardState::Learning,
        scheduled_days: 0,
        elapsed_days: 0,
        duration_ms: 1234,
        answer_duration_ms: Some(1000),
        hints_used: 1,
        mode: "normal".into(),
        stability_after: Some(2.3),
        difficulty_after: Some(2.1),
        created_at: Timestamp(at),
    }
}

/// Migration 0100 on a database created by v0.1 (0001 only, with data): data survives, the
/// new indices exist and the planner uses them; a fresh database gets the same schema.
#[test]
fn migration_0100_upgrades_a_v01_database() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("collection.db");
    {
        let mut conn = Connection::open(&path).unwrap();
        migrate::configure_connection(&conn).unwrap();
        let v01: Vec<_> = MIGRATIONS
            .iter()
            .copied()
            .filter(|m| m.version < 100)
            .collect();
        migrate::migrate_with(&mut conn, &v01).unwrap();
        assert!(!migrate::schema_object_exists(&conn, "review_log_session").unwrap());
        conn.execute_batch(
            "INSERT INTO sessions (id, kind, state, started_at) VALUES ('s1', 'normal', '{}', 5);",
        )
        .unwrap();
    }
    let store = SqliteStore::open(&path).unwrap();
    assert!(store.schema_version().unwrap() >= 100);
    assert!(store.migration_backup().is_some(), "pre-migration snapshot");
    let conn = Connection::open(&path).unwrap();
    for index in ["review_log_session", "sessions_open"] {
        assert!(
            migrate::schema_object_exists(&conn, index).unwrap(),
            "{index}"
        );
    }
    let kept: String = conn
        .query_row("SELECT kind FROM sessions WHERE id = 's1'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(kept, "normal");
    let plan = |sql: &str| -> String {
        conn.prepare(&format!("EXPLAIN QUERY PLAN {sql}"))
            .unwrap()
            .query_map([], |r| r.get::<_, String>(3))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
            .join("\n")
    };
    let p = plan("SELECT id FROM review_log WHERE session_id = 'x'");
    assert!(p.contains("review_log_session"), "{p}");
    let p = plan("SELECT id FROM sessions WHERE ended_at IS NULL ORDER BY started_at DESC");
    assert!(p.contains("sessions_open"), "{p}");
}

#[test]
fn sessions_upsert_and_latest_open() {
    let fx = Fx::new();
    assert!(fx.read(|r| r.latest_open_session().unwrap()).is_none());
    fx.write(|tx| {
        tx.upsert_session(&session(1, 10, None)).unwrap();
        tx.upsert_session(&session(2, 20, None)).unwrap();
        tx.upsert_session(&session(3, 30, Some(31))).unwrap();
    });
    assert_eq!(
        fx.read(|r| r.latest_open_session().unwrap()).unwrap().id,
        sid(2)
    );
    let mut s2 = session(2, 20, Some(25));
    s2.state = json!({ "queue": [], "answered": 3 });
    fx.write(|tx| tx.upsert_session(&s2).unwrap());
    assert_eq!(fx.read(|r| r.get_session(sid(2)).unwrap()).unwrap(), s2);
    assert_eq!(
        fx.read(|r| r.latest_open_session().unwrap()).unwrap().id,
        sid(1)
    );
    assert!(fx.read(|r| r.get_session(sid(9)).unwrap()).is_none());
}

#[test]
fn review_log_roundtrip_order_and_delete() {
    let fx = Fx::seeded();
    fx.write(|tx| {
        tx.insert_deck(&deck(1, None, "D")).unwrap();
        add_note(tx, &note(1, "a", "b", &[], 5), 1);
    });
    let card_id = fc_core::CardId(nid(1).0);
    let mut r1 = review(1, 200);
    r1.card_id = card_id;
    let mut r2 = review(2, 100);
    r2.card_id = card_id;
    r2.session_id = None;
    r2.answer_duration_ms = None;
    r2.stability_after = None;
    r2.rating = Rating::Again;
    fx.write(|tx| {
        tx.insert_review_log(&r1).unwrap();
        tx.insert_review_log(&r2).unwrap();
    });
    assert_eq!(
        fx.read(|r| r.review_logs_for_card(card_id).unwrap()),
        vec![r2.clone(), r1.clone()]
    );
    assert_eq!(
        fx.read(|r| r.get_review_log(rid(1)).unwrap()),
        Some(r1.clone())
    );
    // Duplicate id is a constraint error; delete reports whether a row existed.
    let dup = fx.store.write().unwrap().insert_review_log(&r1);
    assert!(dup.is_err());
    assert!(fx.write(|tx| tx.delete_review_log(rid(1)).unwrap()));
    assert!(!fx.write(|tx| tx.delete_review_log(rid(1)).unwrap()));
    assert_eq!(
        fx.read(|r| r.review_logs_for_card(card_id).unwrap()),
        vec![r2]
    );
}

fn level_def(scope: u128) -> AttributeDef {
    let o = |n: u128, label: &str, order: i32| AttributeOption {
        id: AttributeOptionId(super::u(0x7000 + n)),
        label: label.into(),
        color: None,
        order,
    };
    AttributeDef {
        id: AttributeDefId(super::u(0x7000)),
        scope_deck_id: Some(did(scope)),
        key: "level".into(),
        name: "Seviye".into(),
        kind: AttributeKind::OrderedEnum,
        options: vec![o(1, "A1", 0), o(2, "A2", 1), o(3, "B1", 2)],
        target: AttributeTarget::Note,
        required: false,
        default_value: None,
        position: 0,
        template_id: None,
        created_at: Timestamp(1),
        updated_at: Timestamp(1),
        deleted_at: None,
    }
}

/// Candidates: live cards of live notes; deck/card/state filters; suspended excluded unless
/// asked; classification conditions match a note- or card-level value.
#[test]
fn study_cards_filters() {
    let fx = Fx::seeded();
    let def = level_def(1);
    let a2 = def.options[1].id;
    let b1 = def.options[2].id;
    let card_of = |n: u128| fc_core::CardId(nid(n).0);
    fx.write(|tx| {
        tx.insert_deck(&deck(1, None, "Eng")).unwrap();
        tx.insert_deck(&deck(2, Some(1), "Words")).unwrap();
        for n in 1..=5 {
            add_note(
                tx,
                &note(n, &format!("w{n}"), "x", &[], 10 + n as i64),
                if n == 5 { 2 } else { 1 },
            );
        }
        tx.insert_attribute_def(&def).unwrap();
        // Note-level A2 on notes 1 and 2; card-level A2 on note 3's card; B1 on note 4.
        for n in [1, 2] {
            tx.set_attribute_values(
                EntityRef::Note(nid(n)),
                &[(def.id, Some(AttributeValue::Choice(a2)))],
            )
            .unwrap();
        }
        tx.set_attribute_values(
            EntityRef::Card(card_of(3)),
            &[(def.id, Some(AttributeValue::Choice(a2)))],
        )
        .unwrap();
        tx.set_attribute_values(
            EntityRef::Note(nid(4)),
            &[(def.id, Some(AttributeValue::Choice(b1)))],
        )
        .unwrap();
        // Card 2 suspended; note 1 trashed.
        let mut c2 = tx.get_card(card_of(2)).unwrap().unwrap();
        c2.suspended = true;
        tx.update_card(&c2).unwrap();
        tx.set_notes_deleted(&[nid(1)], Some(Timestamp(99)))
            .unwrap();
    });
    let ids = |f: StudyCardFilter| -> Vec<u128> {
        fx.read(|r| r.study_cards(&f).unwrap())
            .into_iter()
            .map(|c| c.id.0.as_u128() - super::u(0x2000).as_u128())
            .collect()
    };
    assert_eq!(ids(StudyCardFilter::default()), vec![3, 4, 5]);
    assert_eq!(
        ids(StudyCardFilter {
            include_suspended: true,
            ..Default::default()
        }),
        vec![2, 3, 4, 5]
    );
    assert_eq!(
        ids(StudyCardFilter {
            deck_ids: Some(vec![did(2)]),
            ..Default::default()
        }),
        vec![5]
    );
    let level = |values: Vec<AttributeOptionId>| StudyCardFilter {
        attributes: vec![(def.id, values.iter().map(ToString::to_string).collect())],
        include_suspended: true,
        ..Default::default()
    };
    assert_eq!(ids(level(vec![a2])), vec![2, 3]);
    assert_eq!(ids(level(vec![a2, b1])), vec![2, 3, 4]);
    assert_eq!(ids(level(vec![])), Vec::<u128>::new());
    // Two conditions must both hold (same attribute here: impossible).
    let both = StudyCardFilter {
        attributes: vec![
            (def.id, vec![a2.to_string()]),
            (def.id, vec![b1.to_string()]),
        ],
        ..Default::default()
    };
    assert_eq!(ids(both), Vec::<u128>::new());
    assert_eq!(
        ids(StudyCardFilter {
            card_ids: Some(vec![card_of(4), card_of(1)]),
            ..Default::default()
        }),
        vec![4]
    );
    assert_eq!(
        ids(StudyCardFilter {
            states: Some(vec![CardState::Review]),
            ..Default::default()
        }),
        Vec::<u128>::new()
    );
}
