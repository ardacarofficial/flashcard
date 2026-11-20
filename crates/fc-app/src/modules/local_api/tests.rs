//! Service tests of the `local_api` module (three stores): pairing and approval, real
//! `permits()` for `Actor::LocalApi`, the HTTP allow-list, origin binding, idempotent
//! resends, expiry, and backups revoking clients. The HTTP transport itself is tested in
//! `fc-http`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::modules::inbox::tests::english;
use crate::testing::{MemStore, TempSqlite, TestApp};
use fc_api::v1::inbox::{AddInboxItem, InboxSource, ListInbox};
use fc_api::v1::modules::SetModuleEnabled;
use fc_api::v1::notes::CreateNote;
use fc_api::v1::oplog::{Actor, ListOplog};
use fc_api::{ErrorCode, MethodKind, Visibility};
use fc_core::notetype::builtin::CLASSIC_ID;
use fc_storage::{SqliteStore, Store};
use serde_json::json;
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
    pairing_needs_the_users_approval_and_scopes_bind,
    local_api_reaches_only_its_allow_list_with_granted_scopes,
    tokens_are_bound_to_their_origin,
    resends_with_a_client_ref_are_idempotent,
    pending_requests_are_capped_and_expire,
    local_api_writes_are_ordinary_undo_steps,
    restoring_a_backup_revokes_every_client,
    nothing_works_while_the_module_is_off,
);

const EXT: &str = "chrome-extension://abcdefghijklmnop";

fn enable(t: &TestApp) {
    t.ok::<ModulesSetEnabled>(SetModuleEnabled {
        id: ID.into(),
        enabled: true,
    });
}

fn hash(n: u32) -> String {
    format!("{n:064x}")
}

fn pair(t: &TestApp, n: u32, scopes: &[&str], origin: Option<&str>) -> ApiClientDto {
    record_pairing(
        &t.app,
        &PairRequest {
            client_name: format!("Chrome {n}"),
            client_kind: ApiClientKind::BrowserExtension,
            scopes: scopes.iter().map(|s| (*s).to_owned()).collect(),
        },
        origin,
        &hash(n),
    )
    .unwrap()
}

/// An approved client holding `scopes`.
fn approved(t: &TestApp, n: u32, scopes: &[&str]) -> ApiClientDto {
    let c = pair(t, n, scopes, Some(EXT));
    t.ok::<LocalApiClientsApprove>(ApproveApiClient {
        id: c.id,
        scopes: None,
    });
    authenticate(&t.app, &hash(n), Some(EXT)).unwrap()
}

fn item(front: &str) -> AddInboxItem {
    AddInboxItem {
        front: Some(front.into()),
        ..AddInboxItem::default()
    }
}

// ------------------------------------------------------------------ tests

pub(super) fn pairing_needs_the_users_approval_and_scopes_bind(f: Factory) {
    let t = TestApp::new(f());
    enable(&t);
    let (_, verbs) = english(&t);
    t.take_events();
    let c = pair(&t, 1, &["inbox:write", "decks:read", "inbox:write"], Some(EXT));
    assert_eq!(c.status, ApiClientStatus::Pending);
    assert_eq!(c.requested_scopes, ["decks:read", "inbox:write"]);
    assert!(c.scopes.is_empty());
    assert!(t
        .take_events()
        .iter()
        .any(|e| matches!(&e.event, DomainEvent::LocalApiPairing { client } if client.id == c.id)));

    // Until the user decides, the token is useless.
    let e = authenticate(&t.app, &hash(1), Some(EXT)).unwrap_err();
    assert_eq!(e.code, ErrorCode::PermissionUnauthenticated);
    assert_eq!(e.details["reason"], "pending");
    let e = authenticate(&t.app, &hash(99), None).unwrap_err();
    assert_eq!(e.details["reason"], "invalid");

    // Scopes it did not ask for cannot be granted; it gets a subset here.
    let e = t.err::<LocalApiClientsApprove>(ApproveApiClient {
        id: c.id,
        scopes: Some(vec!["notes:write".into()]),
    });
    assert_eq!(e.code, ErrorCode::ValidationApiScope);
    let ok = t.ok::<LocalApiClientsApprove>(ApproveApiClient {
        id: c.id,
        scopes: Some(vec!["inbox:write".into()]),
    });
    assert_eq!(ok.scopes, ["inbox:write"]);
    let client = authenticate(&t.app, &hash(1), Some(EXT)).unwrap();
    let added = add_inbox_item(&t.app, &client, item("porsuk")).unwrap().0;
    assert_eq!(
        added.source,
        InboxSource::LocalApi {
            client_id: c.id,
            client_name: "Chrome 1".into()
        }
    );
    // `decks:read` was asked for but not granted.
    let e = call(&t.app, &client, "decks.list", json!({})).unwrap_err();
    assert_eq!(e.code, ErrorCode::PermissionDenied);
    // Approving twice is a state conflict.
    let e = t.err::<LocalApiClientsApprove>(ApproveApiClient {
        id: c.id,
        scopes: None,
    });
    assert_eq!(e.code, ErrorCode::ConflictApiClientState);

    // Revoking stops the token at once; nothing revives it.
    t.ok::<LocalApiClientsRevoke>(ApiClientRef { id: c.id });
    let e = call(&t.app, &client, "inbox.add", json!({"front": "x"})).unwrap_err();
    assert_eq!(e.code, ErrorCode::PermissionDenied, "grants cleared by revoke");
    let e = authenticate(&t.app, &hash(1), Some(EXT)).unwrap_err();
    assert_eq!(e.details["reason"], "revoked");
    // Pairing, approval and revocation are not undo steps: the newest step is the add.
    let newest = t.ok::<OplogList>(ListOplog::default()).items;
    assert_eq!(newest[0].command, "v1.inbox.add");
    assert_eq!(newest[0].payload["front"], "porsuk");

    // A denied request stays denied.
    let d = pair(&t, 2, &["inbox:write"], None);
    t.ok::<LocalApiClientsDeny>(ApiClientRef { id: d.id });
    assert_eq!(
        authenticate(&t.app, &hash(2), None).unwrap_err().details["reason"],
        "denied"
    );
    // Unknown and non-grantable scopes are refused at pairing time.
    for bad in [&["io:write"][..], &["settings:write"], &["local_api:write"], &[]] {
        let e = record_pairing(
            &t.app,
            &PairRequest {
                client_name: "x".into(),
                client_kind: ApiClientKind::Script,
                scopes: bad.iter().map(|s| (*s).to_owned()).collect(),
            },
            None,
            &hash(50),
        )
        .unwrap_err();
        assert_eq!(e.code, ErrorCode::ValidationApiScope, "{bad:?}");
    }
    // Web pages may not pair.
    let e = record_pairing(
        &t.app,
        &PairRequest {
            client_name: "evil".into(),
            client_kind: ApiClientKind::App,
            scopes: vec!["inbox:write".into()],
        },
        Some("https://evil.example"),
        &hash(51),
    )
    .unwrap_err();
    assert_eq!(e.code, ErrorCode::ValidationApiClient);
    let _ = verbs;
}

pub(super) fn local_api_reaches_only_its_allow_list_with_granted_scopes(f: Factory) {
    let t = TestApp::new(f());
    enable(&t);
    let client = approved(&t, 1, GRANTABLE_SCOPES);
    let actor = actor(&client);
    let registry = t.app.registry();

    // The allow-list: public, scoped within the grantable set, commands undoable+replayable.
    for name in HTTP_METHODS {
        let d = registry.descriptor(name).unwrap();
        assert_eq!(d.visibility, Visibility::Public, "{name}");
        assert!(GRANTABLE_SCOPES.contains(&d.permission), "{name}");
        if d.kind == MethodKind::Command {
            assert!(d.undoable && d.replayable, "{name} must be an undoable command");
        }
    }
    // No grantable scope opens a forbidden area.
    for s in GRANTABLE_SCOPES {
        let area = s.split(':').next().unwrap();
        assert!(
            !["io", "plugins", "mcp", "ai", "ai_keys", "ai_requests", "oplog", "settings",
              "modules", "batch", "jobs", "local_api", "stats"]
                .contains(&area),
            "{s}"
        );
    }
    // Every other public method is refused, even with every scope a client can hold.
    let before = t.ok::<OplogList>(ListOplog::default()).items.len();
    for d in registry.methods() {
        let r = t.app.invoke(&d.qualified_name(), json!({}), &actor);
        if d.visibility == Visibility::Internal || !HTTP_METHODS.contains(&d.name) {
            let e = r.unwrap_err();
            assert_eq!(e.code, ErrorCode::PermissionDenied, "{}", d.name);
        } else if let Err(e) = r {
            assert_ne!(e.code, ErrorCode::PermissionDenied, "{}", d.name);
        }
        // The transport refuses unlisted names before the registry is even asked.
        if !HTTP_METHODS.contains(&d.name) {
            assert_eq!(
                call(&t.app, &client, d.name, json!({})).unwrap_err().code,
                ErrorCode::NotFoundMethod,
                "{}",
                d.name
            );
        }
    }
    assert_eq!(
        t.ok::<OplogList>(ListOplog::default()).items.len(),
        before,
        "nothing was written by refused calls"
    );
    // `notes:write` would cover `notes.delete`, but it is not on the list.
    assert!(registry.permits(&actor, "notes:write"));
    assert!(!method_reachable("notes.delete"));
    // A stored grant outside the grantable set is ignored.
    assert!(!registry.permits(&actor, "io:write"));
    // `local_api.methods` lists exactly the allow-list.
    let m = t.ok::<LocalApiMethods>(Empty {});
    assert_eq!(m.methods.len(), HTTP_METHODS.len());
}

pub(super) fn tokens_are_bound_to_their_origin(f: Factory) {
    let t = TestApp::new(f());
    enable(&t);
    approved(&t, 1, &["inbox:write"]);
    // Same origin, or none (extension GETs carry no Origin): fine.
    assert!(authenticate(&t.app, &hash(1), Some(EXT)).is_ok());
    assert!(authenticate(&t.app, &hash(1), None).is_ok());
    for other in ["https://example.com", "chrome-extension://other", "null"] {
        let e = authenticate(&t.app, &hash(1), Some(other)).unwrap_err();
        assert_eq!(e.code, ErrorCode::PermissionOriginNotAllowed, "{other}");
    }
    // A client paired without a browser cannot be used from one.
    let c = pair(&t, 2, &["inbox:write"], None);
    t.ok::<LocalApiClientsApprove>(ApproveApiClient {
        id: c.id,
        scopes: None,
    });
    assert!(authenticate(&t.app, &hash(2), None).is_ok());
    assert_eq!(
        authenticate(&t.app, &hash(2), Some(EXT)).unwrap_err().code,
        ErrorCode::PermissionOriginNotAllowed
    );
    assert!(is_extension_origin(EXT));
    assert!(!is_extension_origin("chrome-extension://"));
    assert!(!is_extension_origin("http://localhost"));
}

pub(super) fn resends_with_a_client_ref_are_idempotent(f: Factory) {
    let t = TestApp::new(f());
    enable(&t);
    let a = approved(&t, 1, &["inbox:write", "inbox:read"]);
    let b = approved(&t, 2, &["inbox:write"]);
    let mut req = item("porsuk");
    req.client_ref = Some("0193-ref".into());
    let (first, created) = add_inbox_item(&t.app, &a, req.clone()).unwrap();
    assert!(created);
    let steps = t.ok::<OplogList>(ListOplog::default()).items.len();
    let (again, created) = add_inbox_item(&t.app, &a, req.clone()).unwrap();
    assert!(!created);
    assert_eq!(again, first);
    assert_eq!(t.ok::<OplogList>(ListOplog::default()).items.len(), steps);
    // Another client's reference space is its own.
    let (other, created) = add_inbox_item(&t.app, &b, req).unwrap();
    assert!(created);
    assert_ne!(other.id, first.id);
    let list: fc_api::v1::inbox::InboxPage =
        serde_json::from_value(call(&t.app, &a, "inbox.list", json!({})).unwrap()).unwrap();
    assert_eq!(list.items.len(), 2);
    // `via` is the app's word only.
    let e = call(&t.app, &a, "inbox.add", json!({"front": "x", "via": "clipboard"})).unwrap_err();
    assert_eq!(e.details["field"], "via");
    // Unknown fields (e.g. a client-made `source`) are refused.
    let e = call(&t.app, &a, "inbox.add", json!({"front": "x", "source": "evil"})).unwrap_err();
    assert_eq!(e.code, ErrorCode::ValidationInvalidParams);
}

pub(super) fn pending_requests_are_capped_and_expire(f: Factory) {
    let t = TestApp::new(f());
    enable(&t);
    for n in 0..u32::try_from(MAX_PENDING).unwrap() {
        pair(&t, n, &["inbox:write"], None);
    }
    let e = record_pairing(
        &t.app,
        &PairRequest {
            client_name: "one more".into(),
            client_kind: ApiClientKind::Script,
            scopes: vec!["inbox:write".into()],
        },
        None,
        &hash(77),
    )
    .unwrap_err();
    assert_eq!(e.code, ErrorCode::ConflictRateLimited);

    // Age every pending request past the pairing window.
    {
        let mut tx = t.store.write().unwrap();
        for mut c in tx.list_api_clients().unwrap() {
            c.created_at = Timestamp(c.created_at.0 - PAIRING_TTL_MS - 1);
            tx.upsert_api_client(&c).unwrap();
        }
        tx.commit().unwrap();
    }
    assert!(t.ok::<LocalApiClientsList>(Empty {}).clients.is_empty(), "hidden");
    let stale = t.store.read().unwrap().list_api_clients().unwrap()[0].id;
    let e = t.err::<LocalApiClientsApprove>(ApproveApiClient {
        id: stale,
        scopes: None,
    });
    assert_eq!(e.code, ErrorCode::ConflictApiClientState);
    assert_eq!(e.details["status"], "expired");
    assert_eq!(
        authenticate(&t.app, &hash(0), None).unwrap_err().details["reason"],
        "invalid"
    );
    assert_eq!(client(&t.app, stale).unwrap(), None);
    // The next pairing clears them out and is accepted.
    pair(&t, 78, &["inbox:write"], None);
    assert_eq!(t.store.read().unwrap().list_api_clients().unwrap().len(), 1);
}

pub(super) fn local_api_writes_are_ordinary_undo_steps(f: Factory) {
    let t = TestApp::new(f());
    enable(&t);
    let (_, verbs) = english(&t);
    let c = approved(&t, 1, &["notes:write", "inbox:write"]);
    let note = call(
        &t.app,
        &c,
        "notes.create",
        serde_json::to_value(CreateNote {
            notetype_id: CLASSIC_ID,
            deck_id: verbs,
            fields: BTreeMap::from([("front".into(), "porsuk".into())]),
            tags: vec![],
            attributes: vec![],
            allow_duplicate: None,
        })
        .unwrap(),
    )
    .unwrap();
    let newest = &t.ok::<OplogList>(ListOplog::default()).items[0];
    assert_eq!(newest.command, "v1.notes.create");
    assert_eq!(newest.actor, Actor::LocalApi(c.id.to_string()));
    assert_eq!(
        serde_json::to_value(&newest.actor).unwrap(),
        json!({"kind": "local_api", "id": c.id.to_string()})
    );
    t.ok::<OplogUndo>(Empty {});
    let id: fc_core::NoteId = serde_json::from_value(note["id"].clone()).unwrap();
    let live = t.ok::<SearchCardsMethod>(fc_api::v1::search::SearchCards {
        query: String::new(),
        sort: None,
        descending: None,
        notes_only: None,
        offset: None,
        limit: None,
    });
    assert_eq!(live.total, 0, "undo took the note back");
    // `notes.delete` stays out of reach even with `notes:write`.
    let e = t
        .app
        .invoke("v1.notes.delete", json!({"ids": [id]}), &actor(&c))
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::PermissionDenied);
}

pub(super) fn restoring_a_backup_revokes_every_client(f: Factory) {
    let t = TestApp::new(f());
    enable(&t);
    approved(&t, 1, &["inbox:write"]);
    pair(&t, 2, &["inbox:write"], None);
    // MemStore has no dump; the SQLite stores have.
    let Ok(mut dump) = t.store.read().unwrap().dump_collection() else {
        return;
    };
    forget_clients(&mut dump);
    let table = dump.tables.iter().find(|x| x.name == "api_clients").unwrap();
    assert_eq!(table.rows.len(), 2);
    let status = table.columns.iter().position(|c| c == "status").unwrap();
    let scopes = table.columns.iter().position(|c| c == "scopes").unwrap();
    let statuses: Vec<_> = table.rows.iter().map(|r| r[status].clone()).collect();
    assert!(statuses
        .iter()
        .all(|s| !matches!(s, fc_storage::repo::SqlValue::Text(t) if t == "approved")));
    assert!(table
        .rows
        .iter()
        .all(|r| r[scopes] == fc_storage::repo::SqlValue::Text("[]".into())));
}

pub(super) fn nothing_works_while_the_module_is_off(f: Factory) {
    let t = TestApp::new(f());
    assert!(!t.app.is_module_enabled(ID), "off by default");
    let e = record_pairing(
        &t.app,
        &PairRequest {
            client_name: "x".into(),
            client_kind: ApiClientKind::Script,
            scopes: vec!["inbox:write".into()],
        },
        None,
        &hash(1),
    )
    .unwrap_err();
    assert_eq!(e.code, ErrorCode::ConflictModuleDisabled);
    enable(&t);
    approved(&t, 1, &["inbox:write"]);
    t.ok::<ModulesSetEnabled>(SetModuleEnabled {
        id: ID.into(),
        enabled: false,
    });
    assert_eq!(
        authenticate(&t.app, &hash(1), None).unwrap_err().code,
        ErrorCode::ConflictModuleDisabled
    );
    let _ = t.ok::<InboxList>(ListInbox::default());
}
