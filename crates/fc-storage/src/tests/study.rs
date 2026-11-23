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
    // Daily-limit read: since a time, one mode, oldest first.
    let mut r3 = review(3, 300);
    r3.card_id = card_id;
    r3.mode = "trial".into();
    fx.write(|tx| tx.insert_review_log(&r3).unwrap());
    assert_eq!(
        fx.read(|r| r.review_logs_since(Timestamp(100), "normal").unwrap()),
        vec![r2.clone(), r1.clone()]
    );
    assert_eq!(
        fx.read(|r| r.review_logs_since(Timestamp(101), "normal").unwrap()),
        vec![r1.clone()]
    );
    assert_eq!(
        fx.read(|r| r.review_logs_since(Timestamp(0), "trial").unwrap()),
        vec![r3.clone()]
    );
    fx.write(|tx| assert!(tx.delete_review_log(rid(3)).unwrap()));
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

/// v1.0: a due-only session reads cards in progress due before the day ends (through the due
/// index) and only the first new cards in creation order.
#[test]
fn study_cards_due_before_and_first_new() {
    use crate::repo::NewCardsFirst;
    let fx = Fx::seeded();
    let card_of = |n: u128| fc_core::CardId(nid(n).0);
    fx.write(|tx| {
        tx.insert_deck(&deck(1, None, "Eng")).unwrap();
        for n in 1..=6 {
            add_note(tx, &note(n, &format!("w{n}"), "x", &[], 10 + n as i64), 1);
        }
        // 1: review due at 500; 2: review due at 2 000; 3: learning without a due time.
        for (n, state, due) in [
            (1, CardState::Review, Some(500)),
            (2, CardState::Review, Some(2_000)),
            (3, CardState::Learning, None),
        ] {
            let mut c = tx.get_card(card_of(n)).unwrap().unwrap();
            c.scheduling.state = state;
            c.scheduling.due = due.map(Timestamp);
            tx.update_card(&c).unwrap();
        }
    });
    let ids = |f: StudyCardFilter| -> Vec<u128> {
        fx.read(|r| r.study_cards(&f).unwrap())
            .into_iter()
            .map(|c| c.id.0.as_u128() - super::u(0x2000).as_u128())
            .collect()
    };
    assert_eq!(ids(StudyCardFilter::default()), vec![1, 2, 3, 4, 5, 6]);
    let due = |new_first| StudyCardFilter {
        due_before: Some(Timestamp(1_000)),
        new_first,
        ..Default::default()
    };
    assert_eq!(ids(due(None)), vec![1, 3, 4, 5, 6]);
    assert_eq!(
        ids(due(Some(NewCardsFirst {
            n: 2,
            newest: false
        }))),
        vec![1, 3, 4, 5]
    );
    assert_eq!(
        ids(due(Some(NewCardsFirst { n: 2, newest: true }))),
        vec![1, 3, 5, 6]
    );
    assert_eq!(
        ids(StudyCardFilter {
            new_first: Some(NewCardsFirst {
                n: 0,
                newest: false
            }),
            ..Default::default()
        }),
        vec![1, 2, 3]
    );
}

/// v1.0: a session's queue rows: inserted, deleted by seq, read in seq order, cleared.
#[test]
fn session_queue_rows() {
    use crate::repo::SessionQueueRow;
    let fx = Fx::new();
    let row = |seq: i64, n: u128| SessionQueueRow {
        seq,
        card_id: cid(n),
        note_id: nid(n),
        kind: "review".into(),
        due: (n == 2).then_some(Timestamp(77)),
    };
    fx.write(|tx| {
        tx.upsert_session(&session(1, 10, None)).unwrap();
        tx.upsert_session(&session(2, 10, None)).unwrap();
        tx.edit_session_queue(sid(1), false, &[], &[row(30, 3), row(10, 1), row(20, 2)])
            .unwrap();
        tx.edit_session_queue(sid(2), false, &[], &[row(10, 9)])
            .unwrap();
    });
    let seqs = |s: u128| -> Vec<i64> {
        fx.read(|r| r.session_queue(sid(s)).unwrap())
            .iter()
            .map(|r| r.seq)
            .collect()
    };
    assert_eq!(seqs(1), vec![10, 20, 30]);
    assert_eq!(fx.read(|r| r.session_queue(sid(1)).unwrap())[1], row(20, 2));
    fx.write(|tx| {
        tx.edit_session_queue(sid(1), false, &[10], &[row(40, 1)])
            .unwrap();
    });
    assert_eq!(seqs(1), vec![20, 30, 40]);
    // A seq is taken once.
    fx.write(|tx| {
        assert!(tx
            .edit_session_queue(sid(1), false, &[], &[row(20, 5)])
            .is_err())
    });
    fx.write(|tx| {
        tx.edit_session_queue(sid(1), true, &[], &[row(5, 4)])
            .unwrap()
    });
    assert_eq!(seqs(1), vec![5]);
    assert_eq!(seqs(2), vec![10], "other sessions untouched");
}

/// v1.4 Z2: the per-deck first-new read (`cards_deck_new_order`) and the skipped branches
/// give what the reference gives — every candidate read whole, then the first `n` new cards
/// taken in creation order — on random worlds (decks, states, suspended, trashed, ties).
#[test]
fn study_cards_per_deck_first_new_equals_reference() {
    use crate::repo::NewCardsFirst;
    use fc_core::rng::{Rng, SplitMix64};
    for seed in 0..6u64 {
        let fx = Fx::seeded();
        let mut rng = SplitMix64::new(seed);
        let decks = 5u128;
        fx.write(|tx| {
            for d in 1..=decks {
                tx.insert_deck(&deck(d, None, &format!("D{d}"))).unwrap();
            }
            for n in 1..=120u128 {
                let d = 1 + u128::from(rng.next_u64() % 5);
                // Ties on created_at on purpose (ordered by id then).
                let at = 10 + (rng.next_u64() % 40) as i64;
                add_note(tx, &note(n, &format!("w{n}"), "x", &[], at), d);
                let mut c = tx.get_card(fc_core::CardId(nid(n).0)).unwrap().unwrap();
                match rng.next_u64() % 4 {
                    0 => {
                        c.scheduling.state = CardState::Review;
                        c.scheduling.due = Some(Timestamp((rng.next_u64() % 2_000) as i64));
                    }
                    1 => {
                        c.scheduling.state = CardState::Learning;
                        c.scheduling.due = (rng.next_u64() % 2 == 0)
                            .then(|| Timestamp((rng.next_u64() % 2_000) as i64));
                    }
                    _ => {}
                }
                c.suspended = rng.next_u64() % 9 == 0;
                tx.update_card(&c).unwrap();
                if rng.next_u64() % 15 == 0 {
                    tx.set_notes_deleted(&[nid(n)], Some(Timestamp(5))).unwrap();
                }
            }
        });
        let started = [
            CardState::Learning,
            CardState::Review,
            CardState::Relearning,
        ];
        for (decks_sel, states, n, newest) in [
            (vec![1u128], None, 3u32, false),
            (vec![2, 3], None, 5, true),
            (vec![1, 2, 3, 4, 5], None, 7, false),
            (vec![4, 5], Some(vec![CardState::New]), 4, false),
            (vec![3], Some(started.to_vec()), 4, true),
            (
                vec![1, 5],
                Some(vec![CardState::New, CardState::Review]),
                0,
                false,
            ),
        ] {
            let f = StudyCardFilter {
                deck_ids: Some(decks_sel.iter().map(|d| did(*d)).collect()),
                states: states.clone(),
                due_before: Some(Timestamp(1_000)),
                new_first: Some(NewCardsFirst { n, newest }),
                ..Default::default()
            };
            let got: Vec<_> = fx.read(|r| r.study_cards(&f).unwrap());
            // Reference: everything, then the first/last `n` new cards.
            let all: Vec<_> = fx.read(|r| {
                r.study_cards(&StudyCardFilter {
                    new_first: None,
                    ..f.clone()
                })
                .unwrap()
            });
            let mut new: Vec<_> = all
                .iter()
                .filter(|c| c.scheduling.state == CardState::New)
                .collect();
            if newest {
                new.reverse();
            }
            let keep: std::collections::HashSet<_> =
                new.iter().take(n as usize).map(|c| c.id).collect();
            let want: Vec<_> = all
                .iter()
                .filter(|c| c.scheduling.state != CardState::New || keep.contains(&c.id))
                .map(|c| c.id)
                .collect();
            let got: Vec<_> = got.iter().map(|c| c.id).collect();
            assert_eq!(
                got, want,
                "seed {seed}, decks {decks_sel:?}, states {states:?}"
            );
        }
        let (totals, counts) = fx.read(|r| {
            (
                r.card_totals_by_deck().unwrap(),
                r.card_counts_by_deck(Timestamp(0)).unwrap(),
            )
        });
        let counts: Vec<_> = counts.into_iter().map(|(d, c)| (d, c.total)).collect();
        assert_eq!(totals, counts, "seed {seed}");
    }
}

/// Migration 1422 adds the two indexes to an existing collection; its cards are untouched.
#[test]
fn migration_1422_adds_speed_indexes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("collection.db");
    let before: Vec<_> = MIGRATIONS
        .iter()
        .filter(|m| m.version < 1422)
        .copied()
        .collect();
    drop(SqliteStore::open_with_migrations(&path, &before).unwrap());
    let index_count = || -> i64 {
        Connection::open(&path)
            .unwrap()
            .query_row(
                "SELECT count(*) FROM sqlite_master
                 WHERE name IN ('cards_deck_new_order', 'cards_deck_live')",
                [],
                |r| r.get(0),
            )
            .unwrap()
    };
    assert_eq!(index_count(), 0);
    let store = SqliteStore::open(&path).unwrap();
    assert_eq!(index_count(), 2);
    let r = store.read().unwrap();
    assert!(r.card_totals_by_deck().unwrap().is_empty());
}
