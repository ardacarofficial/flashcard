//! v1.4 Z1: **every module can be switched off without breaking the others** (CLAUDE.md
//! rule 7). For each module the user can switch off: switch it off, restart the app on the
//! same SQLite file, and check that the app starts (`modules.list` says it is off), studying
//! works (start + answer, the answer read back from the review history) and a Subtitle
//! capture is stored (read back as a note) — the owner's two daily jobs.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::modules::subtitle;
use crate::testing::{TempSqlite, TestApp};
use fc_api::v1::oplog::Actor;
use fc_storage::Store;
use serde_json::{json, Value};
use std::sync::Arc;

fn call(t: &TestApp, method: &str, params: Value) -> Value {
    match t.app.invoke(&format!("v1.{method}"), params, &Actor::User) {
        Ok(v) => v,
        Err(e) => panic!("{method} failed: {e:?}"),
    }
}

fn set_module(t: &TestApp, id: &str, enabled: bool) {
    call(t, "modules.set_enabled", json!({ "id": id, "enabled": enabled }));
}

fn capture_json(n: u32, lemma: &str) -> Value {
    json!({
      "schema": 1,
      "capture_id": format!("0192812a-6f3c-7a41-9c2e-{n:012x}"),
      "kind": "word",
      "lang": { "target": "en", "native": "tr" },
      "term": { "surface": lemma, "lemma": lemma, "pos": "noun" },
      "definition": { "text": format!("anlam {n}"), "source": "dict" },
      "context": { "sentence": format!("I need my {lemma} now."), "term_offset": [10, 10 + lemma.len()] },
      "source": { "type": "video", "url": "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
                  "title": "Film", "site": "youtube", "video_id": "dQw4w9WgXcQ",
                  "t_start_ms": 1000, "t_end_ms": 2500 },
      "media": {},
      "deck_id": null,
      "tags": ["subtitle"],
      "created_at": "2026-10-12T10:00:00.000Z",
      "client": { "name": "subtitle-extension", "ext_version": "3.1.3" }
    })
}

/// Studies one new card of a fresh deck and reads the answer back.
fn study_works(t: &TestApp, module: &str) {
    let deck = call(t, "decks.create", json!({ "path": format!("Deste {module}") }))["id"].clone();
    let classic = fc_core::content::builtin::ALL
        .iter()
        .find(|b| b.key == "classic")
        .unwrap()
        .id;
    call(
        t,
        "notes.create",
        json!({ "notetype_id": classic, "deck_id": deck, "fields": { "front": "apple", "back": "elma" } }),
    );
    let s = call(t, "study.start", json!({ "kind": "normal", "deck_id": deck }));
    let card = s["current"]["card_id"].clone();
    assert!(card.is_string(), "{module} off: no card to study: {s}");
    call(
        t,
        "study.answer",
        json!({ "session_id": s["id"], "card_id": card, "rating": "good", "duration_ms": 3000 }),
    );
    let card_id: fc_core::CardId = serde_json::from_value(card).unwrap();
    let logs = t.store.read().unwrap().review_logs_for_cards(&[card_id]).unwrap();
    assert_eq!(logs.len(), 1, "{module} off: the answer was not stored");
}

/// Posts a capture and reads the note back.
fn capture_works(t: &TestApp, module: &str, n: u32) {
    let made = call(t, "subtitle.keys.create", json!({ "name": "Chrome" }));
    let key = subtitle::authenticate(
        &t.app,
        &subtitle::key_hash(made["secret"].as_str().unwrap()),
    )
    .unwrap();
    let o = subtitle::capture(&t.app, &key, &capture_json(n, "lantern"))
        .unwrap_or_else(|e| panic!("{module} off: capture failed: {e:?}"));
    let note = call(t, "notes.get", json!({ "id": o.note_id }));
    assert!(
        note["fields"].to_string().contains("lantern"),
        "{module} off: the capture's note lost its word: {note}"
    );
}

#[test]
fn every_module_can_be_switched_off_and_the_app_still_starts_studies_and_captures() {
    let probe = TestApp::mem();
    let modules: Vec<String> = probe
        .app
        .registry()
        .modules()
        .iter()
        .filter(|m| m.can_disable)
        .map(|m| m.id.to_owned())
        .collect();
    assert!(modules.len() >= 20, "{modules:?}");
    for (n, module) in modules.iter().enumerate() {
        let file: Arc<dyn Store> = Arc::new(TempSqlite::new());
        {
            let t = TestApp::new(file.clone());
            if module != subtitle::ID {
                set_module(&t, subtitle::ID, true);
            }
            set_module(&t, module, false);
        }
        // Restart on the same file.
        let t = TestApp::new(file.clone());
        let list = call(&t, "modules.list", json!({}));
        let row = list["modules"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["id"] == json!(module))
            .unwrap_or_else(|| panic!("{module} not in modules.list: {list}"));
        assert_eq!(row["enabled"], json!(false), "{module}: {row}");
        call(&t, "decks.tree", json!({}));
        call(&t, "settings.schema", json!({}));
        study_works(&t, module);
        if module != subtitle::ID {
            capture_works(&t, module, n as u32 + 1);
        }
    }
}
