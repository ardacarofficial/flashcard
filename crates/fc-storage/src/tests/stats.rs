//! v0.6 statistics & planning storage: migration 0500, review-log generation triggers, the
//! statistics cache, due times, exam dates, goals.

use super::{add_note, deck, did, nid, note, Fx};
use crate::migrate::{self, MIGRATIONS};
use crate::repo::{ExamRecord, GoalRecord, ReviewLogRecord, StatsCacheRecord};
use crate::store::{SqliteStore, Store};
use fc_core::card::{CardState, Rating};
use fc_core::{CardId, ExamId, GoalId, ReviewLogId, Timestamp};
use rusqlite::Connection;
use serde_json::json;

fn rid(n: u128) -> ReviewLogId {
    ReviewLogId(super::u(0x6000 + n))
}

fn review(n: u128, card: CardId, at: i64, mode: &str) -> ReviewLogRecord {
    ReviewLogRecord {
        id: rid(n),
        card_id: card,
        session_id: None,
        rating: Rating::Good,
        state_before: CardState::Review,
        state_after: CardState::Review,
        scheduled_days: 3,
        elapsed_days: 2,
        duration_ms: 4000,
        answer_duration_ms: None,
        hints_used: 0,
        mode: mode.into(),
        stability_after: Some(5.0),
        difficulty_after: Some(5.0),
        created_at: Timestamp(at),
    }
}

/// Migration 0500 on a v0.5 database with reviews: rows survive, the new tables and triggers
/// exist, the generation starts at 0.
#[test]
fn migration_0500_upgrades_a_v05_database() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("collection.db");
    {
        let mut conn = Connection::open(&path).unwrap();
        migrate::configure_connection(&conn).unwrap();
        let v05: Vec<_> = MIGRATIONS
            .iter()
            .copied()
            .filter(|m| m.version < 500)
            .collect();
        migrate::migrate_with(&mut conn, &v05).unwrap();
        assert!(!migrate::schema_object_exists(&conn, "stats_day_cache").unwrap());
        conn.execute_batch(
            "INSERT INTO goals (id, kind, target, created_at, updated_at)
             VALUES ('g1', 'streak', 7, 1, 1);",
        )
        .unwrap();
    }
    let store = SqliteStore::open(&path).unwrap();
    assert!(store.schema_version().unwrap() >= 500);
    let conn = Connection::open(&path).unwrap();
    for object in [
        "stats_day_cache",
        "stats_cache_meta",
        "review_log_generation",
        "review_log_generation_on_delete",
        "review_log_generation_on_update",
        "exam_dates",
        "exam_dates_date",
    ] {
        assert!(
            migrate::schema_object_exists(&conn, object).unwrap(),
            "{object}"
        );
    }
    let n: i64 = conn
        .query_row("SELECT n FROM review_log_generation", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 0);
    let kind: String = conn
        .query_row("SELECT kind FROM goals WHERE id = 'g1'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(kind, "streak");
}

/// Appends keep the generation (they are folded incrementally); deletes and updates bump it.
#[test]
fn review_log_mark_tracks_appends_and_changes() {
    let fx = Fx::seeded();
    fx.write(|tx| {
        tx.insert_deck(&deck(1, None, "D")).unwrap();
        add_note(tx, &note(1, "a", "b", &[], 5), 1);
        add_note(tx, &note(2, "c", "d", &[], 6), 1);
    });
    let (c1, c2) = (CardId(nid(1).0), CardId(nid(2).0));
    let m0 = fx.read(|r| r.review_log_mark().unwrap());
    assert_eq!((m0.max_seq, m0.generation), (0, 0));
    fx.write(|tx| {
        tx.insert_review_log(&review(1, c1, 100, "normal")).unwrap();
        tx.insert_review_log(&review(2, c2, 200, "trial")).unwrap();
        tx.insert_review_log(&review(3, c2, 300, "imported"))
            .unwrap();
    });
    let m1 = fx.read(|r| r.review_log_mark().unwrap());
    assert_eq!(m1.generation, 0, "appends do not invalidate");
    assert!(m1.max_seq >= 3);
    // Trial rows never reach statistics; `after` and the card filter apply.
    let rows = fx.read(|r| r.stats_reviews(0, None).unwrap());
    let ids: Vec<_> = rows.iter().map(|(_, r)| r.id).collect();
    assert_eq!(ids, vec![rid(1), rid(3)]);
    assert!(rows[0].0 < rows[1].0);
    let after_first = fx.read(|r| r.stats_reviews(rows[0].0, None).unwrap());
    assert_eq!(after_first.len(), 1);
    let only_c1 = fx.read(|r| r.stats_reviews(0, Some(&[c1])).unwrap());
    assert_eq!(only_c1.len(), 1);
    assert_eq!(fx.read(|r| r.review_count_upto(m1.max_seq).unwrap()), 3);
    // Undo of an answer deletes its row: the cache must notice.
    fx.write(|tx| assert!(tx.delete_review_log(rid(1)).unwrap()));
    let m2 = fx.read(|r| r.review_log_mark().unwrap());
    assert_eq!(m2.generation, 1);
    fx.raw()
        .execute(
            "UPDATE review_log SET duration_ms = 1 WHERE id = ?1",
            [rid(3).to_string()],
        )
        .unwrap();
    assert_eq!(fx.read(|r| r.review_log_mark().unwrap()).generation, 2);
    // A card purged for good takes its reviews along (cascade) — also a change.
    fx.raw()
        .execute("DELETE FROM cards WHERE id = ?1", [c2.to_string()])
        .unwrap();
    assert!(fx.read(|r| r.review_log_mark().unwrap()).generation >= 3);
}

#[test]
fn cache_round_trip_and_due_times() {
    let fx = Fx::seeded();
    let rec = StatsCacheRecord {
        calendar: "180:4".into(),
        upto_seq: 7,
        row_count: 5,
        generation: 2,
        days: vec![(10, json!({"day": 10})), (12, json!({"day": 12}))],
        updated_at: Timestamp(99),
    };
    fx.write(|tx| tx.put_stats_cache(&rec).unwrap());
    assert_eq!(
        fx.read(|r| r.stats_cache("180:4").unwrap()),
        Some(rec.clone())
    );
    assert_eq!(fx.read(|r| r.stats_cache("0:4").unwrap()), None);
    let mut smaller = rec.clone();
    smaller.days.truncate(1);
    fx.write(|tx| tx.put_stats_cache(&smaller).unwrap());
    assert_eq!(fx.read(|r| r.stats_cache("180:4").unwrap()), Some(smaller));

    fx.write(|tx| {
        tx.insert_deck(&deck(1, None, "D")).unwrap();
        for (n, due, state, susp) in [
            (1, 1000, CardState::Review, false),
            (2, 2000, CardState::Review, true),
            (3, 3000, CardState::Learning, false),
            (4, 9000, CardState::Review, false),
        ] {
            add_note(tx, &note(n, "a", "b", &[], 5), 1);
            let mut c = tx.get_card(CardId(nid(n).0)).unwrap().unwrap();
            c.scheduling.state = state;
            c.scheduling.due = Some(Timestamp(due));
            c.suspended = susp;
            tx.update_card(&c).unwrap();
        }
    });
    let due = fx.read(|r| r.due_times(Timestamp(0), Timestamp(5000)).unwrap());
    assert_eq!(due, vec![Timestamp(1000), Timestamp(3000)]);
}

#[test]
fn exams_and_goals_crud() {
    let fx = Fx::seeded();
    fx.write(|tx| tx.insert_deck(&deck(1, None, "YKS")).unwrap());
    let exam = ExamRecord {
        id: ExamId(super::u(0x7001)),
        name: "YKS".into(),
        date: "2027-06-20".into(),
        deck_id: Some(did(1)),
        query: Some("attr_x:y".into()),
        created_at: Timestamp(1),
        updated_at: Timestamp(2),
    };
    let goal = GoalRecord {
        id: GoalId(super::u(0x7002)),
        kind: "cards_learned".into(),
        target: 500.0,
        deadline: Some(Timestamp(5_000)),
        query: None,
        created_at: Timestamp(3),
        updated_at: Timestamp(3),
    };
    fx.write(|tx| {
        tx.upsert_exam(&exam).unwrap();
        tx.upsert_goal(&goal).unwrap();
    });
    assert_eq!(fx.read(|r| r.list_exams().unwrap()), vec![exam.clone()]);
    assert_eq!(
        fx.read(|r| r.get_goal(goal.id).unwrap()),
        Some(goal.clone())
    );
    let mut g2 = goal.clone();
    g2.target = 600.0;
    fx.write(|tx| tx.upsert_goal(&g2).unwrap());
    assert_eq!(fx.read(|r| r.list_goals().unwrap()), vec![g2]);
    fx.write(|tx| {
        assert!(tx.delete_goal(goal.id).unwrap());
        assert!(!tx.delete_goal(goal.id).unwrap());
    });
    // An exam on a deck goes with the deck when it is purged for good.
    fx.raw()
        .execute("DELETE FROM decks WHERE id = ?1", [did(1).to_string()])
        .unwrap();
    assert!(fx.read(|r| r.get_exam(exam.id).unwrap()).is_none());
}
