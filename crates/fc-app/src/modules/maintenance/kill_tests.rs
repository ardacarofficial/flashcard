//! v1.0 integration (round 2): crash recovery × the incremental session queue. A real
//! process studies on a SQLite file and is killed mid-session (`abort`: no destructor, no
//! clean run end, no in-memory queue cache survives); the next start finds the unfinished
//! run marker, offers the session and resumes it with exactly the queue the killed process
//! had saved last (`session_queue` rows written per answer, docs/10 §36).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::run::RunGuard;
use crate::App;
use fc_api::v1::oplog::Actor;
use fc_core::notetype::builtin::CLASSIC_ID;
use fc_core::{SessionId, Timestamp};
use serde_json::{json, Value};
use std::path::Path;

/// Folder the [`killed_child`] process studies in.
const KILL_DIR: &str = "FC_KILL_TEST_DIR";
/// What the child saw just before it was killed.
const EXPECTED: &str = "expected.json";

fn call(app: &App, method: &str, params: Value) -> Value {
    app.invoke(&format!("v1.{method}"), params, &Actor::User)
        .unwrap_or_else(|e| panic!("{method}: {e:?}"))
}

/// The stored queue of `session`: (card, kind, due) in order.
fn stored_queue(app: &App, session: &str) -> Vec<(String, String, Option<i64>)> {
    let id: SessionId = serde_json::from_value(json!(session)).unwrap();
    app.store()
        .read()
        .unwrap()
        .session_queue(id)
        .unwrap()
        .into_iter()
        .map(|r| (r.card_id.to_string(), r.kind, r.due.map(|t| t.0)))
        .collect()
}

/// The process that gets killed: does nothing unless started by the test below.
#[test]
#[ignore = "crash-recovery child: started by `a_killed_process_resumes_its_exact_queue`"]
fn killed_child() {
    let Ok(dir) = std::env::var(KILL_DIR) else {
        return;
    };
    let dir = Path::new(&dir);
    let (_run, start) = RunGuard::begin(dir, "1.0.0", Timestamp(1_000)).unwrap();
    assert!(!start.previous_unclean());
    let app = App::open_sqlite(&dir.join("collection.db")).unwrap();
    let deck = call(&app, "decks.create", json!({"path": "Kurtarma"}));
    for i in 0..30 {
        call(
            &app,
            "notes.create",
            json!({"notetype_id": CLASSIC_ID, "deck_id": deck["id"],
                   "fields": {"front": format!("ön {i}"), "back": format!("arka {i}")},
                   "tags": [], "attributes": []}),
        );
    }
    let mut s = call(
        &app,
        "study.start",
        json!({"kind": "normal", "deck_id": deck["id"], "tz_offset_mins": 0}),
    );
    // Mixed answers: "again" re-queues learning steps, "good"/"easy" move cards on.
    for rating in ["good", "again", "easy", "good", "again", "hard", "good"] {
        let card = s["current"]["card_id"].clone();
        s = call(
            &app,
            "study.answer",
            json!({"session_id": s["id"], "card_id": card, "rating": rating, "duration_ms": 900}),
        );
    }
    let id = s["id"].as_str().unwrap().to_owned();
    let expected = json!({
        "session": s,
        "queue": stored_queue(&app, &id),
    });
    std::fs::write(dir.join(EXPECTED), expected.to_string()).unwrap();
    // Killed: no `finish` of the run marker, no destructors, nothing flushed by us.
    std::process::abort();
}

#[test]
fn a_killed_process_resumes_its_exact_queue() {
    let dir = tempfile::tempdir().unwrap();
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "modules::maintenance::kill_tests::killed_child",
            "--exact",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(KILL_DIR, dir.path())
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "the child must die mid-session: {}\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let expected: Value = serde_json::from_str(
        &std::fs::read_to_string(dir.path().join(EXPECTED)).unwrap_or_else(|e| {
            panic!(
                "child wrote nothing ({e}): {}\n{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            )
        }),
    )
    .unwrap();
    let session = &expected["session"];
    let id = session["id"].as_str().unwrap();
    assert!(
        expected["queue"].as_array().unwrap().len() > 1,
        "the killed session still had cards queued"
    );

    // The next start: the run marker says the last run did not end.
    let (_run, start) = RunGuard::begin(dir.path(), "1.0.0", Timestamp(5_000)).unwrap();
    assert!(start.previous_unclean());
    let app = App::open_sqlite(&dir.path().join("collection.db")).unwrap();
    app.start_maintenance(dir.path(), None, start);
    let offer = call(&app, "maintenance.recovery.get", json!({}));
    assert_ne!(offer["reason"], Value::Null, "{offer}");
    assert_eq!(offer["session"]["id"], session["id"], "the open session is offered");
    assert_eq!(offer["integrity"]["ok"], true);

    // Exactly the queue the killed process saved last (read from rows: no cache survived).
    assert_eq!(json!(stored_queue(&app, id)), expected["queue"]);
    let resumed = call(&app, "study.session", json!({"session_id": id}));
    for field in ["current", "counts", "answered", "correct", "mistakes"] {
        assert_eq!(resumed[field], session[field], "{field}");
    }

    // And it goes on from there: answering the card on screen takes the queue's next one.
    let current = resumed["current"]["card_id"].clone();
    let next = call(
        &app,
        "study.answer",
        json!({"session_id": id, "card_id": current, "rating": "good", "duration_ms": 900}),
    );
    assert_eq!(next["answered"], session["answered"].as_u64().unwrap() + 1);
    // The rows written by the resumed process agree with what a fresh start reads.
    let after = stored_queue(&app, id);
    drop(app);
    let again = App::open_sqlite(&dir.path().join("collection.db")).unwrap();
    assert_eq!(stored_queue(&again, id), after);
    let reread = call(&again, "study.session", json!({"session_id": id}));
    assert_eq!(reread["current"], next["current"]);
}
