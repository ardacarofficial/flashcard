//! v0.7 acceptance (docs/04): the example plugins of `plugins-sdk/examples` are built from
//! Rust to WASM, packed and installed through the public API, on every store.
//!
//! * new card type, new study mode, TTS provider, hook filling a field → `example_*`
//! * a host call without permission is refused → `host_calls_without_permission_are_refused`
//! * infinite loops / panics / memory hogs do not crash the app → `misbehaving_plugins_cannot_crash_the_app`
//! * the plugin API is versioned; a v1 plugin keeps running → `plugin_api_v1_contract`

use super::*;
use crate::testing::{MemStore, TempSqlite, TestApp};
use fc_api::v1::cardtypes::{CardInteraction, CardRef};
use fc_api::v1::decks::{CreateDeck, DeckDto, DeckRef};
use fc_api::InvokeRequest;
use fc_api::v1::modules::UiSlot;
use fc_api::v1::notes::{CreateNote, NoteDto, NoteRef};
use fc_api::v1::cardtypes::{CreateNoteType, NoteTypeInput};
use fc_api::v1::settings::SetSetting;
use fc_api::v1::study::CardHistoryRequest;
use fc_api::v1::tts::{ListVoices, Synthesize};
use fc_api::ApiMethod;
use fc_core::notetype::{builtin, CardTemplate, FieldDef, FieldKey, FieldKind, NoteTypeKind};
use fc_storage::{SqliteStore, Store};
use std::collections::BTreeMap;

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
    example_card_type_plugin,
    example_study_mode_plugin,
    example_tts_provider_plugin,
    example_hook_fills_a_field,
    host_calls_without_permission_are_refused,
    misbehaving_plugins_cannot_crash_the_app,
    failed_operations_roll_back,
    plugin_api_v1_contract,
    install_update_uninstall_are_undoable,
    model_download_manager,
    hooks_cannot_reach_management_io_or_settings,
    plugins_get_no_free_paths_or_app_settings,
    plugin_storage_rolls_back_with_the_call,
    failed_hook_rollback_does_not_cascade,
    hook_budget_per_action,
    plugin_storage_needs_permission_and_quota,
);

const AUTOFILL: &str = "com.example.autofill";
const SCRAMBLE: &str = "com.example.scramble";
const TONE: &str = "com.example.tone";
const REVERSE: &str = "com.example.reverse";
const MISBEHAVE: &str = "com.example.misbehave";
const ESCAPE: &str = "com.example.escape";

/// Writes example `name` as a `.fcplugin` file; returns its path (the dir lives with `t`).
fn package_file(dir: &tempfile::TempDir, name: &str) -> String {
    let bytes = fc_plugins::testing::example_package(name).unwrap_or_else(|e| panic!("{e}"));
    let path = dir.path().join(format!("{name}.fcplugin"));
    std::fs::write(&path, bytes).unwrap();
    path.display().to_string()
}

fn install(t: &TestApp, dir: &tempfile::TempDir, name: &str, grant: &[&str]) -> PluginDto {
    t.ok::<PluginsInstall>(InstallPlugin {
        path: package_file(dir, name),
        grant: grant.iter().map(|g| (*g).to_owned()).collect(),
    })
}

fn deck(t: &TestApp, path: &str) -> DeckDto {
    t.ok::<DecksCreate>(CreateDeck {
        path: path.into(),
        parent_id: None,
        description: None,
        icon: None,
        color: None,
    })
}

fn classic(t: &TestApp, d: &DeckDto, front: &str, back: &str) -> NoteDto {
    t.ok::<NotesCreate>(CreateNote {
        notetype_id: builtin::CLASSIC_ID,
        deck_id: d.id,
        fields: [("front", front), ("back", back)]
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect(),
        tags: vec![],
        attributes: vec![],
        allow_duplicate: Some(true),
    })
}

fn run(t: &TestApp, plugin: &str, command: &str) -> Result<PluginCommandResult, ApiError> {
    t.app.call::<PluginsRun>(
        &RunPluginCommand {
            plugin_id: plugin.into(),
            command: command.into(),
            params: Value::Null,
        },
        &Actor::User,
    )
}

fn plugin(t: &TestApp, id: &str) -> PluginDto {
    t.ok::<PluginsList>(Empty {})
        .plugins
        .into_iter()
        .find(|p| p.id == id)
        .unwrap_or_else(|| panic!("{id} not listed"))
}

fn set_app_setting(t: &TestApp, key: &str, value: Value) {
    t.ok::<SettingsSet>(SetSetting {
        scope: SettingScope::App,
        key: key.into(),
        value,
    });
}

fn undo(t: &TestApp) {
    t.ok::<OplogUndo>(Empty {});
}

fn redo(t: &TestApp) {
    t.ok::<OplogRedo>(Empty {});
}

// ------------------------------------------------------------------ acceptance 1

fn example_card_type_plugin(f: Factory) {
    let t = TestApp::new(f());
    let dir = tempfile::tempdir().unwrap();
    let engine_id = format!("plugin.{SCRAMBLE}.scramble");
    // Without its permission the card type is not offered.
    install(&t, &dir, "scramble-card", &[]);
    let ids = |t: &TestApp| -> Vec<String> {
        t.ok::<CardtypesList>(Empty {}).card_types.into_iter().map(|c| c.id).collect()
    };
    assert!(!ids(&t).contains(&engine_id));
    t.ok::<PluginsSetPermissions>(SetPluginPermissions {
        id: SCRAMBLE.into(),
        granted: vec!["card_types:provide".into()],
    });
    assert!(ids(&t).contains(&engine_id));

    let field = |name: &str| FieldDef {
        key: FieldKey::new(""),
        name: name.into(),
        kind: FieldKind::Text,
        required: false,
        unique: false,
        searchable: true,
        default_value: None,
    };
    let nt = t.ok::<NotetypesCreate>(CreateNoteType {
        name: None,
        copy_from: None,
        definition: Some(NoteTypeInput {
            name: "Scramble".into(),
            kind: NoteTypeKind::Standard,
            fields: vec![field("Front"), field("Back")],
            templates: vec![CardTemplate {
                key: "s".into(),
                name: "Scramble".into(),
                front: "{{front}}".into(),
                back: "{{back}}".into(),
                card_type: engine_id.clone(),
            }],
            css: String::new(),
            sort_field: FieldKey::new(""),
        }),
        id: None,
    });
    let d = deck(&t, "Words");
    let note = t.ok::<NotesCreate>(CreateNote {
        notetype_id: nt.id,
        deck_id: d.id,
        fields: [("front", "kalkan"), ("back", "<b>shield</b><script>x()</script>")]
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect(),
        tags: vec![],
        attributes: vec![],
        allow_duplicate: Some(true),
    });
    assert_eq!(note.cards.len(), 1, "the plugin generated one card");
    let r = t.ok::<CardsRender>(CardRef { id: note.cards[0].id });
    assert_eq!(r.sides.card_type, engine_id);
    assert!(!r.sides.question.contains("kalkan"), "letters are shuffled: {}", r.sides.question);
    assert!(!r.sides.question.contains("<script"), "plugin HTML is sanitized");
    assert_eq!(
        r.sides.interaction,
        CardInteraction::Type {
            expected: "kalkan".into()
        }
    );
    // Same card, same order (seeded by the card).
    assert_eq!(t.ok::<CardsRender>(CardRef { id: note.cards[0].id }).sides.question, r.sides.question);
    // An empty word yields no card (the engine decides; the app refuses a cardless note).
    let e = t.err::<NotesCreate>(CreateNote {
        notetype_id: nt.id,
        deck_id: d.id,
        fields: BTreeMap::new(),
        tags: vec![],
        attributes: vec![],
        allow_duplicate: Some(true),
    });
    assert_eq!(e.code, ErrorCode::ValidationNoteType);
    // Disabled → the type goes away (cards stay, rendering says why).
    t.ok::<PluginsSetEnabled>(SetPluginEnabled {
        id: SCRAMBLE.into(),
        enabled: false,
    });
    assert!(!ids(&t).contains(&engine_id));
    assert_eq!(
        t.err::<CardsRender>(CardRef { id: note.cards[0].id }).code,
        ErrorCode::NotFoundCardType
    );
    undo(&t);
    assert!(ids(&t).contains(&engine_id));
}

fn example_study_mode_plugin(f: Factory) {
    let t = TestApp::new(f());
    let dir = tempfile::tempdir().unwrap();
    install(
        &t,
        &dir,
        "reverse-drill",
        &["ui:study_mode", "ui:menu", "study:write", "notes:read"],
    );
    // It shows up as a module with a main menu entry opening the mode.
    let modules = t.ok::<ModulesList>(Empty {}).modules;
    let m = modules
        .iter()
        .find(|m| m.id == format!("plugin:{REVERSE}"))
        .expect("plugin module listed");
    assert!(m.ui.iter().any(|u| u.slot == UiSlot::MainMenu
        && u.route.as_deref() == Some("/plugins/view/com.example.reverse/reverse")));

    let d = deck(&t, "English");
    let note = classic(&t, &d, "kılıç", "sword");
    let open = t.ok::<PluginsUiOpen>(OpenPluginUi {
        plugin_id: REVERSE.into(),
        view: "reverse".into(),
        input: json!({ "deck_id": d.id }),
    });
    assert!(!open.done);
    let shown = serde_json::to_string(&open.node).unwrap();
    assert!(shown.contains("sword"), "{shown}");
    assert!(!shown.contains("kılıç"), "the answer is not shown");

    let step = t.ok::<PluginsUiAct>(ActPluginUi {
        session_id: open.session_id.clone(),
        action: "check".into(),
        values: [("answer".to_owned(), json!(" Kılıç "))].into_iter().collect(),
    });
    let shown = serde_json::to_string(&step.node).unwrap();
    assert!(shown.contains("\"correct\":true"), "{shown}");
    // Recorded through the study API: one review, rating good, written by the plugin.
    let history = |t: &TestApp| {
        t.ok::<StudyCardHistory>(CardHistoryRequest {
            card_id: note.cards[0].id,
        })
        .reviews
    };
    let reviews = history(&t);
    assert_eq!(reviews.len(), 1);
    assert_eq!(reviews[0].rating, fc_core::card::Rating::Good);
    let last = t.ok::<OplogList>(Default::default()).items.remove(0);
    assert_eq!(last.actor, Actor::Plugin(REVERSE.into()));
    assert_eq!(last.summary.key, "oplog.summary.plugins.run");
    // One undo step reverts what the step recorded.
    undo(&t);
    assert!(history(&t).is_empty());
    redo(&t);
    assert_eq!(history(&t).len(), 1);
    // The deck has a single card: the session ends; closing forgets it.
    t.ok::<PluginsUiClose>(ClosePluginUi {
        session_id: open.session_id.clone(),
    });
    assert_eq!(
        t.err::<PluginsUiAct>(ActPluginUi {
            session_id: open.session_id,
            action: "check".into(),
            values: BTreeMap::new(),
        })
        .code,
        ErrorCode::NotFoundPluginSession
    );
    // Without `ui:study_mode` the mode cannot be opened.
    t.ok::<PluginsSetPermissions>(SetPluginPermissions {
        id: REVERSE.into(),
        granted: vec!["study:write".into(), "notes:read".into()],
    });
    assert_eq!(
        t.err::<PluginsUiOpen>(OpenPluginUi {
            plugin_id: REVERSE.into(),
            view: "reverse".into(),
            input: Value::Null,
        })
        .code,
        ErrorCode::ConflictPluginInactive
    );
}

fn example_tts_provider_plugin(f: Factory) {
    let t = TestApp::new(f());
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(
        t.err::<TtsSynthesize>(Synthesize {
            provider: None,
            text: "merhaba".into(),
            language: "tr".into(),
            voice: None,
            rate: None,
        })
        .code,
        ErrorCode::ProviderUnavailable
    );
    install(&t, &dir, "tone-tts", &["tts:provide"]);
    let id = format!("plugin.{TONE}.tone");
    let providers = t.ok::<TtsProviders>(Empty {}).providers;
    assert_eq!(providers.len(), 1);
    assert_eq!(providers[0].id, id);
    assert_eq!(providers[0].name_key, format!("plugin.{TONE}.tts.tone"));
    let voices = t.ok::<TtsVoices>(ListVoices { provider: None });
    assert_eq!(voices.provider, id);
    assert_eq!(voices.voices.len(), 2);
    let audio = t.ok::<TtsSynthesize>(Synthesize {
        provider: Some(id.clone()),
        text: "kılıç".into(),
        language: "tr".into(),
        voice: None,
        rate: Some(1.0),
    });
    use base64::Engine as _;
    let wav = base64::engine::general_purpose::STANDARD.decode(audio.data_b64).unwrap();
    assert_eq!(audio.mime, "audio/wav");
    assert_eq!(&wav[0..4], b"RIFF");
    assert_eq!(&wav[8..12], b"WAVE");
    // 5 letters × 40 ms at 8 kHz, 16-bit.
    assert_eq!(wav.len(), 44 + 5 * 320 * 2);
    // A plugin error (empty text) is a provider error, not a crash.
    let e = t.err::<TtsSynthesize>(Synthesize {
        provider: Some(id),
        text: " ".into(),
        language: "tr".into(),
        voice: None,
        rate: None,
    });
    assert_eq!(e.code, ErrorCode::ProviderFailed);
}

fn example_hook_fills_a_field(f: Factory) {
    let t = TestApp::new(f());
    let dir = tempfile::tempdir().unwrap();
    let d = deck(&t, "English");
    install(
        &t,
        &dir,
        "autofill",
        &["hooks:before:notes.create", "notes:write", "notetypes:read", "ui:command", "storage:kv"],
    );
    let filled = classic(&t, &d, "sword", "");
    assert_eq!(filled.fields["back"], "kılıç");
    // Stored, not just returned.
    assert_eq!(t.ok::<NotesGet>(NoteRef { id: filled.id }).fields["back"], "kılıç");
    // A word it does not know, or a field already filled, is left alone.
    assert_eq!(classic(&t, &d, "zebra", "").fields["back"], "");
    assert_eq!(classic(&t, &d, "book", "cilt").fields["back"], "cilt");
    // The hook is part of the command: one undo removes the note.
    let n = classic(&t, &d, "water", "");
    assert_eq!(n.fields["back"], "su");
    undo(&t);
    let live = t.ok::<NotesList>(fc_api::v1::notes::ListNotes {
        deck_id: None,
        include_subdecks: None,
        page: Default::default(),
    });
    assert!(live.items.iter().all(|i| i.id != n.id));
    // Plugin storage + a contributed command as a runtime-registered method.
    let count = t
        .app
        .invoke(
            "v1.plugin.com.example.autofill.filled-count",
            json!({}),
            &Actor::User,
        )
        .unwrap();
    assert_eq!(count["result"], json!(2), "sword and water (storage is not undone)");
    assert!(t
        .ok::<PaletteListMethod>(Empty {})
        .entries
        .iter()
        .any(|e| e.method == "v1.plugin.com.example.autofill.filled-count"
            && e.label_key == "plugin.com.example.autofill.commands.filled-count"));
    // Plugin settings drive it.
    let s = t.ok::<PluginsSettingsGet>(PluginRef { id: AUTOFILL.into() });
    assert_eq!(s.values["target_field"], json!("back"));
    t.ok::<PluginsSettingsSet>(SetPluginSetting {
        id: AUTOFILL.into(),
        key: "target_field".into(),
        value: Some(json!("nope")),
    });
    assert_eq!(classic(&t, &d, "shield", "").fields["back"], "");
    undo(&t); // the note
    undo(&t); // the setting
    assert_eq!(classic(&t, &d, "shield", "").fields["back"], "kalkan");
    // Revoking the hook's target permission switches the hook off (a hook never gives more
    // than calling the method would)…
    t.ok::<PluginsSetPermissions>(SetPluginPermissions {
        id: AUTOFILL.into(),
        granted: vec!["hooks:before:notes.create".into(), "notetypes:read".into()],
    });
    assert_eq!(classic(&t, &d, "sword", "").fields["back"], "");
    // …and so does revoking the hook permission.
    t.ok::<PluginsSetPermissions>(SetPluginPermissions {
        id: AUTOFILL.into(),
        granted: vec!["notes:write".into(), "notetypes:read".into()],
    });
    assert_eq!(classic(&t, &d, "sword", "").fields["back"], "");
}

// ------------------------------------------------------------------ acceptance 2

fn host_calls_without_permission_are_refused(f: Factory) {
    let t = TestApp::new(f());
    let dir = tempfile::tempdir().unwrap();
    // decks:write is requested but not granted.
    install(&t, &dir, "misbehave", &["ui:command", "decks:read"]);
    let before = t.ok::<DecksList>(Empty {});
    let e = run(&t, MISBEHAVE, "forbidden").unwrap_err();
    assert_eq!(e.code, ErrorCode::ProviderPluginFailed);
    assert_eq!(e.details["code"], json!("PERMISSION_DENIED"));
    assert_eq!(t.ok::<DecksList>(Empty {}), before, "nothing was written");
    // What is granted works.
    assert!(run(&t, MISBEHAVE, "allowed").unwrap().result["decks"].is_array());
    // Writing its own storage needs `storage:kv` too.
    let e = run(&t, MISBEHAVE, "kv").unwrap_err();
    assert_eq!(e.details["code"], json!("PERMISSION_DENIED"));
    // The same check every caller goes through: a plugin actor without the grant…
    let actor = Actor::Plugin(MISBEHAVE.into());
    let e = t
        .app
        .invoke("v1.decks.create", json!({ "path": "X" }), &actor)
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::PermissionDenied);
    assert_eq!(e.details["permission"], json!("decks:write"));
    // …an unknown plugin, or plugin management, is refused too.
    assert_eq!(
        t.app
            .invoke("v1.decks.list", json!({}), &Actor::Plugin("com.nobody.x".into()))
            .unwrap_err()
            .code,
        ErrorCode::PermissionDenied
    );
    assert_eq!(
        t.app
            .invoke("v1.plugins.list", json!({}), &actor)
            .unwrap_err()
            .code,
        ErrorCode::PermissionDenied
    );
    // A hook it was not granted does not run; granted, it does (and may reject).
    deck(&t, "blocked");
    t.ok::<PluginsSetPermissions>(SetPluginPermissions {
        id: MISBEHAVE.into(),
        granted: vec!["hooks:before:decks.create".into(), "decks:write".into()],
    });
    let e = t.err::<DecksCreate>(CreateDeck {
        path: "blocked".into(),
        parent_id: None,
        description: None,
        icon: None,
        color: None,
    });
    assert_eq!(e.code, ErrorCode::ConflictPluginRejected);
    // Revoked commands are gone from the registry.
    assert_eq!(
        t.app
            .invoke("v1.plugin.com.example.misbehave.ok", json!({}), &Actor::User)
            .unwrap_err()
            .code,
        ErrorCode::NotFoundMethod
    );
    // Granting a permission the plugin did not ask for is invalid.
    assert_eq!(
        t.err::<PluginsSetPermissions>(SetPluginPermissions {
            id: MISBEHAVE.into(),
            granted: vec!["notes:write".into()],
        })
        .code,
        ErrorCode::ValidationPluginPermission
    );
}

// ------------------------------------------------------------------ acceptance 3

fn fault_kind(r: Result<PluginCommandResult, ApiError>) -> String {
    let e = r.unwrap_err();
    assert_eq!(e.code, ErrorCode::ProviderPluginFailed, "{e:?}");
    e.details["kind"].as_str().unwrap().to_owned()
}

fn misbehaving_plugins_cannot_crash_the_app(f: Factory) {
    let t = TestApp::new(f());
    let dir = tempfile::tempdir().unwrap();
    // Plenty of fuel: the deadline ends the loop.
    set_app_setting(&t, SETTING_FUEL, json!(1_000_000));
    set_app_setting(&t, SETTING_TIMEOUT, json!(300));
    set_app_setting(&t, SETTING_MEMORY, json!(16));
    set_app_setting(&t, SETTING_MAX_FAILURES, json!(3));
    install(
        &t,
        &dir,
        "misbehave",
        &[
            "ui:command",
            "hooks:before:decks.create",
            "hooks:event:note_created",
            "decks:read",
            "decks:write",
        ],
    );
    let started = std::time::Instant::now();
    assert_eq!(fault_kind(run(&t, MISBEHAVE, "loop")), "TIMEOUT");
    assert!(started.elapsed() < std::time::Duration::from_secs(10));
    assert_eq!(fault_kind(run(&t, MISBEHAVE, "panic")), "TRAP");
    assert_eq!(plugin(&t, MISBEHAVE).failures, 2);
    // A success resets the count; the app works throughout.
    assert_eq!(run(&t, MISBEHAVE, "ok").unwrap().result, json!("ok"));
    assert_eq!(plugin(&t, MISBEHAVE).failures, 0);
    assert_eq!(fault_kind(run(&t, MISBEHAVE, "alloc")), "MEMORY_LIMIT");
    // Fuel: with almost none, the loop dies on fuel before the deadline.
    set_app_setting(&t, SETTING_FUEL, json!(1));
    set_app_setting(&t, SETTING_TIMEOUT, json!(60_000));
    assert_eq!(fault_kind(run(&t, MISBEHAVE, "loop")), "OUT_OF_FUEL");
    assert_eq!(plugin(&t, MISBEHAVE).failures, 2);
    set_app_setting(&t, SETTING_FUEL, json!(2000));
    set_app_setting(&t, SETTING_TIMEOUT, json!(300));

    // Failing hooks never stop the core operation (default: continue without the plugin).
    let d = deck(&t, "hook-panic");
    assert_eq!(d.name, "hook-panic");
    let p = plugin(&t, MISBEHAVE);
    assert_eq!(p.status, PluginStatus::AutoDisabled, "third fault in a row");
    assert!(p.last_error.is_some());
    assert!(t.take_events().iter().any(|e| matches!(
        &e.event,
        DomainEvent::PluginsChanged { plugin_id, auto_disabled: true, reason: None } if plugin_id == MISBEHAVE
    )));
    assert_eq!(run(&t, MISBEHAVE, "ok").unwrap_err().code, ErrorCode::ConflictPluginInactive);
    // Its hooks are off now.
    assert_eq!(deck(&t, "hook-loop").name, "hook-loop");

    // Switching it back on clears the counter; an event handler that panics after a note
    // was created leaves the note in place.
    t.ok::<PluginsSetEnabled>(SetPluginEnabled {
        id: MISBEHAVE.into(),
        enabled: true,
    });
    assert_eq!(plugin(&t, MISBEHAVE).status, PluginStatus::Active);
    let n = classic(&t, &d, "a", "b");
    assert_eq!(t.ok::<NotesGet>(NoteRef { id: n.id }).fields["front"], "a");
    assert_eq!(plugin(&t, MISBEHAVE).failures, 1);
    // `reject` policy: a failing before-hook stops the call instead.
    set_app_setting(&t, SETTING_HOOK_FAILURE, json!("reject"));
    let e = t.err::<DecksCreate>(CreateDeck {
        path: "hook-panic".into(),
        parent_id: None,
        description: None,
        icon: None,
        color: None,
    });
    assert_eq!(e.code, ErrorCode::ProviderPluginFailed);
    assert_eq!(e.details["kind"], json!("TRAP"));
    // The log channel kept the story.
    let logs = t.ok::<PluginsLogs>(PluginRef { id: MISBEHAVE.into() }).entries;
    assert!(logs.iter().any(|l| l.message.contains("TIMEOUT")), "{logs:?}");
}

fn failed_operations_roll_back(f: Factory) {
    let t = TestApp::new(f());
    let dir = tempfile::tempdir().unwrap();
    install(&t, &dir, "misbehave", &["ui:command", "decks:read", "decks:write", "storage:kv"]);
    let before = t.ok::<DecksList>(Empty {});
    let status = t.ok::<OplogStatus>(Empty {});
    assert_eq!(fault_kind(run(&t, MISBEHAVE, "write-panic")), "TRAP");
    assert_eq!(t.ok::<DecksList>(Empty {}), before, "the half-done write was rolled back");
    assert_eq!(t.ok::<OplogStatus>(Empty {}), status, "and left no undo step");
    // Private storage survives calls (instances do not).
    assert_eq!(run(&t, MISBEHAVE, "kv").unwrap().result, json!(1));
    assert_eq!(run(&t, MISBEHAVE, "kv").unwrap().result, json!(2));
}

// ------------------------------------------------------------------ acceptance 4

/// The v1 surface a plugin may rely on: method names with kind and permission. Checked in;
/// later app versions may add methods but never remove or change these
/// (`FC_WRITE_PLUGIN_API_SNAPSHOT=1` regenerates it — only when starting API v2).
const V1_SNAPSHOT: &str = include_str!("api_v1.json");

fn plugin_api_v1_contract(f: Factory) {
    let t = TestApp::new(f());
    let current: BTreeMap<String, (String, String)> = t
        .app
        .registry()
        .methods()
        .filter(|d| {
            d.visibility == Visibility::Public
                && fc_plugins::permission::api_grantable(d.permission)
        })
        .map(|d| {
            (
                d.qualified_name(),
                (format!("{:?}", d.kind).to_lowercase(), d.permission.to_owned()),
            )
        })
        .collect();
    if std::env::var_os("FC_WRITE_PLUGIN_API_SNAPSHOT").is_some() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/src/modules/plugins/api_v1.json");
        std::fs::write(path, serde_json::to_string_pretty(&current).unwrap() + "\n").unwrap();
    }
    let frozen: BTreeMap<String, (String, String)> = serde_json::from_str(V1_SNAPSHOT).unwrap();
    for (name, spec) in &frozen {
        assert_eq!(current.get(name), Some(spec), "v1 plugin API changed: {name}");
    }

    // A v1 plugin written by hand against the frozen ABI (fixture never edited) still runs:
    // its host call goes through permissions like any other.
    let wasm = include_bytes!("../../../../fc-plugins/tests/fixtures/abi_v1.wat");
    let manifest = |api: &str| {
        format!(
            "id = \"com.example.v1\"\nname = \"V1\"\nversion = \"1.0.0\"\napi_version = \"{api}\"\n\
             permissions = [\"ui:command\", \"decks:read\"]\n[contributes]\ncommands = [\"list\"]\n"
        )
    };
    let dir = tempfile::tempdir().unwrap();
    let binary = fc_plugins::testing::wat2wasm(wasm).unwrap();
    let write = |name: &str, api: &str| {
        let bytes = fc_plugins::package::pack(&manifest(api), &binary, &[]).unwrap();
        let path = dir.path().join(name);
        std::fs::write(&path, bytes).unwrap();
        path.display().to_string()
    };
    let v1 = write("v1.fcplugin", "1");
    t.ok::<PluginsInstall>(InstallPlugin {
        path: v1.clone(),
        grant: vec!["ui:command".into()],
    });
    let e = run(&t, "com.example.v1", "list").unwrap_err();
    assert_eq!(e.details["code"], json!("PERMISSION_DENIED"));
    t.ok::<PluginsSetPermissions>(SetPluginPermissions {
        id: "com.example.v1".into(),
        granted: vec!["ui:command".into(), "decks:read".into()],
    });
    deck(&t, "Kept");
    let listed = run(&t, "com.example.v1", "list").unwrap().result;
    assert!(serde_json::to_string(&listed).unwrap().contains("Kept"), "{listed}");
    // A plugin for an API version this app does not run is refused with its own code.
    let v2 = write("v2.fcplugin", "2");
    let e = t.err::<PluginsInspect>(InspectPackage { path: v2 });
    assert_eq!(e.code, ErrorCode::ValidationPluginApiVersion);
    assert_eq!(e.details["api_version"], json!("2"));
}

// ------------------------------------------------------------------ lifecycle

fn install_update_uninstall_are_undoable(f: Factory) {
    let t = TestApp::new(f());
    let dir = tempfile::tempdir().unwrap();
    let path = package_file(&dir, "misbehave");
    let info = t.ok::<PluginsInspect>(InspectPackage { path: path.clone() });
    assert_eq!(info.id, MISBEHAVE);
    assert_eq!(info.installed_version, None);
    assert!(info.permissions.iter().all(|p| !p.granted));
    let listed = |t: &TestApp| t.ok::<PluginsList>(Empty {}).plugins.len();

    let p = t.ok::<PluginsInstall>(InstallPlugin {
        path: path.clone(),
        grant: vec!["ui:command".into(), "storage:kv".into()],
    });
    assert_eq!((p.version.as_str(), p.status), ("1.0.0", PluginStatus::Active));
    assert_eq!(p.contributes.commands.len(), 8);
    undo(&t);
    assert_eq!(listed(&t), 0);
    assert!(run(&t, MISBEHAVE, "ok").is_err());
    redo(&t);
    assert_eq!(run(&t, MISBEHAVE, "ok").unwrap().result, json!("ok"));

    // Private data, then uninstall with data removal; undo brings both back.
    assert_eq!(run(&t, MISBEHAVE, "kv").unwrap().result, json!(1));
    t.ok::<PluginsUninstall>(UninstallPlugin {
        id: MISBEHAVE.into(),
        delete_data: true,
    });
    assert_eq!(listed(&t), 0);
    assert_eq!(
        t.err::<PluginsSetEnabled>(SetPluginEnabled {
            id: MISBEHAVE.into(),
            enabled: false
        })
        .code,
        ErrorCode::NotFoundPlugin
    );
    undo(&t);
    assert_eq!(run(&t, MISBEHAVE, "kv").unwrap().result, json!(2), "storage came back");

    // Disable / enable and grants are undo steps too.
    t.ok::<PluginsSetEnabled>(SetPluginEnabled {
        id: MISBEHAVE.into(),
        enabled: false,
    });
    assert_eq!(run(&t, MISBEHAVE, "ok").unwrap_err().code, ErrorCode::ConflictPluginInactive);
    undo(&t);
    assert!(run(&t, MISBEHAVE, "ok").is_ok());

    // Reinstalling (update path) keeps grants given now; undo goes back to the old row.
    t.ok::<PluginsInstall>(InstallPlugin {
        path,
        grant: vec![],
    });
    assert!(plugin(&t, MISBEHAVE).permissions.iter().all(|p| !p.granted));
    undo(&t);
    assert!(plugin(&t, MISBEHAVE).permissions.iter().any(|p| p.granted));

    // Broken packages are refused with their own codes.
    let junk = dir.path().join("junk.fcplugin");
    std::fs::write(&junk, b"junk").unwrap();
    assert_eq!(
        t.err::<PluginsInstall>(InstallPlugin {
            path: junk.display().to_string(),
            grant: vec![]
        })
        .code,
        ErrorCode::ValidationPluginPackage
    );
    assert_eq!(
        t.err::<PluginsInstall>(InstallPlugin {
            path: dir.path().join("missing.fcplugin").display().to_string(),
            grant: vec![]
        })
        .code,
        ErrorCode::IoFile
    );
    // Locales are served per language.
    let loc = t.ok::<PluginsLocales>(GetPluginLocales { lang: "tr".into() });
    assert_eq!(loc.plugins[MISBEHAVE]["name"], "Yaramaz");
}

fn model_download_manager(f: Factory) {
    let t = TestApp::new(f());
    let dir = tempfile::tempdir().unwrap();
    t.app.set_data_dir(dir.path().join("collection"));
    let model = dir.path().join("voice.bin");
    let bytes: Vec<u8> = (0..100_000u32).map(|i| (i % 7) as u8).collect();
    std::fs::write(&model, &bytes).unwrap();
    let manifest = format!(
        "id = \"com.example.models\"\nname = \"Models\"\nversion = \"1.0.0\"\napi_version = \"1\"\n\
         permissions = [\"models:manage\"]\n[contributes]\n\
         models = [{{ id = \"voice\", name = \"Voice\", size = {}, url = \"file://{}\", sha256 = \"{}\", license = \"MIT\" }}]\n",
        bytes.len(),
        model.display(),
        fc_plugins::package::sha256_hex(&bytes)
    );
    let wasm = fc_plugins::testing::example_wasm("misbehave").unwrap();
    let pkg = dir.path().join("models.fcplugin");
    std::fs::write(&pkg, fc_plugins::package::pack(&manifest, &wasm, &[]).unwrap()).unwrap();
    t.ok::<PluginsInstall>(InstallPlugin {
        path: pkg.display().to_string(),
        grant: vec!["models:manage".into()],
    });
    let list = t.ok::<PluginsModelsList>(Empty {});
    assert_eq!(list.models.len(), 1);
    assert_eq!(list.models[0].state, ModelState::Missing);
    let r = ModelRef {
        plugin_id: "com.example.models".into(),
        id: "voice".into(),
    };
    let job = t.ok::<PluginsModelsDownload>(r.clone()).job;
    t.app.run_pending_jobs().unwrap();
    let done = t.ok::<JobsGet>(fc_api::v1::jobs::JobRef { id: job.id });
    assert_eq!(done.status, fc_api::v1::jobs::JobStatus::Succeeded, "{done:?}");
    let list = t.ok::<PluginsModelsList>(Empty {});
    assert_eq!(list.models[0].state, ModelState::Ready);
    assert_eq!(list.disk_usage, bytes.len() as u64);
    let gone = t.ok::<PluginsModelsDelete>(r.clone());
    assert_eq!(gone.state, ModelState::Missing);
    assert_eq!(
        t.err::<PluginsModelsDownload>(ModelRef {
            plugin_id: "com.example.models".into(),
            id: "nope".into()
        })
        .code,
        ErrorCode::NotFoundPluginContribution
    );
}

#[test]
fn every_plugin_method_is_registered_with_its_kind() {
    let t = TestApp::mem();
    for d in fc_api::v1::catalog() {
        if d.module == ID {
            assert!(t.app.registry().descriptor(d.name).is_some(), "{}", d.name);
        }
    }
    assert_eq!(PluginsRun::NAME, "plugins.run");
}

// ------------------------------------------------------------------ sandbox escapes
//
// `plugins-sdk/examples/escape` is hostile: every command or hook is a way out of the sandbox
// that must be refused. Variants of its manifest ask for what no plugin may hold.

/// The escape plugin packed with `permissions` instead of its own list, without any check
/// on the packing side (as a hostile author would ship it).
fn escape_variant(dir: &tempfile::TempDir, name: &str, permissions: &[&str]) -> String {
    let src = fc_plugins::testing::example_dir("escape");
    let read = |f: &str| std::fs::read(src.join(f)).unwrap();
    let list = permissions.iter().map(|p| format!("\"{p}\"")).collect::<Vec<_>>().join(", ");
    let toml: String = String::from_utf8(read("plugin.toml"))
        .unwrap()
        .lines()
        .map(|l| {
            if l.starts_with("permissions = ") {
                format!("permissions = [{list}]")
            } else {
                l.to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let wasm = fc_plugins::testing::example_wasm("escape").unwrap();
    let (settings, en, tr) = (
        read("settings.schema.json"),
        read("locales/en.json"),
        read("locales/tr.json"),
    );
    let bytes = fc_plugins::testing::pack_unchecked(&[
        ("plugin.toml", toml.as_bytes()),
        ("plugin.wasm", &wasm),
        ("settings.schema.json", &settings),
        ("locales/en.json", &en),
        ("locales/tr.json", &tr),
    ])
    .unwrap();
    let path = dir.path().join(format!("{name}.fcplugin"));
    std::fs::write(&path, bytes).unwrap();
    path.display().to_string()
}

fn install_refused(t: &TestApp, path: String, grant: &[&str]) -> ApiError {
    t.err::<PluginsInstall>(InstallPlugin {
        path,
        grant: grant.iter().map(|g| (*g).to_owned()).collect(),
    })
}

fn wire(code: ErrorCode) -> Value {
    serde_json::to_value(code).unwrap()
}

/// The plugin's call failed on a refused host call with `code`.
fn refused(r: Result<PluginCommandResult, ApiError>, code: ErrorCode) {
    let e = r.unwrap_err();
    assert_eq!(e.code, ErrorCode::ProviderPluginFailed, "{e:?}");
    assert_eq!(e.details["code"], wire(code), "{e:?}");
}

fn grants(t: &TestApp, id: &str) -> Vec<String> {
    plugin(t, id)
        .permissions
        .into_iter()
        .filter(|p| p.granted)
        .map(|p| p.permission)
        .collect()
}

fn kv(t: &TestApp, key: &str) -> Option<Value> {
    t.app.store().read().unwrap().get_plugin_kv(ESCAPE, key).unwrap()
}

/// Review finding 1: a `before:` hook rewrites the *user's* call, so hooks on plugin
/// management, export/restore, settings, modules, undo or batches would act with the user's
/// authority. They are refused at install and never indexed; any other hook needs the hooked
/// method's own permission.
fn hooks_cannot_reach_management_io_or_settings(f: Factory) {
    let t = TestApp::new(f());
    let dir = tempfile::tempdir().unwrap();
    for (i, hook) in [
        "hooks:before:plugins.set_permissions",
        "hooks:before:plugins.install",
        "hooks:after:plugins.set_enabled",
        "hooks:before:io.export.start",
        "hooks:before:io.backup.restore",
        "hooks:before:io.backup.create",
        "hooks:before:settings.set",
        "hooks:before:shortcuts.set",
        "hooks:before:modules.set_enabled",
        "hooks:before:oplog.undo",
        "hooks:before:batch.execute",
        "hooks:before:jobs.cancel",
    ]
    .into_iter()
    .enumerate()
    {
        let e = install_refused(&t, escape_variant(&dir, &format!("h{i}"), &["ui:command", hook]), &[hook]);
        assert_eq!(e.code, ErrorCode::ValidationPluginManifest, "{hook}: {e:?}");
        assert_eq!(e.details["permission"], json!(hook));
    }
    // A hook without the hooked method's permission in the manifest is refused too.
    let e = install_refused(
        &t,
        escape_variant(&dir, "nowrite", &["hooks:before:notes.create"]),
        &["hooks:before:notes.create"],
    );
    assert_eq!(e.code, ErrorCode::ValidationPluginManifest);
    assert!(t.ok::<PluginsList>(Empty {}).plugins.is_empty(), "nothing got installed");

    // Installed, the hook runs only while the method's own permission is granted as well.
    let d = deck(&t, "Hooked");
    install(&t, &dir, "escape", &["hooks:before:notes.create"]);
    assert_eq!(classic(&t, &d, "plain", "x").fields["front"], "plain");
    t.ok::<PluginsSetPermissions>(SetPluginPermissions {
        id: ESCAPE.into(),
        granted: vec!["hooks:before:notes.create".into(), "notes:write".into()],
    });
    assert_eq!(classic(&t, &d, "plain", "x").fields["front"], "hooked");

    // Defense in depth: a stored row that holds a forbidden hook anyway (an old or edited
    // collection) loads, but the hook is never indexed, so the user's revoke stands.
    {
        let mut tx = t.app.store().write().unwrap();
        let mut row = tx.get_plugin(ESCAPE).unwrap().unwrap();
        let tampered = "hooks:before:plugins.set_permissions";
        row.manifest["permissions"]
            .as_array_mut()
            .unwrap()
            .push(json!(tampered));
        row.granted.push(tampered.into());
        tx.upsert_plugin(&row).unwrap();
        tx.commit().unwrap();
    }
    t.app.plugins().reload(&t.app);
    t.ok::<PluginsSetPermissions>(SetPluginPermissions {
        id: ESCAPE.into(),
        granted: vec![],
    });
    assert!(grants(&t, ESCAPE).is_empty(), "the hook did not rewrite the revoke");
}

struct TestPicker {
    open: Option<std::path::PathBuf>,
    save: Option<std::path::PathBuf>,
    suggested: std::sync::Mutex<Vec<String>>,
}

impl FilePicker for TestPicker {
    fn pick_open(&self, _extensions: &[String]) -> Option<std::path::PathBuf> {
        self.open.clone()
    }

    fn pick_save(&self, suggested_name: &str, _extensions: &[String]) -> Option<std::path::PathBuf> {
        self.suggested.lock().unwrap().push(suggested_name.to_owned());
        self.save.clone()
    }
}

/// Review findings 2 and 3: `io:*` (free file paths), `settings:*`, `modules:*`, `plugins:*`,
/// `oplog:*` are never grantable; files come only through `fs:pick` (a name and bytes); a
/// plugin's settings are its own schema, so it cannot loosen its own limits.
fn plugins_get_no_free_paths_or_app_settings(f: Factory) {
    let t = TestApp::new(f());
    let dir = tempfile::tempdir().unwrap();
    for (i, p) in [
        "io:read",
        "io:write",
        "settings:write",
        "modules:write",
        "plugins:write",
        "oplog:write",
        "mcp:write",
        "ai_keys:write",
        "ai:write",
    ]
    .into_iter()
    .enumerate()
    {
        let e = install_refused(&t, escape_variant(&dir, &format!("p{i}"), &["ui:command", p]), &[p]);
        assert_eq!(e.code, ErrorCode::ValidationPluginManifest, "{p}: {e:?}");
    }

    let all = ["ui:command", "fs:pick", "decks:write", "hooks:before:notes.create", "notes:write"];
    install(&t, &dir, "escape", &all);
    for command in ["self-grant", "export", "restore", "raise-limits", "module-off", "undo"] {
        refused(run(&t, ESCAPE, command), ErrorCode::PermissionDenied);
    }
    assert_eq!(grants(&t, ESCAPE).len(), all.len(), "grants unchanged");
    assert!(t.app.is_module_enabled(ID));
    // Even a grant that slipped into the registry does not open a forbidden area.
    let actor = Actor::Plugin(ESCAPE.into());
    let mut tampered: BTreeSet<String> = all.iter().map(|p| (*p).to_owned()).collect();
    tampered.extend(["io:write".to_owned(), "settings:write".to_owned()]);
    t.app.registry().set_plugin_grants(ESCAPE, Some(tampered));
    for (method, params) in [
        ("v1.io.export.start", json!({ "path": "/tmp/x" })),
        ("v1.settings.set", json!({ "scope": { "kind": "app" }, "key": SETTING_FUEL, "value": 1 })),
    ] {
        let e = t.app.invoke(method, params, &actor).unwrap_err();
        assert_eq!(e.code, ErrorCode::PermissionDenied, "{method}");
    }
    t.app.plugins().reload(&t.app);

    // Its own settings: yes. An app key through the same door: no such setting.
    let r = run(&t, ESCAPE, "own-setting").unwrap().result;
    assert_eq!(r["level"], json!(5));
    let s = t.ok::<PluginsSettingsGet>(PluginRef { id: ESCAPE.into() });
    assert_eq!(s.values["level"], json!(5));
    refused(run(&t, ESCAPE, "app-setting-as-own"), ErrorCode::NotFoundSetting);
    let stored = t
        .app
        .store()
        .read()
        .unwrap()
        .get_setting(&SettingScope::App, SETTING_FUEL)
        .unwrap();
    assert_eq!(stored, None, "the sandbox limit was not touched");

    // Files: only through the user's dialog.
    refused(run(&t, ESCAPE, "open"), ErrorCode::ProviderUnavailable);
    let files = tempfile::tempdir().unwrap();
    let secret = files.path().join("notes.txt");
    std::fs::write(&secret, "hello").unwrap();
    let picker = Arc::new(TestPicker {
        open: Some(secret.clone()),
        save: Some(files.path().join("chosen.txt")),
        suggested: std::sync::Mutex::new(vec![]),
    });
    t.app.set_file_picker(picker.clone());
    let opened = run(&t, ESCAPE, "open").unwrap().result;
    assert_eq!(opened["name"], json!("notes.txt"));
    assert_eq!(opened["data_b64"], json!("aGVsbG8="));
    let folder = files.path().display().to_string();
    assert!(!opened.to_string().contains(&folder), "no path reaches the plugin: {opened}");
    let saved = run(&t, ESCAPE, "save").unwrap().result;
    assert_eq!(saved, json!("chosen.txt"));
    assert_eq!(std::fs::read_to_string(files.path().join("chosen.txt")).unwrap(), "hi");
    assert_eq!(
        *picker.suggested.lock().unwrap(),
        vec!["escaped.txt".to_owned()],
        "a suggested name cannot steer the dialog to another folder"
    );
    assert!(!dir.path().join("escaped.txt").exists());
    // Without fs:pick, no dialog.
    t.ok::<PluginsSetPermissions>(SetPluginPermissions {
        id: ESCAPE.into(),
        granted: vec!["ui:command".into()],
    });
    refused(run(&t, ESCAPE, "open"), ErrorCode::PermissionDenied);
}

/// Review finding 4: plugin storage and settings written during a call are part of its undo
/// group: rolled back when the plugin fails, undone with the user data it changed, and no
/// undo step of their own.
fn plugin_storage_rolls_back_with_the_call(f: Factory) {
    let t = TestApp::new(f());
    let dir = tempfile::tempdir().unwrap();
    install(&t, &dir, "escape", &["ui:command", "decks:write", "storage:kv"]);
    let status = t.ok::<OplogStatus>(Empty {});
    assert_eq!(fault_kind(run(&t, ESCAPE, "kv-then-panic")), "TRAP");
    assert_eq!(kv(&t, "secret"), None, "storage written before the crash is gone");
    let s = t.ok::<PluginsSettingsGet>(PluginRef { id: ESCAPE.into() });
    assert_eq!(s.values["level"], json!(1), "so is the setting");
    assert_eq!(t.ok::<OplogStatus>(Empty {}), status, "and no undo step");

    // Storage only: kept, no undo step.
    assert_eq!(run(&t, ESCAPE, "kv-only").unwrap().result, json!(1));
    assert_eq!(run(&t, ESCAPE, "kv-only").unwrap().result, json!(2));
    assert_eq!(t.ok::<OplogStatus>(Empty {}), status);

    // User data + storage: one undo step takes back both.
    let before = t.ok::<DecksList>(Empty {});
    assert_eq!(run(&t, ESCAPE, "write-and-kv").unwrap().result, json!(true));
    assert_eq!(kv(&t, "deck"), Some(json!(true)));
    undo(&t);
    assert_eq!(t.ok::<DecksList>(Empty {}), before);
    assert_eq!(kv(&t, "deck"), None);
    redo(&t);
    assert_eq!(kv(&t, "deck"), Some(json!(true)));
    assert_eq!(kv(&t, "n"), Some(json!(2)));
}

// ------------------------------------------------------------------ security re-review

/// Installs a hand-written WAT plugin `id` (`wat`) declaring `permissions` and `commands`,
/// granting `grant`.
fn install_wat(
    t: &TestApp,
    dir: &tempfile::TempDir,
    id: &str,
    wat: &str,
    permissions: &[&str],
    commands: &[&str],
    grant: &[&str],
) -> PluginDto {
    let list = |xs: &[&str]| xs.iter().map(|p| format!("\"{p}\"")).collect::<Vec<_>>().join(", ");
    let manifest = format!(
        "id = \"{id}\"\nname = \"{id}\"\nversion = \"1.0.0\"\napi_version = \"1\"\n\
         authors = [\"Flashcard\"]\ndescription = \"Test fixture.\"\nmin_app_version = \"0.7.0\"\n\
         permissions = [{}]\n\n[contributes]\ncommands = [{}]\n",
        list(permissions),
        list(commands),
    );
    let wasm = fc_plugins::testing::wat2wasm(wat.as_bytes()).unwrap();
    let bytes = fc_plugins::package::pack(&manifest, &wasm, &[]).unwrap();
    let path = dir.path().join(format!("{id}.fcplugin"));
    std::fs::write(&path, bytes).unwrap();
    t.ok::<PluginsInstall>(InstallPlugin {
        path: path.display().to_string(),
        grant: grant.iter().map(|g| (*g).to_owned()).collect(),
    })
}

fn log_lines(t: &TestApp, id: &str, needle: &str) -> usize {
    t.ok::<PluginsLogs>(PluginRef { id: id.into() })
        .entries
        .iter()
        .filter(|l| l.message.contains(needle))
        .count()
}

/// Finding 1, the exact repro: an `event:deck_deleted` hook creates decks PA and PB, then
/// fails. Its writes are rolled back as the plugin, so neither they nor the rollback feed
/// hooks again: one hook run per user action, deck count constant, every action fast.
/// (Before the fix: 1 → 8 → 39 → 166+ hook runs, over 60 s by round 7.)
fn failed_hook_rollback_does_not_cascade(f: Factory) {
    const ID: &str = "com.test.cascade";
    let t = TestApp::new(f());
    let dir = tempfile::tempdir().unwrap();
    let perms = ["decks:write", "hooks:event:deck_deleted"];
    install_wat(&t, &dir, ID, include_str!("fixtures/cascade.wat"), &perms, &[], &perms);
    let decks = t.ok::<DecksList>(Empty {}).decks.len();
    for round in 1..=8 {
        let start = std::time::Instant::now();
        let d = deck(&t, &format!("X{round}"));
        t.ok::<DecksDelete>(DeckRef { id: d.id });
        assert!(
            start.elapsed() < std::time::Duration::from_secs(10),
            "round {round} took {:?}",
            start.elapsed()
        );
        assert_eq!(t.ok::<DecksList>(Empty {}).decks.len(), decks, "round {round}");
        assert_eq!(log_lines(&t, ID, "boom"), round, "one hook run per action");
    }
    // A domain error is not a fault: the plugin stays on.
    assert_eq!(plugin(&t, ID).status, PluginStatus::Active);
}

/// Hook budget: one user action runs at most `plugins.max_hooks_per_action` hooks; past it
/// the rest are dropped and the plugin is switched off with an i18n notice.
fn hook_budget_per_action(f: Factory) {
    const ID: &str = "com.test.fail";
    let t = TestApp::new(f());
    let dir = tempfile::tempdir().unwrap();
    set_app_setting(&t, SETTING_MAX_HOOKS, json!(2));
    let perms = ["hooks:event:deck_created"];
    install_wat(&t, &dir, ID, include_str!("fixtures/hook_fail.wat"), &perms, &[], &perms);
    // Under the budget: fine.
    deck(&t, "One");
    assert_eq!(log_lines(&t, ID, "boom"), 1);
    t.take_events();
    // One action, four deck_created events: two hooks run, then the budget trips.
    let start = std::time::Instant::now();
    deck(&t, "A::B::C::D");
    assert!(start.elapsed() < std::time::Duration::from_secs(10));
    assert_eq!(log_lines(&t, ID, "boom"), 3);
    assert_eq!(log_lines(&t, ID, "HOOK_BUDGET"), 1);
    let p = plugin(&t, ID);
    assert_eq!(p.status, PluginStatus::AutoDisabled);
    assert!(p.last_error.unwrap_or_default().contains("HOOK_BUDGET"));
    assert!(t.take_events().iter().any(|e| matches!(
        &e.event,
        DomainEvent::PluginsChanged { plugin_id, auto_disabled: true, reason: Some(r) }
            if plugin_id == ID && r.key == "plugins.notice.hook_budget"
    )));
    // Off now: the next action runs no hook.
    deck(&t, "Two");
    assert_eq!(log_lines(&t, ID, "boom"), 3);
}

/// Re-entrancy guard: a hook never starts inside another one, and nothing that happens
/// while a hook runs is queued for `event:` hooks, whatever the actor.
#[test]
fn hooks_do_not_nest_and_their_effects_do_not_queue() {
    let t = TestApp::new(Arc::new(MemStore::new()));
    let host = t.app.plugins();
    let _action = host.begin_action();
    let outer = host.enter_hook(&t.app, "com.test.a", "event:x").expect("first hook runs");
    host.queue_events(&[DomainEvent::CollectionReplaced], &Actor::System);
    assert_eq!(host.queued_events(), 0, "effects of a hook are not queued");
    assert!(host.enter_hook(&t.app, "com.test.b", "event:y").is_none(), "no nesting");
    drop(outer);
    assert!(host.enter_hook(&t.app, "com.test.a", "event:x").is_none(), "action tripped");
    assert_eq!(host.pending_disables(), vec!["com.test.b".to_owned()]);
}

/// Finding 3: `kv.set` needs `storage:kv`, and a plugin's storage is capped (entries and
/// bytes, app settings plugins cannot reach). A refused write rolls back with the call.
fn plugin_storage_needs_permission_and_quota(f: Factory) {
    const ID: &str = "com.test.kvhog";
    let t = TestApp::new(f());
    let dir = tempfile::tempdir().unwrap();
    let wat = include_str!("fixtures/kv_hog.wat");
    install_wat(&t, &dir, ID, wat, &["ui:command", "storage:kv"], &["go"], &["ui:command"]);
    let usage = || t.app.store().read().unwrap().plugin_kv_usage(ID).unwrap();
    // Declared but not granted: refused.
    let e = run(&t, ID, "go").unwrap_err();
    assert_eq!(e.details["code"], json!("PERMISSION_DENIED"));
    assert_eq!(usage(), (0, 0));
    t.ok::<PluginsSetPermissions>(SetPluginPermissions {
        id: ID.into(),
        granted: vec!["ui:command".into(), "storage:kv".into()],
    });
    // 2 × 600 KiB against 1 MiB: the second write is refused, the first rolled back.
    set_app_setting(&t, SETTING_KV_MAX_MB, json!(1));
    let e = run(&t, ID, "go").unwrap_err();
    assert_eq!(e.details["code"], json!("VALIDATION_INVALID_PARAMS"));
    assert!(e.details["message"].as_str().unwrap_or("").contains("quota"), "{e:?}");
    assert_eq!(usage(), (0, 0));
    // Room for both.
    set_app_setting(&t, SETTING_KV_MAX_MB, json!(2));
    run(&t, ID, "go").unwrap();
    let (keys, bytes) = usage();
    assert_eq!(keys, 2);
    assert!(bytes > 1200 * 1024 && bytes < 1300 * 1024, "{bytes}");
    // Entries: a third key is refused at 2; overwriting in place is not growth.
    set_app_setting(&t, SETTING_KV_MAX_KEYS, json!(2));
    run(&t, ID, "go").unwrap();
    let set = |key: &str, restore: bool| {
        let request = InvokeRequest {
            method: format!("{}.{}", fc_api::API_VERSION, PluginsKvSet::NAME),
            params: json!({ "plugin_id": ID, "key": key, "value": 1, "restore": restore }),
        };
        t.app.invoke_trusted(&request, &Actor::Plugin(ID.into()))
    };
    assert_eq!(set("c", false).unwrap_err().code, ErrorCode::ValidationInvalidParams);
    // Putting back an earlier value (undo/rollback) is never refused.
    set("c", true).unwrap();
    assert_eq!(usage().0, 3);
}
