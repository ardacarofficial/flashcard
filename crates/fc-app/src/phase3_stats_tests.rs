//! Phase-3 integration, round 2 (v0.6 statistics + v0.8 MCP + the Phase-2 history rules), end
//! to end on one SQLite file opened by two apps, as on a real machine: the desktop (`load_state`,
//! actor `user`) and `fc mcp serve` (`open_current`, actor `mcp:<client>`) next to a CLI
//! process (actor `cli`).
//!
//! - Notes Claude adds through MCP and the reviews answered in the other process reach the
//!   desktop's statistics: the desktop notices them through `data_changed` and its cached
//!   overview (cache + rows appended by the other process) equals a recomputation.
//! - MCP cannot answer or start the optimizer (no tool reaches them).
//! - An undo in the other process invalidates the cache through the shared generation.
//! - Emptying the trash writes a history barrier but keeps the review history: totals and the
//!   cache stay valid, undo stops at the barrier.
//! - A restore drops the cache (derived, not in the backup) and the next refresh rebuilds it.
//! - The optimizer job is cancellable and then leaves the preset alone.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::modules::mcp::tools::call_tool;
use crate::testing::TestApp;
use fc_api::v1::browse::PurgeTrash;
use fc_api::v1::common::Empty;
use fc_api::v1::events::DomainEvent;
use fc_api::v1::jobs::{JobRef, JobStatus};
use fc_api::v1::methods::*;
use fc_api::v1::notes::NoteIds;
use fc_api::v1::oplog::Actor;
use fc_api::v1::planning::RunOptimizer;
use fc_api::v1::presets::{AssignPreset, CreatePreset, PresetRef};
use fc_api::v1::stats::{
    RefreshStatsCache, StatsCacheInfo, StatsOverview, StatsRequest, StatsScope,
};
use fc_api::v1::study::{AnswerCard, SessionRef, StartSession, StudyKind, StudyMode};
use fc_api::{ApiError, ApiMethod, ErrorCode};
use fc_core::card::Rating;
use fc_core::NoteId;
use fc_storage::SqliteStore;
use serde_json::{json, Value};
use std::sync::Arc;

const CLIENT: &str = "claude-code";

fn tool(t: &TestApp, name: &str, args: Value) -> Value {
    match call_tool(&t.app, CLIENT, false, name, args) {
        Ok(v) => v,
        Err(e) => panic!("{name} failed: {e:?}"),
    }
}

fn as_actor<M: ApiMethod>(
    t: &TestApp,
    actor: &Actor,
    req: M::Request,
) -> Result<M::Response, ApiError> {
    t.app.call::<M>(&req, actor)
}

fn overview(t: &TestApp, fresh: bool) -> StatsOverview {
    t.ok::<StatsGetOverview>(StatsRequest {
        scope: StatsScope::default(),
        tz_offset_mins: Some(0),
        days: None,
        forecast_days: Some(14),
        fresh: Some(fresh),
    })
}

fn refresh(t: &TestApp) -> StatsCacheInfo {
    t.ok::<StatsRefreshCache>(RefreshStatsCache {
        tz_offset_mins: Some(0),
    })
}

/// What the desktop's statistics screen does on load: refresh the cache, read the overview.
/// The cached overview must equal a recomputation; returns it.
fn screen(t: &TestApp) -> StatsOverview {
    refresh(t);
    let cached = overview(t, false);
    let fresh = overview(t, true);
    assert_eq!(
        StatsOverview {
            from_cache: false,
            ..cached.clone()
        },
        fresh,
        "cache + appended rows = recomputation"
    );
    cached
}

/// `data_changed` announcements since the last call.
fn announced(t: &TestApp) -> usize {
    t.take_events()
        .iter()
        .filter(|e| matches!(e.event, DomainEvent::DataChanged))
        .count()
}

fn start() -> StartSession {
    StartSession {
        kind: StudyKind::Normal,
        mode: Some(StudyMode::Scheduled),
        deck_id: None,
        include_subdecks: None,
        filter: None,
        from_session: None,
        new_limit: Some(100),
        order: None,
        tz_offset_mins: Some(0),
    }
}

/// Answers every card of a normal session as `actor` (`Good`); returns how many answers.
fn study_as(t: &TestApp, actor: &Actor) -> usize {
    let mut s = as_actor::<StudyStart>(t, actor, start()).unwrap();
    let mut n = 0;
    while let Some(c) = s.current.clone() {
        n += 1;
        s = as_actor::<StudyAnswer>(
            t,
            actor,
            AnswerCard {
                session_id: s.id,
                card_id: c.card_id,
                rating: Rating::Good,
                duration_ms: 4_000,
                reveal_ms: None,
            },
        )
        .unwrap();
        assert!(n < 100);
    }
    as_actor::<StudyEnd>(t, actor, SessionRef { session_id: s.id }).unwrap();
    n
}

#[test]
fn mcp_and_cli_writes_reach_the_desktop_statistics() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("collection.db");
    let desktop = TestApp::new(Arc::new(SqliteStore::open(&path).unwrap()));
    assert!(!desktop.app.poll_external_changes().unwrap());
    desktop.take_events();
    let empty = screen(&desktop);
    assert_eq!(empty.reviews.totals.reviews, 0);
    assert_eq!(empty.cards.states.total, 0);

    // `fc mcp serve`: its own process and connection, attaches without migrating.
    let server = TestApp::new(Arc::new(SqliteStore::open_current(&path).unwrap()));
    let mcp = Actor::Mcp(CLIENT.into());
    tool(&server, "create_deck", json!({ "path": "Claude" }));
    let one = |front: &str| json!({ "fields": { "front": front, "back": "x" } });
    let added = tool(
        &server,
        "add_notes",
        json!({ "deck": "Claude", "notes": [one("a"), one("b"), one("c"), one("d")] }),
    );
    assert_eq!(added["added"], 4);

    // MCP has no tool that answers cards or starts jobs: the actor is refused outright.
    let refused = |e: ApiError| assert_eq!(e.code, ErrorCode::PermissionDenied, "{e:?}");
    refused(
        as_actor::<StudyStart>(&server, &mcp, start())
            .map(|_| ())
            .unwrap_err(),
    );

    // The desktop notices the foreign commit once; its screen shows Claude's new cards.
    assert!(desktop.app.poll_external_changes().unwrap());
    assert_eq!(announced(&desktop), 1);
    let s = screen(&desktop);
    assert_eq!(s.cards.states.new, 4);
    assert_eq!(s.reviews.totals.reviews, 0);

    // Another process answers them (`fc study`, actor cli): the desktop's cache folds the
    // rows that process appended, after one `data_changed`.
    let cli = Actor::Cli;
    let answered = study_as(&server, &cli);
    assert!(answered >= 4, "{answered}");
    assert!(desktop.app.poll_external_changes().unwrap());
    assert_eq!(announced(&desktop), 1);
    let s = screen(&desktop);
    assert_eq!(s.reviews.totals.reviews as usize, answered);
    assert_eq!(s.reviews.totals.new_cards, 4);
    assert!(s.from_cache);
    assert_eq!(s.cards.states.new, 0);

    // That process undoes its last answer: the row disappears, the shared generation moves,
    // the desktop's cache is rebuilt instead of trusted.
    as_actor::<OplogUndo>(&server, &cli, Empty {}).unwrap();
    assert!(desktop.app.poll_external_changes().unwrap());
    assert_eq!(announced(&desktop), 1);
    assert!(refresh(&desktop).rebuilt);
    let after_undo = screen(&desktop);
    assert_eq!(after_undo.reviews.totals.reviews as usize, answered - 1);

    // Emptying the trash (desktop) writes a barrier and keeps the review history: the
    // statistics and the cache are unchanged by the purge (no rebuild), undo stops there.
    let a: NoteId = added["notes"][0]["note_id"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    desktop.ok::<NotesDelete>(NoteIds { ids: vec![a] });
    refresh(&desktop);
    let purged = desktop.ok::<BrowsePurgeTrash>(PurgeTrash {
        older_than_days: Some(0),
    });
    assert!(purged.notes >= 1, "{purged:?}");
    assert!(!refresh(&desktop).rebuilt, "a purge leaves the cache valid");
    let s = screen(&desktop);
    assert_eq!(s.reviews, after_undo.reviews);
    assert_eq!(s.cards.states.total, 3);
    let e = desktop.err::<OplogUndo>(Empty {});
    assert_eq!(e.code, ErrorCode::ConflictHistoryBarrier, "{e:?}");
    // Undo in the other process stops at the same barrier (the history is shared).
    let e = as_actor::<OplogUndo>(&server, &cli, Empty {}).unwrap_err();
    assert_eq!(e.code, ErrorCode::ConflictHistoryBarrier, "{e:?}");

    // A restore replaces the history: the cache is dropped and rebuilt to the same numbers.
    let dump = desktop.store.read().unwrap().dump_collection().unwrap();
    {
        let mut tx = desktop.store.write().unwrap();
        tx.load_collection(&dump).unwrap();
        tx.commit().unwrap();
    }
    let info = refresh(&desktop);
    assert!(info.rebuilt, "{info:?}");
    assert_eq!(screen(&desktop).reviews, after_undo.reviews);
}

#[test]
fn optimizer_job_is_cancellable_and_out_of_mcp_reach() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("collection.db");
    let desktop = TestApp::new(Arc::new(SqliteStore::open(&path).unwrap()));
    let server = TestApp::new(Arc::new(SqliteStore::open_current(&path).unwrap()));
    tool(&server, "create_deck", json!({ "path": "Opt" }));
    let deck = desktop
        .ok::<DecksList>(Empty {})
        .decks
        .into_iter()
        .find(|d| d.path == "Opt")
        .unwrap()
        .id;
    let preset = desktop.ok::<PresetsCreate>(CreatePreset {
        name: "Mine".into(),
        config: None,
    });
    desktop.ok::<PresetsAssign>(AssignPreset {
        deck_ids: vec![deck],
        preset_id: Some(preset.id),
    });
    let run = RunOptimizer {
        preset_id: preset.id,
        tz_offset_mins: Some(0),
    };

    // Claude cannot start it (it would rewrite the user's scheduler weights).
    let e = as_actor::<OptimizerRun>(&server, &Actor::Mcp(CLIENT.into()), run.clone()).unwrap_err();
    assert_eq!(e.code, ErrorCode::PermissionDenied, "{e:?}");

    // The user starts and cancels it: the job ends cancelled, no weights, no undo step.
    let before = desktop.ok::<OplogStatus>(Empty {});
    let started = desktop.ok::<OptimizerRun>(run);
    desktop.ok::<JobsCancel>(JobRef { id: started.job.id });
    desktop.app.run_pending_jobs().unwrap();
    let job = desktop.ok::<JobsGet>(JobRef { id: started.job.id });
    assert_eq!(job.status, JobStatus::Cancelled);
    assert!(desktop
        .ok::<PresetsGet>(PresetRef { id: preset.id })
        .config
        .fsrs_weights
        .is_empty());
    assert_eq!(desktop.ok::<OplogStatus>(Empty {}), before);
}
