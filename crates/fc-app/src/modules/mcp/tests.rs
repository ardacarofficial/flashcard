//! Service tests of the `mcp` module, each run against the in-memory store and SQLite (in
//! memory and on a temp file).
//!
//! v0.8 (MCP) acceptance, service side: `tools_list_decks_and_add_classified_notes` (a client
//! lists decks, adds notes with `Seviye = B1` into an inherited-classification deck, finds them
//! with `seviye>=B1`; the writes are one undo step with actor `mcp`). The stdio transport is
//! proven end to end in `fc-mcp` and `fc-cli/tests/mcp_stdio.rs`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::tools::{call_tool, catalog, list_tools};
use super::MCP_PERMISSIONS;
use crate::testing::{MemStore, TempSqlite, TestApp};
use fc_api::v1::attributes::*;
use fc_api::v1::common::Empty;
use fc_api::v1::decks::*;
use fc_api::v1::mcp::*;
use fc_api::v1::methods::*;
use fc_api::v1::modules::SetModuleEnabled;
use fc_api::v1::notes::NoteRef;
use fc_api::v1::oplog::{Actor, ListOplog, OplogEntryDto};
use fc_api::v1::settings::SetSetting;
use fc_api::{ApiMethod, ErrorCode, Visibility};
use fc_core::attribute::AttributeKind;
use fc_core::paging::PageRequest;
use fc_core::settings::SettingScope;
use fc_core::DeckId;
use fc_storage::{SqliteStore, Store};
use serde_json::{json, Value};
use std::sync::Arc;

type Factory = fn() -> Arc<dyn Store>;

macro_rules! suite {
    ($($name:ident),* $(,)?) => {
        mod mem {
            $( #[test] fn $name() { super::$name(|| std::sync::Arc::new(super::MemStore::new())); } )*
        }
        mod sqlite {
            $( #[test] fn $name() {
                super::$name(|| std::sync::Arc::new(super::SqliteStore::open_in_memory().unwrap()));
            } )*
        }
        mod sqlite_file {
            $( #[test] fn $name() { super::$name(|| std::sync::Arc::new(super::TempSqlite::new())); } )*
        }
    };
}

suite!(
    servers_crud_validate_and_undo,
    tools_list_decks_and_add_classified_notes,
    add_notes_is_all_or_nothing_with_positioned_errors,
    update_note_merges_and_undoes,
    set_classification_previews_until_confirmed,
    writes_switch_read_only_flag_and_module_switch,
    mcp_actor_holds_only_curated_permissions,
    only_user_and_cli_may_register_servers,
    registrations_are_approved_by_the_user,
    set_classification_refuses_selections_over_the_limit,
    foreign_commits_are_noticed_and_announced,
    mcp_writes_are_ordinary_undo_steps_behind_barriers,
);

// ------------------------------------------------------------------ helpers

const CLIENT: &str = "claude-code";

fn tool(t: &TestApp, name: &str, args: Value) -> Value {
    match call_tool(&t.app, CLIENT, false, name, args) {
        Ok(v) => v,
        Err(e) => panic!("{name} failed: {e:?}"),
    }
}

fn tool_err(t: &TestApp, name: &str, args: Value) -> fc_api::ApiError {
    match call_tool(&t.app, CLIENT, false, name, args) {
        Ok(v) => panic!("{name} unexpectedly succeeded: {v}"),
        Err(e) => e,
    }
}

fn deck(t: &TestApp, path: &str) -> DeckId {
    t.ok::<DecksCreate>(CreateDeck {
        path: path.into(),
        parent_id: None,
        description: None,
        icon: None,
        color: None,
    })
    .id
}

/// `İngilizce` with "Seviye" A1–C2 (ordered) and its sub-deck `İngilizce::Fiiller`.
fn english(t: &TestApp) -> (DeckId, DeckId) {
    let parent = deck(t, "İngilizce");
    let child = deck(t, "İngilizce::Fiiller");
    t.ok::<AttributesDefine>(DefineAttribute {
        scope_deck_id: Some(parent),
        key: Some("seviye".into()),
        name: "Seviye".into(),
        kind: AttributeKind::OrderedEnum,
        target: None,
        required: None,
        options: ["A1", "A2", "B1", "B2", "C1", "C2"]
            .iter()
            .map(|l| NewAttributeOption {
                label: (*l).into(),
                color: None,
            })
            .collect(),
    });
    (parent, child)
}

fn newest(t: &TestApp, n: u32) -> Vec<OplogEntryDto> {
    t.ok::<OplogList>(ListOplog {
        page: PageRequest {
            cursor: None,
            limit: Some(n),
        },
    })
    .items
}

fn total(t: &TestApp, query: &str) -> u64 {
    tool(t, "search_cards", json!({ "query": query }))["total"]
        .as_u64()
        .unwrap()
}

fn undo(t: &TestApp) {
    t.ok::<OplogUndo>(Empty {});
}

fn redo(t: &TestApp) {
    t.ok::<OplogRedo>(Empty {});
}

// ------------------------------------------------------------------ tests

pub(super) fn servers_crud_validate_and_undo(f: Factory) {
    let t = TestApp::new(f());
    let add = |name: &str| AddMcpServer {
        name: name.into(),
        transport: McpTransport::Stdio,
        command: Some(" npx ".into()),
        args: vec!["-y".into(), "@modelcontextprotocol/server-filesystem".into()],
        url: None,
        enabled: None,
    };
    let files = t.ok::<McpServersAdd>(add("Files"));
    assert_eq!(files.command, "npx");
    assert!(files.enabled && files.url.is_none());

    // Validation and name uniqueness (case/accent-insensitive).
    let e = t.err::<McpServersAdd>(add("FILES"));
    assert_eq!(e.code, ErrorCode::ConflictMcpServerName);
    let mut no_cmd = add("x");
    no_cmd.command = None;
    let e = t.err::<McpServersAdd>(no_cmd);
    assert_eq!(e.code, ErrorCode::ValidationMcpServer);
    assert_eq!(e.details["field"], "command");
    let e = t.err::<McpServersAdd>(AddMcpServer {
        name: "web".into(),
        transport: McpTransport::Http,
        command: None,
        args: vec![],
        url: Some("ftp://x".into()),
        enabled: None,
    });
    assert_eq!(e.details["field"], "url");
    let notion = t.ok::<McpServersAdd>(AddMcpServer {
        name: "Notion".into(),
        transport: McpTransport::Http,
        command: Some("ignored".into()),
        args: vec!["ignored".into()],
        url: Some("https://mcp.example.com/mcp".into()),
        enabled: Some(false),
    });
    assert!(notion.command.is_empty() && notion.args.is_empty());
    let list = || t.ok::<McpServersList>(Empty {}).servers;
    assert_eq!(list(), vec![files.clone(), notion.clone()]);

    // Update ↔ undo/redo.
    let edited = t.ok::<McpServersUpdate>(UpdateMcpServer {
        id: files.id,
        name: "Files (home)".into(),
        transport: McpTransport::Stdio,
        command: "npx".into(),
        args: vec![],
        url: None,
        enabled: false,
    });
    undo(&t);
    let back = list();
    assert_eq!(back[0].name, "Files");
    assert_eq!(back[0].args, files.args);
    assert!(back[0].enabled);
    redo(&t);
    assert_eq!(list()[0].name, edited.name);

    // Remove ↔ undo restores the exact row (id, created_at).
    let before = list();
    t.ok::<McpServersRemove>(McpServerRef { id: notion.id });
    assert_eq!(list().len(), 1);
    undo(&t);
    assert_eq!(list(), before);
    let e = t.err::<McpServersRemove>(McpServerRef {
        id: fc_core::McpServerId(uuid::Uuid::nil()),
    });
    assert_eq!(e.code, ErrorCode::NotFoundMcpServer);

    // The internal inverse is unreachable from outside, the oplog summary names the server.
    let internal = crate::tests::internal_methods_of(&t, &["mcp"]);
    assert_eq!(internal, ["mcp.servers.restore"]);
    let top = newest(&t, 1);
    assert_eq!(top[0].summary.params["name"], "Notion");
}

pub(super) fn tools_list_decks_and_add_classified_notes(f: Factory) {
    let t = TestApp::new(f());
    let (_, child) = english(&t);

    let decks = tool(&t, "list_decks", Value::Null);
    let paths: Vec<&str> = decks["decks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["path"].as_str().unwrap())
        .collect();
    assert_eq!(paths, ["İngilizce", "İngilizce::Fiiller"]);

    // The sub-deck inherits "Seviye" from its parent.
    let cls = tool(&t, "list_classifications", json!({ "deck": "ingilizce::fiiller" }));
    assert_eq!(cls["classifications"][0]["name"], "Seviye");
    assert_eq!(cls["classifications"][0]["inherited"], true);
    assert_eq!(cls["classifications"][0]["options"][2], "B1");

    let types = tool(&t, "list_note_types", json!({}));
    assert!(types["note_types"]
        .as_array()
        .unwrap()
        .iter()
        .any(|nt| nt["default"] == true && nt["fields"][0]["key"] == "front"));

    let added = tool(
        &t,
        "add_notes",
        json!({
            "deck": "İngilizce::Fiiller",
            "notes": [
                { "fields": { "front": "to go", "Back": "gitmek" },
                  "tags": ["verbs"], "classifications": { "Seviye": "b1" } },
                { "fields": { "front": "to be", "back": "olmak" },
                  "classifications": { "seviye": "A2" } },
            ]
        }),
    );
    assert_eq!(added["added"], 2);
    assert_eq!(added["notes"][0]["cards"], 1);
    let first = added["notes"][0]["note_id"].as_str().unwrap().to_owned();

    // The query language sees the classification values.
    assert_eq!(total(&t, "seviye>=B1"), 1);
    assert_eq!(total(&t, "seviye:A2 deck:İngilizce"), 1);
    let hit = tool(&t, "search_cards", json!({ "query": "seviye>=B1", "limit": 5 }));
    assert_eq!(hit["cards"][0]["note_id"], first.as_str());
    assert_eq!(hit["cards"][0]["deck"], "İngilizce::Fiiller");

    let note = tool(&t, "get_note", json!({ "note_id": first }));
    assert_eq!(note["fields"]["back"], "gitmek");
    assert_eq!(note["classifications"]["Seviye"], "B1");
    assert_eq!(note["tags"][0], "verbs");
    let dto = t.ok::<NotesGet>(NoteRef {
        id: first.parse().unwrap(),
    });
    assert_eq!(dto.home_deck_id, child);

    // Both notes are one undo step, recorded with actor mcp.
    let top = newest(&t, 3);
    let creates: Vec<_> = top
        .iter()
        .filter(|e| e.command == "v1.notes.create")
        .collect();
    assert_eq!(creates.len(), 2);
    assert_eq!(creates[0].batch_id, creates[1].batch_id);
    assert!(creates
        .iter()
        .all(|e| e.actor == Actor::Mcp(CLIENT.into())));
    assert_eq!(added["undo_batch_id"], json!(creates[0].batch_id));
    undo(&t);
    assert_eq!(total(&t, ""), 0);
    redo(&t);
    assert_eq!(total(&t, "seviye>=A1"), 2);

    // create_deck is idempotent.
    let d = tool(&t, "create_deck", json!({ "path": "Japonca::Kanji" }));
    assert_eq!(d["created"], true);
    let again = tool(&t, "create_deck", json!({ "path": "japonca::kanji" }));
    assert_eq!(again["created"], false);
    assert_eq!(again["id"], d["id"]);
}

pub(super) fn add_notes_is_all_or_nothing_with_positioned_errors(f: Factory) {
    let t = TestApp::new(f());
    english(&t);
    let one = |front: &str, level: &str| {
        json!({ "fields": { "front": front, "back": "x" }, "classifications": { "Seviye": level } })
    };
    // Unknown option on the 2nd note: nothing added, the error says where.
    let e = tool_err(
        &t,
        "add_notes",
        json!({ "deck": "İngilizce", "notes": [one("a", "B1"), one("b", "Z9")] }),
    );
    assert_eq!(e.code, ErrorCode::NotFoundAttributeOption);
    assert_eq!(e.details["note_index"], 1);
    assert!(e.message.contains("A1, A2, B1"), "{}", e.message);
    // Unknown field.
    let e = tool_err(
        &t,
        "add_notes",
        json!({ "deck": "İngilizce", "notes": [{ "fields": { "kanji": "水" } }] }),
    );
    assert_eq!(e.code, ErrorCode::ValidationInvalidParams);
    assert_eq!(e.details["field"], "kanji");
    // Unknown deck / note type / argument.
    let e = tool_err(&t, "add_notes", json!({ "deck": "Yok", "notes": [one("a", "B1")] }));
    assert_eq!(e.code, ErrorCode::NotFoundDeck);
    let e = tool_err(
        &t,
        "add_notes",
        json!({ "deck": "İngilizce", "note_type": "Nope", "notes": [one("a", "B1")] }),
    );
    assert_eq!(e.code, ErrorCode::NotFoundNoteType);
    let e = tool_err(&t, "list_decks", json!({ "surprise": 1 }));
    assert_eq!(e.code, ErrorCode::ValidationInvalidParams);
    // A duplicate fails the whole batch (the first, valid note is not kept).
    tool(
        &t,
        "add_notes",
        json!({ "deck": "İngilizce", "notes": [one("dup", "B1")] }),
    );
    let e = tool_err(
        &t,
        "add_notes",
        json!({ "deck": "İngilizce", "notes": [one("fresh", "B1"), one("dup", "B1")] }),
    );
    assert_eq!(e.code, ErrorCode::ConflictNoteDuplicate);
    assert_eq!(total(&t, ""), 1);
    let ok = tool(
        &t,
        "add_notes",
        json!({ "deck": "İngilizce", "allow_duplicates": true,
                "notes": [one("dup", "B1")] }),
    );
    assert_eq!(ok["added"], 1);
    let e = tool_err(&t, "nope", json!({}));
    assert_eq!(e.code, ErrorCode::NotFoundMcpTool);
}

pub(super) fn update_note_merges_and_undoes(f: Factory) {
    let t = TestApp::new(f());
    english(&t);
    deck(&t, "Arşiv");
    let added = tool(
        &t,
        "add_notes",
        json!({ "deck": "İngilizce::Fiiller", "notes": [
            { "fields": { "front": "to go", "back": "git" }, "tags": ["a"],
              "classifications": { "Seviye": "A1" } } ] }),
    );
    let id = added["notes"][0]["note_id"].as_str().unwrap().to_owned();
    let before = tool(&t, "get_note", json!({ "note_id": id }));

    let after = tool(
        &t,
        "update_note",
        json!({ "note_id": id, "fields": { "back": "gitmek" },
                "classifications": { "Seviye": "B2" } }),
    );
    assert_eq!(after["fields"]["front"], "to go", "untouched field kept");
    assert_eq!(after["fields"]["back"], "gitmek");
    assert_eq!(after["tags"], before["tags"], "tags kept when not given");
    assert_eq!(after["classifications"]["Seviye"], "B2");
    assert_eq!(newest(&t, 1)[0].actor, Actor::Mcp(CLIENT.into()));

    // Clearing a classification, then moving the home deck.
    let cleared = tool(
        &t,
        "update_note",
        json!({ "note_id": id, "classifications": { "Seviye": null }, "tags": [] }),
    );
    assert_eq!(cleared["classifications"], json!({}));
    assert_eq!(cleared["tags"], json!([]));
    let moved = tool(&t, "update_note", json!({ "note_id": id, "deck": "Arşiv" }));
    assert_eq!(moved["deck"], "Arşiv");
    // Seviye does not apply in Arşiv.
    let e = tool_err(
        &t,
        "update_note",
        json!({ "note_id": id, "classifications": { "Seviye": "A1" } }),
    );
    assert_eq!(e.code, ErrorCode::NotFoundAttribute);

    undo(&t);
    undo(&t);
    undo(&t);
    let restored = tool(&t, "get_note", json!({ "note_id": id }));
    assert_eq!(restored["fields"], before["fields"]);
    assert_eq!(restored["classifications"], before["classifications"]);
    assert_eq!(restored["deck"], before["deck"]);
}

pub(super) fn set_classification_previews_until_confirmed(f: Factory) {
    let t = TestApp::new(f());
    english(&t);
    let notes: Vec<Value> = (0..3)
        .map(|i| json!({ "fields": { "front": format!("w{i}"), "back": "x" }, "tags": ["t"] }))
        .collect();
    tool(&t, "add_notes", json!({ "deck": "İngilizce", "notes": notes }));
    let args = json!({ "query": "tag:t", "classification": "seviye", "value": "C1" });

    let preview = tool(&t, "set_classification", args.clone());
    assert_eq!(preview["applied"], false);
    assert_eq!(preview["matched_notes"], 3);
    assert_eq!(total(&t, "seviye:C1"), 0, "a preview changes nothing");

    let token = preview["confirm_token"].as_str().unwrap().to_owned();

    // `confirm: true` straight away (no preview) or with a made-up token changes nothing.
    let mut blind = args.clone();
    blind["confirm"] = json!(true);
    assert_eq!(tool_err(&t, "set_classification", blind.clone()).code, ErrorCode::ValidationInvalidParams);
    blind["confirm_token"] = json!("0".repeat(32));
    assert_eq!(tool_err(&t, "set_classification", blind).code, ErrorCode::ValidationInvalidParams);
    assert_eq!(total(&t, "seviye:C1"), 0);

    // The token is for this exact selection and value…
    let mut other = args.clone();
    other["value"] = json!("B2");
    other["confirm"] = json!(true);
    other["confirm_token"] = json!(token);
    assert_eq!(tool_err(&t, "set_classification", other).code, ErrorCode::ValidationInvalidParams);

    // …and breaks when the matches change between preview and apply.
    tool(&t, "add_notes", json!({ "deck": "İngilizce", "notes": [
        { "fields": { "front": "late", "back": "x" }, "tags": ["t"] } ] }));
    let mut stale = args.clone();
    stale["confirm"] = json!(true);
    stale["confirm_token"] = json!(token);
    assert_eq!(tool_err(&t, "set_classification", stale).code, ErrorCode::ValidationInvalidParams);
    assert_eq!(total(&t, "seviye:C1"), 0);
    let preview = tool(&t, "set_classification", args.clone());
    assert_eq!(preview["matched_notes"], 4);
    let token = preview["confirm_token"].as_str().unwrap().to_owned();

    let mut confirmed = args.clone();
    confirmed["confirm"] = json!(true);
    confirmed["confirm_token"] = json!(token);
    let done = tool(&t, "set_classification", confirmed);
    assert_eq!(done["applied"], true);
    assert_eq!(done["changed"], 4);
    assert_eq!(total(&t, "seviye:C1"), 4);
    let top = newest(&t, 1);
    assert_eq!(top[0].command, "v1.bulk.set_attribute");
    assert_eq!(top[0].actor, Actor::Mcp(CLIENT.into()));
    undo(&t);
    assert_eq!(total(&t, "seviye:C1"), 0, "one undo step");

    // Nothing matched: nothing to confirm. Bad query: positioned error.
    let none = tool(
        &t,
        "set_classification",
        json!({ "query": "tag:none", "classification": "Seviye", "value": "A1", "confirm": true }),
    );
    assert_eq!(none["applied"], false);
    assert_eq!(none["matched_cards"], 0);
    let e = tool_err(
        &t,
        "search_cards",
        json!({ "query": "seviye>=Z9" }),
    );
    assert_eq!(e.code, ErrorCode::ValidationQuery);
}

pub(super) fn writes_switch_read_only_flag_and_module_switch(f: Factory) {
    let t = TestApp::new(f());
    let names = |read_only: bool| -> Vec<&str> {
        list_tools(&t.app, read_only)
            .unwrap()
            .iter()
            .map(|s| s.name)
            .collect()
    };
    let all = names(false);
    assert!(all.contains(&"add_notes") && all.contains(&"list_decks"));
    let read = names(true);
    assert!(read.contains(&"list_decks") && !read.contains(&"add_notes"));
    let e = call_tool(&t.app, CLIENT, true, "create_deck", json!({ "path": "X" })).unwrap_err();
    assert_eq!(e.code, ErrorCode::PermissionMcpReadOnly);

    // The app setting turns writes off for every client.
    let info = t.ok::<McpToolsList>(Empty {});
    assert!(info.writes_allowed);
    assert_eq!(info.tools.len(), catalog().len());
    t.ok::<SettingsSet>(SetSetting {
        scope: SettingScope::App,
        key: super::ALLOW_WRITES.into(),
        value: json!(false),
    });
    assert!(!t.ok::<McpToolsList>(Empty {}).writes_allowed);
    assert_eq!(names(false), read);
    let e = tool_err(&t, "create_deck", json!({ "path": "X" }));
    assert_eq!(e.code, ErrorCode::PermissionMcpReadOnly);
    assert!(tool(&t, "list_decks", json!({}))["decks"]
        .as_array()
        .unwrap()
        .is_empty());

    // Module off: no tools at all.
    t.ok::<ModulesSetEnabled>(SetModuleEnabled {
        id: super::ID.into(),
        enabled: false,
    });
    let e = list_tools(&t.app, false).unwrap_err();
    assert_eq!(e.code, ErrorCode::ConflictModuleDisabled);
    let e = tool_err(&t, "list_decks", json!({}));
    assert_eq!(e.code, ErrorCode::ConflictModuleDisabled);
}

pub(super) fn mcp_actor_holds_only_curated_permissions(f: Factory) {
    let t = TestApp::new(f());
    let mcp = Actor::Mcp(CLIENT.into());
    let catalog_methods = fc_api::v1::catalog();
    for spec in catalog() {
        for name in spec.methods {
            let d = catalog_methods
                .iter()
                .find(|d| d.name == *name)
                .unwrap_or_else(|| panic!("{} calls unknown method {name}", spec.name));
            assert_eq!(d.visibility, Visibility::Public, "{name} must be public");
            assert!(
                MCP_PERMISSIONS.contains(&d.permission),
                "{} needs {} which MCP lacks",
                spec.name,
                d.permission
            );
            assert_eq!(
                spec.access == McpToolAccess::Read,
                spec.methods
                    .iter()
                    .all(|n| catalog_methods
                        .iter()
                        .any(|d| d.name == *n && d.kind == fc_api::MethodKind::Query)),
                "{}: read tools call only queries, write tools at least one command",
                spec.name
            );
        }
    }
    // Outside the curated set: refused even through the raw dispatcher.
    let denied = [
        (SettingsSet::NAME, json!({ "scope": { "kind": "app" }, "key": "mcp.server.allow_writes", "value": true })),
        (ModulesSetEnabled::NAME, json!({ "id": "mcp", "enabled": false })),
        (OplogUndo::NAME, json!({})),
        (McpServersAdd::NAME, json!({ "name": "evil", "transport": "stdio", "command": "sh" })),
        (DecksRestoreAffected::NAME, json!({})),
        // Held via `notes:write`/`decks:write`, but no tool calls them: deleting is out.
        (NotesDelete::NAME, json!({ "ids": [] })),
        (DecksDelete::NAME, json!({ "id": "00000000-0000-0000-0000-000000000000" })),
        (BrowsePurgeTrash::NAME, json!({})),
        (PresetsDelete::NAME, json!({})),
        (AttributesDelete::NAME, json!({})),
    ];
    for (method, params) in denied {
        let e = t
            .app
            .invoke(&format!("v1.{method}"), params, &mcp)
            .unwrap_err();
        assert_eq!(e.code, ErrorCode::PermissionDenied, "{method}");
    }
    // …and inside batch.execute too.
    let e = t
        .app
        .invoke(
            "v1.batch.execute",
            json!({ "calls": [{ "method": "v1.modules.set_enabled",
                                "params": { "id": "mcp", "enabled": false } }] }),
            &mcp,
        )
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::PermissionDenied);
    assert!(t.app.is_module_enabled(super::ID));

    // Defence in depth: whatever permission a method needs, none of the catalog's methods
    // deletes anything.
    for spec in catalog() {
        for name in spec.methods {
            assert!(
                !name.contains("delete") && !name.contains("purge"),
                "{}: {name}",
                spec.name
            );
        }
    }
}

/// `mcp:write` (registering programs the app will launch) is for the user and the CLI only.
pub(super) fn only_user_and_cli_may_register_servers(f: Factory) {
    let t = TestApp::new(f());
    let add = json!({ "name": "evil", "transport": "stdio", "command": "sh", "args": ["-c", "x"] });
    for actor in [
        Actor::Plugin("p".into()),
        Actor::LocalApi("l".into()),
        Actor::Automation("a".into()),
        Actor::Mcp("m".into()),
    ] {
        let e = t.app.invoke("v1.mcp.servers.add", add.clone(), &actor).unwrap_err();
        assert_eq!(e.code, ErrorCode::PermissionDenied, "{actor:?}");
    }
    assert!(t.ok::<McpServersList>(Empty {}).servers.is_empty());
    for actor in [Actor::User, Actor::Cli] {
        let mut a = add.clone();
        a["name"] = json!(format!("{actor:?}"));
        t.app.invoke("v1.mcp.servers.add", a, &actor).unwrap();
    }
    // Reading stays open to plugins.
    t.app
        .invoke("v1.mcp.servers.list", json!({}), &Actor::Plugin("p".into()))
        .unwrap();
}

/// A registration counts as approved only while it still is exactly what the user saved.
pub(super) fn registrations_are_approved_by_the_user(f: Factory) {
    let t = TestApp::new(f());
    let files = t.ok::<McpServersAdd>(AddMcpServer {
        name: "Files".into(),
        transport: McpTransport::Stdio,
        command: Some("npx".into()),
        args: vec!["-y".into(), "pkg".into()],
        url: None,
        enabled: None,
    });
    assert!(files.approved, "registering is the user's approval");
    let get = || t.ok::<McpServersList>(Empty {}).servers.remove(0);

    // A row written behind the commands' back (a restored backup, a hand edit of the
    // database): stale approval, or none at all, is not approval.
    let mut rec = t.store.read().unwrap().get_mcp_server(files.id).unwrap().unwrap();
    rec.command = "sh".into();
    let mut tx = t.store.write().unwrap();
    tx.upsert_mcp_server(&rec).unwrap();
    tx.commit().unwrap();
    assert!(!get().approved, "the command changed after the approval");
    rec.approved = None;
    let mut tx = t.store.write().unwrap();
    tx.upsert_mcp_server(&rec).unwrap();
    tx.commit().unwrap();
    assert!(!get().approved);

    // Saving it again in the app is the new approval; undo puts the exact old state back.
    let saved = t.ok::<McpServersUpdate>(UpdateMcpServer {
        id: files.id,
        name: "Files".into(),
        transport: McpTransport::Stdio,
        command: "sh".into(),
        args: vec![],
        url: None,
        enabled: true,
    });
    assert!(saved.approved && get().approved);
    undo(&t);
    assert!(!get().approved, "undo does not approve what was never approved");
    redo(&t);
    assert!(get().approved);
}

pub(super) fn set_classification_refuses_selections_over_the_limit(f: Factory) {
    let t = TestApp::new(f());
    english(&t);
    let limit = super::tools::MAX_BULK_CARDS;
    let per = super::tools::MAX_NOTES_PER_CALL;
    for chunk in 0..=(limit / per) {
        let notes: Vec<Value> = (0..per)
            .map(|i| json!({ "fields": { "front": format!("w{chunk}-{i}"), "back": "x" }, "tags": ["big"] }))
            .collect();
        tool(&t, "add_notes", json!({ "deck": "İngilizce", "notes": notes }));
    }
    let args = json!({ "query": "tag:big", "classification": "seviye", "value": "A2" });
    let preview = tool(&t, "set_classification", args.clone());
    assert!(preview["matched_cards"].as_u64().unwrap() > limit as u64);
    assert_eq!(preview["applied"], false);
    assert_eq!(preview["limit"], limit);
    assert!(preview.get("confirm_token").is_none(), "nothing to confirm");
    let mut confirmed = args;
    confirmed["confirm"] = json!(true);
    confirmed["confirm_token"] = json!("x");
    assert_eq!(
        tool_err(&t, "set_classification", confirmed).code,
        ErrorCode::ValidationInvalidParams
    );
    assert_eq!(total(&t, "seviye:A2"), 0);
}

/// The desktop's screens go stale when another process (here: an MCP client) commits; the
/// oplog position tells, and `data_changed` makes screens refetch. Own commits stay quiet.
pub(super) fn foreign_commits_are_noticed_and_announced(f: Factory) {
    use fc_api::v1::events::DomainEvent;
    let t = TestApp::new(f());
    let changed = |t: &TestApp| {
        t.take_events()
            .iter()
            .filter(|e| matches!(e.event, DomainEvent::DataChanged))
            .count()
    };
    assert!(!t.app.poll_external_changes().unwrap());
    t.take_events();

    // The app's own commands (actor user) are not news, nor is an idle poll.
    deck(&t, "Mine");
    assert!(!t.app.poll_external_changes().unwrap());
    assert!(!t.app.poll_external_changes().unwrap());
    assert_eq!(changed(&t), 0);

    // An MCP client writes (as another process would): one announcement, then quiet again.
    tool(&t, "create_deck", json!({ "path": "Claude" }));
    assert!(t.app.poll_external_changes().unwrap());
    assert_eq!(changed(&t), 1);
    assert!(!t.app.poll_external_changes().unwrap());
    assert_eq!(changed(&t), 0);

    // Mixed: own and foreign commits since the last poll still announce once.
    deck(&t, "Mine 2");
    tool(&t, "create_deck", json!({ "path": "Claude 2" }));
    deck(&t, "Mine 3");
    assert!(t.app.poll_external_changes().unwrap());
    assert_eq!(changed(&t), 1);
}

#[test]
fn a_second_process_on_the_same_file_is_noticed() {
    use fc_api::v1::events::DomainEvent;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("collection.db");
    let desktop = TestApp::new(Arc::new(SqliteStore::open(&path).unwrap()));
    assert!(!desktop.app.poll_external_changes().unwrap());
    desktop.take_events();

    // `fc mcp serve` is its own process with its own connection to the same file.
    let server = TestApp::new(Arc::new(SqliteStore::open_current(&path).unwrap()));
    tool(&server, "create_deck", json!({ "path": "From Claude" }));

    assert!(desktop.app.poll_external_changes().unwrap());
    assert!(desktop
        .take_events()
        .iter()
        .any(|e| matches!(e.event, DomainEvent::DataChanged)));
    let decks = desktop.ok::<DecksList>(Empty {}).decks;
    assert!(decks.iter().any(|d| d.path == "From Claude"));
}

/// Phase-2 history rules hold for MCP writes: an `add_notes` batch is one ordinary undo step
/// whose stored inverses pass the replay allow-list, and a history barrier (here: emptying the
/// trash) stops undo before reaching what Claude wrote earlier.
pub(super) fn mcp_writes_are_ordinary_undo_steps_behind_barriers(f: Factory) {
    use fc_api::v1::browse::PurgeTrash;
    use fc_api::v1::notes::NoteIds;
    let t = TestApp::new(f());
    deck(&t, "Lang");
    let one = |front: &str| json!({ "fields": { "front": front, "back": "x" } });
    let added = tool(&t, "add_notes", json!({ "deck": "Lang", "notes": [one("a"), one("b")] }));
    assert_eq!(added["added"], 2);
    // User undo/redo of Claude's batch runs its stored inverses (all replayable).
    undo(&t);
    assert_eq!(total(&t, "deck:Lang"), 0);
    redo(&t);
    assert_eq!(total(&t, "deck:Lang"), 2);

    // The user trashes one of Claude's notes and empties the trash: a barrier.
    let a: fc_core::NoteId = added["notes"][0]["note_id"].as_str().unwrap().parse().unwrap();
    t.ok::<NotesDelete>(NoteIds { ids: vec![a] });
    t.ok::<BrowsePurgeTrash>(PurgeTrash {
        older_than_days: Some(0),
    });
    // Claude writes again after the barrier: that step undoes normally…
    tool(&t, "create_deck", json!({ "path": "After" }));
    undo(&t);
    assert!(!t
        .ok::<DecksList>(Empty {})
        .decks
        .iter()
        .any(|d| d.path == "After"));
    // …but undo stops at the barrier, before Claude's earlier batch.
    let e = t.err::<OplogUndo>(Empty {});
    assert_eq!(e.code, ErrorCode::ConflictHistoryBarrier, "{e:?}");
    assert_eq!(e.details["reason"], "purge");
    assert_eq!(total(&t, "deck:Lang"), 1);
}

/// Every command an MCP tool reaches records an ordinary undo step that undo/redo may replay
/// (`batch.execute` is only the envelope of `add_notes`; its nested calls are checked here
/// too). The external-server registry's commands and their internal inverse are replayable
/// as well (a restored backup cannot smuggle an approval in: restore clears the oplog).
#[test]
fn mcp_reachable_commands_are_replayable() {
    let t = TestApp::mem();
    let registry = t.app.registry();
    let mut checked = 0;
    for spec in catalog() {
        for name in spec.methods {
            let d = registry.descriptor(name).unwrap();
            if d.kind != fc_api::MethodKind::Command || *name == BatchExecute::NAME {
                continue;
            }
            assert!(d.undoable && d.replayable, "{} via tool {}", d.name, spec.name);
            checked += 1;
        }
    }
    assert!(checked > 0);
    for name in [
        McpServersAdd::NAME,
        McpServersUpdate::NAME,
        McpServersRemove::NAME,
        McpServersRestore::NAME,
    ] {
        let d = registry.descriptor(name).unwrap();
        assert!(d.undoable && d.replayable, "{name}");
    }
}

/// `fc mcp serve` attaches to a collection the app may have open: none of the owning app's
/// startup work (jobs marked interrupted, crash rollback, trash purge) runs, nothing is written.
#[test]
fn serving_attaches_without_startup_work() {
    use fc_api::v1::io::CreateBackup;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("collection.db");
    let desktop = TestApp::new(Arc::new(SqliteStore::open(&path).unwrap()));
    deck(&desktop, "Mine");
    let job = desktop.ok::<IoBackupCreate>(CreateBackup {
        path: dir.path().join("b.fcbackup").to_string_lossy().into_owned(),
    });
    let before = desktop.store.read().unwrap().oplog_watermark().unwrap();

    let _server = crate::App::open_sqlite_current(&path).unwrap();

    let tx = desktop.store.read().unwrap();
    assert_eq!(tx.oplog_watermark().unwrap(), before, "no oplog write");
    let j = tx.get_job(job.id).unwrap().unwrap();
    assert_eq!(j.status, "queued", "the app's job is untouched");
}
