use crate::error::{core_to_api, storage_to_api, AppError};
use crate::events::{EventBus, InMemoryEventBus};
use crate::settings::resolve_setting;
use crate::testing::{MemStore, TestApp};
use crate::{inverse_call, App, AppDeps};
use fc_api::v1::events::{DomainEvent, EventEnvelope};
use fc_api::v1::methods::DecksDelete;
use fc_api::v1::oplog::Actor;
use fc_api::{v1, ErrorCode, MethodKind};
use fc_core::error::{CoreError, NameProblem};
use fc_core::settings::{validate_setting_def, SettingScope};
use fc_core::{DeckId, Timestamp};
use fc_storage::{StorageError, Store};
use serde_json::json;
use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

fn app() -> App {
    let store = Arc::new(MemStore::new());
    let app = App::new(AppDeps::with_store(store)).unwrap();
    app.load_state().unwrap();
    app
}

/// The storage-agnostic service suite against the in-memory store.
mod mem_suite {
    use super::*;
    crate::store_suite!(|| -> Arc<dyn Store> { Arc::new(MemStore::new()) });
}

/// The same suite against the real SQLite store, in memory.
mod sqlite_memory_suite {
    use super::*;
    use fc_storage::SqliteStore;
    crate::store_suite!(|| -> Arc<dyn Store> { Arc::new(SqliteStore::open_in_memory().unwrap()) });
}

/// The same suite against the real SQLite store on a temp file (WAL, reader pool).
mod sqlite_file_suite {
    use super::*;
    use crate::testing::TempSqlite;
    crate::store_suite!(|| -> Arc<dyn Store> { Arc::new(TempSqlite::new()) });
}

#[test]
fn registry_covers_the_catalog_exactly() {
    let app = app();
    let catalog: HashSet<_> = v1::catalog().iter().map(|m| m.name).collect();
    let registered: HashSet<_> = app.registry().methods().map(|m| m.name).collect();
    let missing: Vec<_> = catalog.difference(&registered).collect();
    let extra: Vec<_> = registered.difference(&catalog).collect();
    assert!(
        missing.is_empty(),
        "catalog methods without handler: {missing:?}"
    );
    assert!(
        extra.is_empty(),
        "registered methods not in fc-api catalog: {extra:?}"
    );
}

#[test]
fn modules_are_consistent() {
    let app = app();
    let modules = app.registry().modules();
    let ids: HashSet<_> = modules.iter().map(|m| m.id).collect();
    assert_eq!(ids.len(), modules.len());
    for m in app.registry().methods() {
        assert!(
            ids.contains(m.module),
            "{} → unknown module {}",
            m.name,
            m.module
        );
    }
    let mut ui_ids = HashSet::new();
    for m in modules {
        assert_eq!(m.name_key, format!("modules.{}.name", m.id));
        assert_eq!(m.description_key, format!("modules.{}.description", m.id));
        for ui in &m.ui {
            assert!(ui_ids.insert(ui.id.clone()), "duplicate ui id {}", ui.id);
        }
        for s in &m.settings {
            validate_setting_def(s).unwrap();
            let owner = if m.id == "core" { "app." } else { m.id };
            assert!(
                s.key.starts_with(owner),
                "setting {} must be prefixed by {owner}",
                s.key
            );
        }
        assert!(app.is_module_enabled(m.id) == m.enabled_by_default);
    }
    assert!(app
        .settings_schema()
        .find("decks.include_subdecks")
        .is_some());
}

#[test]
fn invoke_rejects_unknown_and_unversioned_methods() {
    let app = app();
    for name in ["decks.create", "v1.nope.nope", "v2.decks.create"] {
        let err = app.invoke(name, json!({}), &Actor::User).unwrap_err();
        assert_eq!(err.code, ErrorCode::NotFoundMethod, "{name}");
    }
}

#[test]
fn invoke_validates_params_before_touching_storage() {
    let app = app();
    let err = app
        .invoke(
            "v1.decks.create",
            json!({"path": "A", "bogus": 1}),
            &Actor::User,
        )
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::ValidationInvalidParams);
    let err = app
        .invoke("v1.decks.get", json!({"id": "not-a-uuid"}), &Actor::User)
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::ValidationInvalidParams);
}

#[test]
fn no_handler_is_a_stub() {
    let t = TestApp::mem();
    for m in t.app.registry().methods() {
        let err = t
            .app
            .invoke(&m.qualified_name(), json!({}), &Actor::User)
            .err();
        if let Some(e) = err {
            assert_ne!(e.code, ErrorCode::InternalNotImplemented, "{}", m.name);
        }
    }
}

#[test]
fn disabled_module_blocks_its_methods() {
    let app = app();
    app.set_module_enabled_cached("tags", false);
    let err = app
        .invoke("v1.tags.list", json!({}), &Actor::User)
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::ConflictModuleDisabled);
    assert_eq!(err.details["module"], "tags");
}

#[test]
fn command_and_query_kinds_match_catalog() {
    let app = app();
    for m in app.registry().methods() {
        if m.kind == MethodKind::Query {
            assert!(!m.undoable);
        }
    }
}

#[test]
fn core_errors_map_to_documented_codes() {
    let cases = [
        (
            CoreError::InvalidDeckName {
                name: "".into(),
                reason: NameProblem::Empty,
            },
            ErrorCode::ValidationDeckName,
        ),
        (CoreError::DeckCycle, ErrorCode::ConflictDeckCycle),
        (
            CoreError::AttributeRequired {
                key: "level".into(),
            },
            ErrorCode::ValidationAttributeRequired,
        ),
        (
            CoreError::UnknownAttributeOption {
                key: "level".into(),
                option: "x".into(),
            },
            ErrorCode::NotFoundAttributeOption,
        ),
        (
            CoreError::RequiredFieldEmpty {
                field: "front".into(),
            },
            ErrorCode::ValidationFieldRequired,
        ),
    ];
    for (e, code) in cases {
        let api = core_to_api(e);
        assert_eq!(api.code, code);
        assert_eq!(api.message_key, code.message_key());
    }
    let nf = storage_to_api(StorageError::NotFound {
        entity: "deck",
        id: "x".into(),
    });
    assert_eq!(nf.code, ErrorCode::NotFoundDeck);
    assert!(storage_to_api(StorageError::Busy).retryable);
    let via_app: fc_api::ApiError = AppError::from(CoreError::DeckCycle).into();
    assert_eq!(via_app.code, ErrorCode::ConflictDeckCycle);
}

#[test]
fn inverse_calls_are_qualified() {
    let id = DeckId(uuid_from(7));
    let call = inverse_call::<DecksDelete>(&v1::decks::DeckRef { id }).unwrap();
    assert_eq!(call.method, "v1.decks.delete");
    assert_eq!(call.params, json!({"id": id.to_string()}));
}

fn uuid_from(n: u128) -> uuid::Uuid {
    uuid::Uuid::from_u128(n)
}

#[test]
fn event_bus_delivers_until_unsubscribed() {
    let bus = InMemoryEventBus::new();
    let hits = Arc::new(AtomicUsize::new(0));
    let h = hits.clone();
    let id = bus.subscribe(Arc::new(move |_e: &EventEnvelope| {
        h.fetch_add(1, Ordering::SeqCst);
    }));
    let ev = EventEnvelope {
        seq: 0,
        at: Timestamp(1),
        batch_id: None,
        event: DomainEvent::TagsChanged,
    };
    bus.publish(&ev);
    bus.unsubscribe(id);
    bus.publish(&ev);
    assert_eq!(hits.load(Ordering::SeqCst), 1);
    assert_eq!(
        serde_json::to_value(&ev.event).unwrap(),
        json!({"type": "tags_changed"})
    );
}

#[test]
fn settings_resolve_most_specific_first() {
    let app = app();
    let def = app
        .settings_schema()
        .find("decks.include_subdecks")
        .unwrap()
        .clone();
    let deck = SettingScope::Deck(DeckId(uuid_from(1)));
    let (v, src) = resolve_setting(&def, &[(deck.clone(), None), (SettingScope::App, None)]);
    assert_eq!((v, src), (json!(true), None));
    let (v, src) = resolve_setting(
        &def,
        &[
            (deck.clone(), Some(json!(false))),
            (SettingScope::App, Some(json!(true))),
        ],
    );
    assert_eq!((v, src), (json!(false), Some(deck)));
}

// ------------------------------------------------------------------ oplog order & access

/// Probe for nested recording: `tags.set_color` rewired to a composite command that first
/// inserts deck `name` itself (inverse: `decks.delete`), then nests a `decks.update` on it.
fn probe_app() -> TestApp {
    use crate::handler::{CommandCtx, CommandOutcome};
    use crate::registry::Registry;
    use fc_api::v1::methods::{DecksUpdate, TagsSetColor};
    use fc_api::v1::tags::{SetTagColor, TagDto};
    use fc_core::deck::Deck;
    use fc_core::tag::TagName;
    use fc_core::TagId;

    fn composite(
        req: SetTagColor,
        ctx: &mut CommandCtx<'_>,
    ) -> crate::AppResult<CommandOutcome<TagDto>> {
        let id = DeckId::generate(ctx.ids);
        ctx.tx.insert_deck(&Deck {
            id,
            parent_id: None,
            name: req.name.clone(),
            description: String::new(),
            icon: None,
            color: None,
            preset_id: None,
            position: 0,
            created_at: ctx.now,
            updated_at: ctx.now,
            deleted_at: None,
        })?;
        ctx.call(&inverse_call::<DecksUpdate>(&v1::decks::UpdateDeck {
            id,
            description: "nested".into(),
            icon: None,
            color: req.color.clone(),
        })?)?;
        Ok(CommandOutcome {
            response: TagDto {
                id: TagId(uuid_from(1)),
                name: TagName::parse(&req.name)?,
                color: req.color,
                note_count: 0,
            },
            inverse: vec![inverse_call::<DecksDelete>(&v1::decks::DeckRef { id })?],
            summary: v1::common::I18nMessage::key("probe"),
            events: vec![],
        })
    }

    let mut reg = Registry::new();
    crate::modules::core::register(&mut reg);
    crate::modules::decks::register(&mut reg);
    reg.module(crate::modules::tags::descriptor());
    reg.command::<TagsSetColor>(composite);
    let store: Arc<dyn Store> = Arc::new(MemStore::new());
    let mut t = TestApp::new(store.clone());
    let deps = AppDeps {
        store,
        clock: Arc::new(crate::testing::TickClock::new(1_800_000_000_000)),
        ids: Box::new(fc_core::V7IdGen::new(
            Arc::new(crate::testing::TickClock::new(1_800_000_000_000)),
            fc_core::rng::SplitMix64::new(7),
        )),
        events: Arc::new(InMemoryEventBus::new()),
    };
    t.app = App::with_registry(deps, reg).unwrap();
    t
}

fn batch_entries(t: &TestApp) -> Vec<String> {
    let tx = t.store.read().unwrap();
    let batch = tx.list_oplog_batches(None, 1).unwrap()[0].batch_id;
    tx.oplog_batch_entries(batch)
        .unwrap()
        .into_iter()
        .map(|r| r.command)
        .collect()
}

#[test]
fn nested_oplog_entries_follow_their_parent_at_any_depth() {
    use fc_api::v1::methods::{BatchExecute, DecksList, OplogRedo, OplogUndo};
    let t = probe_app();
    let set_color = |name: &str| fc_api::InvokeRequest {
        method: "v1.tags.set_color".into(),
        params: json!({"name": name, "color": "#123456"}),
    };
    let decks = |t: &TestApp| t.ok::<DecksList>(v1::common::Empty {}).decks;
    let empty = v1::common::Empty {};

    // Top level: the parent is seq 0, its nested call seq 1.
    t.app
        .invoke("v1.tags.set_color", set_color("a").params, &Actor::User)
        .unwrap();
    assert_eq!(batch_entries(&t), ["v1.tags.set_color", "v1.decks.update"]);
    // One level down (inside a batch) the parent still reserves its seq first.
    t.ok::<BatchExecute>(v1::oplog::ExecuteBatch {
        calls: vec![set_color("b")],
    });
    assert_eq!(batch_entries(&t), ["v1.tags.set_color", "v1.decks.update"]);
    assert_eq!(decks(&t).len(), 2);

    // Undo runs entries in reverse: nested update reverted first, then the parent's delete.
    t.ok::<OplogUndo>(empty);
    assert_eq!(batch_entries(&t), ["v1.decks.update", "v1.decks.delete"]);
    assert_eq!(decks(&t).len(), 1);
    t.ok::<OplogRedo>(empty);
    let restored = decks(&t);
    assert_eq!(restored.len(), 2);
    assert!(restored.iter().all(|d| d.description == "nested"));
    t.ok::<OplogUndo>(empty);
    t.ok::<OplogUndo>(empty);
    assert!(decks(&t).is_empty());
}

#[test]
fn internal_methods_are_rejected_from_every_transport_path() {
    use fc_api::v1::methods::{BatchExecute, CommandsList, ModulesList};
    use fc_api::Visibility;
    let t = TestApp::mem();
    let internal: Vec<_> = t
        .app
        .registry()
        .methods()
        .filter(|d| d.visibility == Visibility::Internal)
        .copied()
        .collect();
    let names: Vec<_> = internal.iter().map(|d| d.name).collect();
    assert_eq!(
        names,
        [
            "attributes.option.restore",
            "decks.restore_affected",
            "io.backup.load",
            "io.import.apply",
            "io.import.trash",
            "io.import.untrash",
        ]
    );
    for d in &internal {
        assert!(
            d.undoable,
            "{} must record itself inside undo batches",
            d.name
        );
        let params = json!({});
        for actor in [Actor::User, Actor::Cli, Actor::Plugin("p".into())] {
            let e = t
                .app
                .invoke(&d.qualified_name(), params.clone(), &actor)
                .unwrap_err();
            assert_eq!(e.code, ErrorCode::PermissionDenied, "{}", d.name);
        }
        let e = t
            .app
            .call::<BatchExecute>(
                &v1::oplog::ExecuteBatch {
                    calls: vec![fc_api::InvokeRequest {
                        method: d.qualified_name(),
                        params,
                    }],
                },
                &Actor::User,
            )
            .unwrap_err();
        assert_eq!(e.code, ErrorCode::PermissionDenied, "batch: {}", d.name);
    }
    // Menus/method lists drawn from modules.list never offer them; commands.list marks them.
    let listed: Vec<String> = t
        .ok::<ModulesList>(v1::common::Empty {})
        .modules
        .into_iter()
        .flat_map(|m| m.methods)
        .collect();
    for d in &internal {
        assert!(!listed.contains(&d.qualified_name()));
    }
    let all = t.ok::<CommandsList>(v1::common::Empty {}).methods;
    assert!(all
        .iter()
        .filter(|m| m.visibility == Visibility::Internal)
        .all(|m| names.iter().any(|n| m.name.ends_with(n))));
}

#[test]
fn batch_checks_module_switches_as_changed_inside_the_batch() {
    use fc_api::v1::methods::BatchExecute;
    let t = TestApp::mem();
    let call = |m: &str, p: serde_json::Value| fc_api::InvokeRequest {
        method: format!("v1.{m}"),
        params: p,
    };
    let e = t
        .app
        .call::<BatchExecute>(
            &v1::oplog::ExecuteBatch {
                calls: vec![
                    call(
                        "modules.set_enabled",
                        json!({"id": "tags", "enabled": false}),
                    ),
                    call("tags.set_color", json!({"name": "x", "color": null})),
                ],
            },
            &Actor::User,
        )
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::ConflictModuleDisabled);
    assert!(t.app.is_module_enabled("tags"), "the batch rolled back");
}

#[test]
fn bad_cursor_is_a_validation_error() {
    use fc_api::v1::methods::{NotesList, OplogList};
    let t = TestApp::mem();
    let page = fc_core::paging::PageRequest {
        cursor: Some("not-a-cursor".into()),
        limit: None,
    };
    let e = t
        .app
        .call::<NotesList>(
            &v1::notes::ListNotes {
                deck_id: None,
                include_subdecks: None,
                page: page.clone(),
            },
            &Actor::User,
        )
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::ValidationInvalidParams);
    assert_eq!(e.details["field"], "cursor");
    let e = t
        .app
        .call::<OplogList>(&v1::oplog::ListOplog { page }, &Actor::User)
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::ValidationInvalidParams);
}
