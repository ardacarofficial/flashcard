//! Phase-4 integration, rounds 1 and 2 (v0.10 modes + v0.10 reading next to v0.7 plugins, v0.8
//! MCP/AI and the Phase-2 history rules; docs/10 §29).
//!
//! * Reading items, the reading queue, word marks and inline cards are ordinary undo steps:
//!   every forward command and stored inverse is replayable, the job starters
//!   (`reading.items.import`, `reading.clips.extract`) and the timer start are not; undo stops
//!   at the trash-purge barrier (which also purges trashed reading items).
//! * MCP reaches no reading or modes method (no tool calls them); what every actor reaches is
//!   one matrix in `phase4_permits_tests` (round 2).
//!
//! Round 2 (v0.9 automation + local API/inbox; docs/10 §33): inbox proposals, their acceptance
//! by a rule, rejections and rule management are ordinary, replayable undo steps behind the
//! purge barrier (`automation_and_inbox_steps_*`).

use crate::testing::{MemStore, TempSqlite, TestApp};
use fc_api::v1::common::Empty;
use fc_api::v1::methods::*;
use fc_api::v1::oplog::{Actor, ListOplog};
use fc_api::{ApiMethod, ErrorCode, Visibility};
use fc_storage::Store;
use serde_json::{json, Value};
use std::sync::Arc;

fn call(t: &TestApp, method: &str, params: Value) -> Value {
    t.app
        .invoke(method, params, &Actor::User)
        .unwrap_or_else(|e| panic!("{method}: {e:?}"))
}

fn replayable(t: &TestApp, qualified: &str) -> bool {
    let name = qualified.strip_prefix("v1.").unwrap_or(qualified);
    t.app.registry().descriptor(name).unwrap().replayable
}

fn reading_and_modes_steps_are_replayable_behind_barriers(t: &TestApp) {
    let logged_before = t.ok::<OplogList>(ListOplog::default()).items.len();
    let deck = call(t, "v1.decks.create", json!({ "path": "Reading" }));
    let body = "The quick brown fox jumps over the lazy dog. It runs away.";
    let item = call(
        t,
        "v1.reading.items.create",
        json!({ "title": "Fox", "body": body, "language": "en" }),
    );
    let id = item["id"].clone();
    call(
        t,
        "v1.reading.queue.set",
        json!({ "id": id, "queued": true }),
    );
    call(
        t,
        "v1.reading.queue.read",
        json!({ "id": id, "position": 10 }),
    );
    call(
        t,
        "v1.reading.extracts.create",
        json!({ "item_id": id, "start": 0, "end": 43 }),
    );
    call(
        t,
        "v1.reading.words.mark",
        json!({ "language": "en", "lemmas": ["fox", "dog"], "mark": "known" }),
    );
    call(
        t,
        "v1.modes.inline.create",
        json!({ "text": "fox :: tilki\ndog :: köpek", "deck_id": deck["id"] }),
    );

    // Every forward command (redo) and stored inverse (undo) in the history is replayable.
    let log = t.ok::<OplogList>(ListOplog::default());
    let new = &log.items[..log.items.len() - logged_before];
    assert!(new.iter().any(|e| e.command.starts_with("v1.reading.")));
    assert!(
        new.iter().any(|e| e.command == "v1.notes.create"),
        "inline cards are nested notes.create"
    );
    for e in new {
        assert!(replayable(t, &e.command), "forward {}", e.command);
        for inv in &e.inverse {
            assert!(
                replayable(t, &inv.method),
                "inverse {} of {}",
                inv.method,
                e.command
            );
        }
    }
    // Job starters and session bookkeeping never run as undo/redo steps.
    for name in [
        ReadingItemsImport::NAME,
        ReadingClipsExtract::NAME,
        ModesTimedStart::NAME,
    ] {
        assert!(!replayable(t, name), "{name}");
    }

    // Undo/redo walk them exactly.
    let snap = crate::testing::suite::snapshot(t);
    for _ in 0..6 {
        t.ok::<OplogUndo>(Empty {});
    }
    for _ in 0..6 {
        t.ok::<OplogRedo>(Empty {});
    }
    assert_eq!(crate::testing::suite::snapshot(t), snap);

    // Trash the item and empty the trash: nothing else is in the trash, so the barrier below
    // is the reading item's alone (the purge counts it).
    call(t, "v1.reading.items.delete", json!({ "id": id }));
    let purged = call(t, "v1.browse.purge_trash", json!({ "older_than_days": 0 }));
    assert_eq!(
        (purged["notes"].clone(), purged["decks"].clone()),
        (json!(0), json!(0)),
        "{purged}"
    );
    let after = call(
        t,
        "v1.reading.items.create",
        json!({ "title": "After", "body": "Later text." }),
    );
    t.ok::<OplogUndo>(Empty {});
    let listed = call(t, "v1.reading.items.list", json!({}));
    assert!(
        !listed.to_string().contains(after["id"].as_str().unwrap()),
        "{listed}"
    );
    let e = t.err::<OplogUndo>(Empty {});
    assert_eq!(e.code, ErrorCode::ConflictHistoryBarrier, "{e:?}");
    assert_eq!(e.details["reason"], "purge");
    let e = t
        .app
        .invoke("v1.reading.items.get", json!({ "id": id }), &Actor::User)
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::NotFoundReadingItem);
}

#[test]
fn reading_and_modes_steps_mem() {
    reading_and_modes_steps_are_replayable_behind_barriers(&TestApp::new(
        Arc::new(MemStore::new()) as Arc<dyn Store>,
    ));
}

#[test]
fn reading_and_modes_steps_sqlite() {
    reading_and_modes_steps_are_replayable_behind_barriers(&TestApp::new(
        Arc::new(TempSqlite::new()) as Arc<dyn Store>,
    ));
}

/// No MCP tool calls a reading or modes method, so MCP reaches none (reads included).
#[test]
fn mcp_reaches_no_reading_or_modes_method() {
    let t = TestApp::mem();
    let mcp = Actor::Mcp("claude".into());
    let mut checked = 0;
    for d in catalog() {
        if d.visibility != Visibility::Public
            || !(d.module == "reading" || d.name.starts_with("modes."))
        {
            continue;
        }
        let e = t
            .app
            .invoke(&d.qualified_name(), json!({}), &mcp)
            .unwrap_err();
        assert_eq!(e.code, ErrorCode::PermissionDenied, "{}", d.name);
        checked += 1;
    }
    assert!(checked >= 25, "{checked}");
    assert!(!crate::modules::mcp::MCP_PERMISSIONS
        .iter()
        .any(|p| p.starts_with("reading")));
}

// ------------------------------------------------------------------ round 2: v0.9

/// Inbox items and automation rules as the user sees them (status and notes of every item,
/// every rule with its switch).
fn inbox_and_rules(t: &TestApp) -> Value {
    let items =
        |status: &str| call(t, "v1.inbox.list", json!({ "status": status }))["items"].clone();
    json!({
        "pending": items("pending"),
        "accepted": items("accepted"),
        "rejected": items("rejected"),
        "rules": call(t, "v1.automation.rules.list", json!({}))["rules"],
        "snapshot": crate::testing::suite::snapshot(t),
    })
}

/// Phase 4 round 2 (docs/10 §33): inbox proposals, their acceptance by a rule, rejections and
/// rule management are ordinary undo steps — every forward command and stored inverse is
/// replayable, undo/redo walk them exactly (redo runs no rule), and undo stops at the
/// trash-purge barrier like everywhere else.
fn automation_and_inbox_steps_are_replayable_behind_barriers(t: &TestApp) {
    let logged_before = t.ok::<OplogList>(ListOplog::default()).items.len();
    let deck = call(t, "v1.decks.create", json!({ "path": "Inbox" }));
    let accept = call(
        t,
        "v1.automation.rules.create",
        json!({
            "name": "Accept", "trigger": { "kind": "inbox_added" }, "condition": "",
            "actions": [{ "method": "inbox.accept", "params": { "deck_id": deck["id"] } }],
        }),
    );
    let tagger = call(
        t,
        "v1.automation.rules.create",
        json!({
            "name": "Tag", "trigger": { "kind": "note_added" }, "condition": "",
            "actions": [{ "method": "bulk.add_tags", "params": { "tags": ["from-inbox"] } }],
        }),
    );
    let start = inbox_and_rules(t);
    // Proposal → accepted by the rule → tagged by the next one: one undo step.
    call(
        t,
        "v1.inbox.add",
        json!({ "front": "badger", "back": "porsuk" }),
    );
    let accepted = call(t, "v1.inbox.list", json!({ "status": "accepted" }));
    let note = accepted["items"][0]["note_ids"][0].clone();
    assert!(note.is_string(), "{accepted}");
    let got = call(t, "v1.notes.get", json!({ "id": note }));
    assert_eq!(got["tags"], json!(["from-inbox"]));
    call(
        t,
        "v1.automation.rules.update",
        json!({ "id": accept["id"], "enabled": false }),
    );
    let otter = call(t, "v1.inbox.add", json!({ "front": "otter" }));
    call(t, "v1.inbox.reject", json!({ "ids": [otter["id"]] }));
    call(
        t,
        "v1.automation.rules.delete",
        json!({ "id": tagger["id"] }),
    );

    let log = t.ok::<OplogList>(ListOplog::default());
    let new = &log.items[..log.items.len() - logged_before];
    for cmd in [
        "v1.inbox.add",
        "v1.inbox.accept",
        "v1.inbox.reject",
        "v1.bulk.add_tags",
        "v1.automation.rules.create",
        "v1.automation.rules.update",
        "v1.automation.rules.delete",
    ] {
        assert!(new.iter().any(|e| e.command == cmd), "{cmd} logged");
    }
    assert!(new
        .iter()
        .any(|e| e.command == "v1.inbox.accept" && matches!(e.actor, Actor::Automation(_))));
    for e in new {
        assert!(replayable(t, &e.command), "forward {}", e.command);
        for inv in &e.inverse {
            assert!(
                replayable(t, &inv.method),
                "inverse {} of {}",
                inv.method,
                e.command
            );
        }
    }
    // A rule run and the local API's bookkeeping are never replayed.
    for name in [
        AutomationApply::NAME,
        LocalApiPair::NAME,
        LocalApiTouch::NAME,
    ] {
        assert!(!replayable(t, name), "{name}");
    }

    // Undo/redo walk the five steps exactly (the proposal and what its rules did are one).
    let end = inbox_and_rules(t);
    for _ in 0..5 {
        t.ok::<OplogUndo>(Empty {});
    }
    assert_eq!(inbox_and_rules(t), start);
    for _ in 0..5 {
        t.ok::<OplogRedo>(Empty {});
    }
    assert_eq!(inbox_and_rules(t), end, "redo replays, runs no rule");

    // Purging the trash is a barrier: the accepted note goes, a later proposal undoes, then
    // undo stops.
    call(t, "v1.notes.delete", json!({ "ids": [note] }));
    call(t, "v1.browse.purge_trash", json!({ "older_than_days": 0 }));
    call(t, "v1.inbox.add", json!({ "front": "later" }));
    t.ok::<OplogUndo>(Empty {});
    let e = t.err::<OplogUndo>(Empty {});
    assert_eq!(e.code, ErrorCode::ConflictHistoryBarrier, "{e:?}");
    assert_eq!(e.details["reason"], "purge");
}

#[test]
fn automation_and_inbox_steps_mem() {
    automation_and_inbox_steps_are_replayable_behind_barriers(&TestApp::new(
        Arc::new(MemStore::new()) as Arc<dyn Store>,
    ));
}

#[test]
fn automation_and_inbox_steps_sqlite() {
    automation_and_inbox_steps_are_replayable_behind_barriers(&TestApp::new(Arc::new(
        TempSqlite::new(),
    )
        as Arc<dyn Store>));
}
