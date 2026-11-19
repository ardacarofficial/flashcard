use crate::error::{core_to_api, storage_to_api, AppError};
use crate::events::{EventBus, InMemoryEventBus};
use crate::settings::resolve_setting;
use crate::{inverse_call, App, AppDeps};
use fc_api::v1::events::{DomainEvent, EventEnvelope};
use fc_api::v1::methods::DecksDelete;
use fc_api::v1::oplog::Actor;
use fc_api::{v1, ErrorCode, MethodKind};
use fc_core::error::{CoreError, NameProblem};
use fc_core::settings::{validate_setting_def, SettingScope};
use fc_core::{DeckId, Timestamp};
use fc_storage::{SqliteStore, StorageError};
use serde_json::json;
use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

fn app() -> App {
    let store = Arc::new(SqliteStore::open_in_memory().unwrap());
    App::new(AppDeps::with_store(store)).unwrap()
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
fn unimplemented_paths_report_not_implemented() {
    let app = app();
    let err = app
        .invoke("v1.decks.tree", json!({}), &Actor::Cli)
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::InternalNotImplemented);
    assert!(app.load_state().is_err());
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
