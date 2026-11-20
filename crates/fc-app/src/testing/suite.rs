//! Storage-agnostic service tests. Every function takes a store factory, so the same suite
//! runs against [`MemStore`](super::MemStore) and any real [`Store`](fc_storage::Store):
//!
//! ```ignore
//! mod sqlite_suite {
//!     fc_app::store_suite!(|| -> std::sync::Arc<dyn fc_storage::Store> {
//!         std::sync::Arc::new(fc_storage::SqliteStore::open_in_memory().unwrap())
//!     });
//! }
//! ```
//!
//! "Exact previous state" is checked with [`snapshot`]: everything observable through the API
//! (decks, tree, notes with cards and values, tags, attributes incl. trash, settings, modules),
//! minus `updated_at` (an undo is a new modification).

use super::{StoreFactory, TestApp};
use fc_api::v1::attributes::*;
use fc_api::v1::common::{AffectedCount, Empty, EntityRef};
use fc_api::v1::decks::*;
use fc_api::v1::events::DomainEvent;
use fc_api::v1::methods::*;
use fc_api::v1::modules::SetModuleEnabled;
use fc_api::v1::notes::*;
use fc_api::v1::oplog::{Actor, ExecuteBatch, ListOplog, OplogKind};
use fc_api::v1::settings::{GetSettings, ResetSetting, SetSetting};
use fc_api::v1::tags::*;
use fc_api::{ApiMethod, ErrorCode, InvokeRequest};
use fc_core::attribute::{AttributeKind, AttributeTarget, AttributeValue};
use fc_core::i18n::Locale;
use fc_core::notetype::builtin;
use fc_core::paging::PageRequest;
use fc_core::settings::SettingScope;
use fc_core::{AttributeOptionId, DeckId, JobId, Timestamp};
use fc_storage::repo::JobRecord;
use serde_json::{json, Value};
use std::collections::BTreeMap;

/// Generates one `#[test]` per suite function, each running against stores from `$factory`
/// (an expression evaluating to `Fn() -> Arc<dyn Store>`).
#[macro_export]
macro_rules! store_suite {
    ($factory:expr) => {
        $crate::store_suite!(@tests $factory;
            bootstrap_seeds_classic_once,
            deck_create_builds_missing_parents,
            deck_create_undo_redo_roundtrip,
            deck_create_under_parent_id,
            deck_rename_move_update_undo,
            deck_delete_cascades_and_undo_restores,
            note_create_generates_classic_card,
            note_create_validates,
            note_update_delete_restore_undo,
            notes_list_filters_and_pages,
            notetypes_list_and_get,
            tags_rename_merge_color_undo,
            attribute_level_acceptance,
            attribute_define_validates,
            attribute_option_merge_and_delete_undo,
            attribute_values_required_and_applicability,
            attribute_delete_restore_undo,
            settings_resolve_set_reset_undo,
            modules_toggle_and_undo,
            undo_redo_stack_semantics,
            batch_is_atomic_and_one_undo_step,
            events_follow_commits,
            jobs_interrupted_and_cancel,
            noop_commands_are_not_undo_steps,
            option_restore_is_exact,
            deck_delete_inverse_carries_ids,
            note_home_deck_drives_note_attributes,
            tag_ids_are_minted_by_the_app,
            undo_redo_chains,
        );
    };
    (@tests $factory:expr; $($name:ident),* $(,)?) => {
        $(
            #[test]
            fn $name() {
                let factory = $factory;
                $crate::testing::suite::$name(&factory);
            }
        )*
    };
}

// ------------------------------------------------------------------ helpers

fn app(f: &StoreFactory) -> TestApp {
    TestApp::new(f())
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

fn fields(front: &str, back: &str) -> BTreeMap<String, String> {
    BTreeMap::from([
        ("front".to_owned(), front.to_owned()),
        ("back".to_owned(), back.to_owned()),
    ])
}

fn new_note(deck_id: DeckId, front: &str) -> CreateNote {
    CreateNote {
        notetype_id: builtin::CLASSIC_ID,
        deck_id,
        fields: fields(front, "arka"),
        tags: vec![],
        attributes: vec![],
        allow_duplicate: None,
    }
}

fn note(t: &TestApp, deck_id: DeckId, front: &str) -> NoteDto {
    t.ok::<NotesCreate>(new_note(deck_id, front))
}

fn undo(t: &TestApp) {
    t.ok::<OplogUndo>(Empty {});
}

fn redo(t: &TestApp) {
    t.ok::<OplogRedo>(Empty {});
}

fn get<M: ApiMethod>(t: &TestApp, req: M::Request) -> Value {
    match t.app.call::<M>(&req, &Actor::User) {
        Ok(v) => serde_json::to_value(v).unwrap(),
        Err(e) => json!({ "error": e.code }),
    }
}

fn strip(v: &mut Value) {
    match v {
        Value::Object(m) => {
            m.remove("updated_at");
            m.values_mut().for_each(strip);
        }
        Value::Array(a) => a.iter_mut().for_each(strip),
        _ => {}
    }
}

/// Everything observable through the API, without `updated_at`.
pub fn snapshot(t: &TestApp) -> Value {
    let all = PageRequest {
        cursor: None,
        limit: Some(1000),
    };
    let mut notes = t
        .ok::<NotesList>(ListNotes {
            deck_id: None,
            include_subdecks: None,
            page: all,
        })
        .items;
    notes.sort_by_key(|n| n.id);
    let details: Vec<Value> = notes
        .iter()
        .map(|n| get::<NotesGet>(t, NoteRef { id: n.id }))
        .collect();
    let card_values: Vec<Value> = notes
        .iter()
        .flat_map(|n| t.ok::<NotesGet>(NoteRef { id: n.id }).cards)
        .map(|c| {
            get::<AttributesValuesGet>(
                t,
                GetAttributeValues {
                    entity: EntityRef::Card(c.id),
                },
            )
        })
        .collect();
    let mut v = json!({
        "decks": get::<DecksList>(t, Empty {}),
        "tree": get::<DecksTree>(t, Empty {}),
        "notes": notes,
        "note_details": details,
        "card_values": card_values,
        "tags": get::<TagsList>(t, Empty {}),
        "attributes": get::<AttributesList>(t, ListAttributes { scope_deck_id: None, include_deleted: Some(true) }),
        "settings": get::<SettingsGet>(t, GetSettings { scope: SettingScope::App, keys: None }),
        "modules": get::<ModulesList>(t, Empty {}),
    });
    strip(&mut v);
    v
}

/// First differing path between two snapshots (readable failure messages).
fn diff(a: &Value, b: &Value, path: &str) -> Option<String> {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            x.keys()
                .chain(y.keys())
                .find_map(|k| match (x.get(k), y.get(k)) {
                    (Some(l), Some(r)) => diff(l, r, &format!("{path}.{k}")),
                    (l, r) => Some(format!("{path}.{k}: {l:?} vs {r:?}")),
                })
        }
        (Value::Array(x), Value::Array(y)) if x.len() == y.len() => x
            .iter()
            .zip(y)
            .enumerate()
            .find_map(|(i, (l, r))| diff(l, r, &format!("{path}[{i}]"))),
        _ if a == b => None,
        _ => Some(format!("{path}: {a} vs {b}")),
    }
}

/// Asserts two snapshots are equal, reporting the first difference.
pub fn assert_same(actual: &Value, expected: &Value, what: &str) {
    if let Some(d) = diff(actual, expected, "") {
        panic!("{what}: {d}");
    }
}

/// Runs `action`, then checks undo restores the previous snapshot and redo the next one.
/// Leaves the action applied.
fn assert_undo_redo(t: &TestApp, action: impl FnOnce(&TestApp)) {
    let before = snapshot(t);
    action(t);
    let after = snapshot(t);
    assert!(
        diff(&before, &after, "").is_some(),
        "action changed nothing"
    );
    undo(t);
    assert_same(
        &snapshot(t),
        &before,
        "undo did not restore the previous state",
    );
    redo(t);
    assert_same(
        &snapshot(t),
        &after,
        "redo did not restore the changed state",
    );
}

fn code(e: fc_api::ApiError) -> ErrorCode {
    e.code
}

// ------------------------------------------------------------------ bootstrap

pub fn bootstrap_seeds_classic_once(f: &StoreFactory) {
    let store = f();
    let t = TestApp::new(store.clone());
    let list = t.ok::<NotetypesList>(Empty {});
    // Classic plus the v0.3 built-ins, each seeded once.
    assert_eq!(list.notetypes.len(), fc_core::content::builtin::ALL.len());
    let classic = list
        .notetypes
        .iter()
        .find(|n| n.id == builtin::CLASSIC_ID)
        .unwrap();
    assert_eq!(classic.id, builtin::CLASSIC_ID);
    assert_eq!(classic.name, "Klasik");
    assert_eq!(classic.fields[0].name, "Ön");
    // Re-opening is idempotent and keeps user state.
    deck(&t, "A");
    let t2 = TestApp::new(store);
    t2.app.load_state().unwrap();
    assert_eq!(
        t2.ok::<NotetypesList>(Empty {}).notetypes.len(),
        fc_core::content::builtin::ALL.len()
    );
    assert_eq!(t2.ok::<DecksList>(Empty {}).decks.len(), 1);
    assert_eq!(t2.app.locale(), Locale::Tr);
    assert!(t2.ok::<OplogList>(ListOplog::default()).items.len() == 1);
}

// ------------------------------------------------------------------ decks

pub fn deck_create_builds_missing_parents(f: &StoreFactory) {
    let t = app(f);
    let leaf = deck(&t, " Dil :: İngilizce ::Fiiller");
    assert_eq!(leaf.path, "Dil::İngilizce::Fiiller");
    assert_eq!(leaf.name, "Fiiller");
    let created: Vec<_> = t
        .take_events()
        .into_iter()
        .filter(|e| matches!(e.event, DomainEvent::DeckCreated { .. }))
        .collect();
    assert_eq!(created.len(), 3);
    let sibling = deck(&t, "dil::Almanca");
    assert_eq!(
        sibling.path, "Dil::Almanca",
        "existing parent reused (case-insensitive)"
    );
    let e = t.err::<DecksCreate>(CreateDeck {
        path: "DIL::İngilizce::FIILLER".into(),
        parent_id: None,
        description: None,
        icon: None,
        color: None,
    });
    assert_eq!(e.code, ErrorCode::ConflictDeckExists);
    let e = t.err::<DecksCreate>(CreateDeck {
        path: "A::::B".into(),
        parent_id: None,
        description: None,
        icon: None,
        color: None,
    });
    assert_eq!(e.code, ErrorCode::ValidationDeckName);

    let paths: Vec<String> = t
        .ok::<DecksList>(Empty {})
        .decks
        .into_iter()
        .map(|d| d.path)
        .collect();
    assert_eq!(
        paths,
        [
            "Dil",
            "Dil::Almanca",
            "Dil::İngilizce",
            "Dil::İngilizce::Fiiller"
        ]
    );
    let tree = t.ok::<DecksTree>(Empty {});
    assert_eq!(tree.roots.len(), 1);
    let kids: Vec<_> = tree.roots[0]
        .children
        .iter()
        .map(|c| c.deck.name.as_str())
        .collect();
    assert_eq!(kids, ["İngilizce", "Almanca"], "children by position");
    let got = t.ok::<DecksGet>(DeckRef { id: leaf.id });
    assert_eq!(got, leaf);
}

/// `decks.create` with `parent_id`: the path is relative to the parent and Rust builds the
/// full path (clients never join paths).
pub fn deck_create_under_parent_id(f: &StoreFactory) {
    let t = app(f);
    let dil = deck(&t, "Dil");
    let req = |path: &str, parent_id| CreateDeck {
        path: path.into(),
        parent_id,
        description: None,
        icon: None,
        color: None,
    };
    let child = t.ok::<DecksCreate>(req("İngilizce", Some(dil.id)));
    assert_eq!(child.path, "Dil::İngilizce");
    assert_eq!(child.parent_id, Some(dil.id));
    let deep = t.ok::<DecksCreate>(req("Fiiller::Düzensiz", Some(child.id)));
    assert_eq!(deep.path, "Dil::İngilizce::Fiiller::Düzensiz");
    let e = t.err::<DecksCreate>(req("fiiller", Some(child.id)));
    assert_eq!(e.code, ErrorCode::ConflictDeckExists);
    assert_eq!(e.details["path"], "Dil::İngilizce::fiiller");
    let e = t.err::<DecksCreate>(req("X", Some(DeckId(uuid::Uuid::from_u128(1)))));
    assert_eq!(e.code, ErrorCode::NotFoundDeck);
    t.ok::<DecksDelete>(DeckRef { id: child.id });
    let e = t.err::<DecksCreate>(req("X", Some(child.id)));
    assert_eq!(e.code, ErrorCode::ConflictDeleted);
}

pub fn deck_create_undo_redo_roundtrip(f: &StoreFactory) {
    let t = app(f);
    deck(&t, "Dil");
    assert_undo_redo(&t, |t| {
        let d = t.ok::<DecksCreate>(CreateDeck {
            path: "Dil::İngilizce::Fiiller".into(),
            parent_id: None,
            description: Some("açıklama".into()),
            icon: Some("📘".into()),
            color: Some("#ff0000".into()),
        });
        assert_eq!(d.description, "açıklama");
    });
    assert_eq!(t.ok::<DecksList>(Empty {}).decks.len(), 3);
}

pub fn deck_rename_move_update_undo(f: &StoreFactory) {
    let t = app(f);
    let a = deck(&t, "A");
    let b = deck(&t, "B");
    let a1 = deck(&t, "A::Child");
    deck(&t, "B::Child");

    let e = t.err::<DecksRename>(RenameDeck {
        id: a.id,
        name: "b".into(),
    });
    assert_eq!(e.code, ErrorCode::ConflictDeckExists);
    assert_undo_redo(&t, |t| {
        let r = t.ok::<DecksRename>(RenameDeck {
            id: a.id,
            name: " Alfa ".into(),
        });
        assert_eq!(r.path, "Alfa");
    });
    assert_eq!(t.ok::<DecksGet>(DeckRef { id: a1.id }).path, "Alfa::Child");

    let e = t.err::<DecksMove>(MoveDeck {
        id: a.id,
        parent_id: Some(a1.id),
        position: None,
    });
    assert_eq!(e.code, ErrorCode::ConflictDeckCycle);
    let e = t.err::<DecksMove>(MoveDeck {
        id: a1.id,
        parent_id: Some(b.id),
        position: None,
    });
    assert_eq!(
        e.code,
        ErrorCode::ConflictDeckExists,
        "B already has a Child"
    );
    assert_undo_redo(&t, |t| {
        let m = t.ok::<DecksMove>(MoveDeck {
            id: b.id,
            parent_id: Some(a.id),
            position: Some(-5),
        });
        assert_eq!(m.path, "Alfa::B");
        assert_eq!(m.position, -5);
    });
    let tree = t.ok::<DecksTree>(Empty {});
    assert_eq!(tree.roots.len(), 1);
    assert_eq!(
        tree.roots[0].children[0].deck.name, "B",
        "position -5 sorts first"
    );

    assert_undo_redo(&t, |t| {
        t.ok::<DecksUpdate>(UpdateDeck {
            id: a.id,
            description: "yeni".into(),
            icon: Some("x".into()),
            color: None,
        });
    });
}

pub fn deck_delete_cascades_and_undo_restores(f: &StoreFactory) {
    let t = app(f);
    let root = deck(&t, "Dil");
    let sub = deck(&t, "Dil::Fiiller");
    let other = deck(&t, "Matematik");
    note(&t, root.id, "a");
    note(&t, sub.id, "b");
    let keep = note(&t, other.id, "c");
    // A sub-deck trashed earlier must stay trashed when the parent is restored.
    let old = deck(&t, "Dil::Eski");
    t.ok::<DecksDelete>(DeckRef { id: old.id });

    let before = snapshot(&t);
    let affected = t.ok::<DecksDelete>(DeckRef { id: root.id });
    assert_eq!(
        affected,
        AffectedCount {
            decks: 2,
            notes: 2,
            cards: 2
        }
    );
    let after = snapshot(&t);
    let listed = t.ok::<NotesList>(ListNotes {
        deck_id: None,
        include_subdecks: None,
        page: PageRequest::default(),
    });
    assert_eq!(listed.items.len(), 1);
    assert_eq!(listed.items[0].id, keep.id);
    assert_eq!(
        t.err::<DecksRename>(RenameDeck {
            id: sub.id,
            name: "x".into()
        })
        .code,
        ErrorCode::ConflictDeleted
    );
    undo(&t);
    assert_eq!(snapshot(&t), before);
    redo(&t);
    assert_eq!(snapshot(&t), after);
    undo(&t);

    // Restoring explicitly: the earlier-trashed sub-deck stays in the trash.
    let restored = t.ok::<DecksRestore>(DeckRef { id: old.id });
    assert_eq!(restored.decks, 1);
    assert_eq!(t.ok::<DecksList>(Empty {}).decks.len(), 4);
}

// ------------------------------------------------------------------ notes

pub fn note_create_generates_classic_card(f: &StoreFactory) {
    let t = app(f);
    let d = deck(&t, "Dil");
    let n = t.ok::<NotesCreate>(CreateNote {
        tags: vec![
            " #fiil ".into(),
            "dil::düzensiz  fiil".into(),
            "FIIL".into(),
        ],
        ..new_note(d.id, "<b>run</b>")
    });
    assert_eq!(n.cards.len(), 1);
    assert_eq!(n.cards[0].template_key, builtin::FORWARD);
    assert_eq!(n.cards[0].deck_id, d.id);
    assert_eq!(n.sort_field, "run");
    let tags: Vec<&str> = n.tags.iter().map(|t| t.as_str()).collect();
    assert_eq!(tags, ["fiil", "dil::düzensiz_fiil"]);
    assert_eq!(t.ok::<NotesGet>(NoteRef { id: n.id }), n);
    let tree = t.ok::<DecksTree>(Empty {});
    assert_eq!(tree.roots[0].counts.new, 1);
    assert_eq!(tree.roots[0].counts.total, 1);
    assert!(t.take_events().iter().any(|e| e.event
        == DomainEvent::NoteCreated {
            note_id: n.id,
            deck_id: d.id
        }));
    assert_undo_redo(&t, |t| {
        note(t, d.id, "walk");
    });
}

pub fn note_create_validates(f: &StoreFactory) {
    let t = app(f);
    let d = deck(&t, "Dil");
    note(&t, d.id, "dog");
    let e = t.err::<NotesCreate>(new_note(d.id, " <br> "));
    assert_eq!(e.code, ErrorCode::ValidationFieldRequired);
    let mut bad = new_note(d.id, "x");
    bad.fields.insert("nope".into(), "y".into());
    assert_eq!(
        code(t.err::<NotesCreate>(bad)),
        ErrorCode::ValidationUnknownField
    );
    let e = t.err::<NotesCreate>(new_note(d.id, "<i>DOG</i>"));
    assert_eq!(e.code, ErrorCode::ConflictNoteDuplicate);
    t.ok::<NotesCreate>(CreateNote {
        allow_duplicate: Some(true),
        ..new_note(d.id, "dog")
    });
    let e = t.err::<NotesCreate>(CreateNote {
        notetype_id: fc_core::NoteTypeId(uuid::Uuid::from_u128(9)),
        ..new_note(d.id, "z")
    });
    assert_eq!(e.code, ErrorCode::NotFoundNoteType);
    let e = t.err::<NotesCreate>(CreateNote {
        tags: vec!["a::::b".into()],
        ..new_note(d.id, "z")
    });
    assert_eq!(e.code, ErrorCode::ValidationTagName);
    t.ok::<DecksDelete>(DeckRef { id: d.id });
    assert_eq!(
        code(t.err::<NotesCreate>(new_note(d.id, "z"))),
        ErrorCode::ConflictDeleted
    );
    let e = t.err::<NotesCreate>(new_note(DeckId(uuid::Uuid::from_u128(1)), "z"));
    assert_eq!(e.code, ErrorCode::NotFoundDeck);
}

pub fn note_update_delete_restore_undo(f: &StoreFactory) {
    let t = app(f);
    let d = deck(&t, "Dil");
    let n = t.ok::<NotesCreate>(CreateNote {
        tags: vec!["eski".into()],
        ..new_note(d.id, "run")
    });
    note(&t, d.id, "other");
    assert_undo_redo(&t, |t| {
        let u = t.ok::<NotesUpdate>(UpdateNote {
            id: n.id,
            fields: fields("ran", "koştu"),
            tags: vec!["yeni".into()],
            attributes: vec![],
            deck_id: None,
        });
        assert_eq!(u.sort_field, "ran");
        assert_eq!(u.tags[0].as_str(), "yeni");
    });
    assert_undo_redo(&t, |t| {
        let a = t.ok::<NotesDelete>(NoteIds {
            ids: vec![n.id, n.id],
        });
        assert_eq!(a.notes, 1);
        assert_eq!(a.cards, 1);
    });
    let e = t.err::<NotesUpdate>(UpdateNote {
        id: n.id,
        fields: fields("x", ""),
        tags: vec![],
        attributes: vec![],
        deck_id: None,
    });
    assert_eq!(e.code, ErrorCode::ConflictDeleted);
    assert_undo_redo(&t, |t| {
        let a = t.ok::<NotesRestore>(NoteIds { ids: vec![n.id] });
        assert_eq!(a.notes, 1);
    });
    // Deleting a trashed note is a no-op and not an undo step.
    t.ok::<NotesDelete>(NoteIds { ids: vec![n.id] });
    let status = t.ok::<OplogStatus>(Empty {});
    let again = t.ok::<NotesDelete>(NoteIds { ids: vec![n.id] });
    assert_eq!(again.notes, 0);
    assert_eq!(t.ok::<OplogStatus>(Empty {}), status);
}

pub fn notes_list_filters_and_pages(f: &StoreFactory) {
    let t = app(f);
    let parent = deck(&t, "P");
    let child = deck(&t, "P::C");
    let other = deck(&t, "O");
    note(&t, parent.id, "p1");
    note(&t, child.id, "c1");
    note(&t, other.id, "o1");
    let list = |deck_id, sub| {
        t.ok::<NotesList>(ListNotes {
            deck_id: Some(deck_id),
            include_subdecks: sub,
            page: PageRequest::default(),
        })
        .items
        .into_iter()
        .map(|i| i.sort_field)
        .collect::<Vec<_>>()
    };
    assert_eq!(
        list(parent.id, None),
        ["c1", "p1"],
        "newest first, sub-decks included"
    );
    assert_eq!(list(parent.id, Some(false)), ["p1"]);
    assert_eq!(list(child.id, None), ["c1"]);
    let page = t.ok::<NotesList>(ListNotes {
        deck_id: None,
        include_subdecks: None,
        page: PageRequest {
            cursor: None,
            limit: Some(2),
        },
    });
    assert_eq!(page.items.len(), 2);
    let rest = t.ok::<NotesList>(ListNotes {
        deck_id: None,
        include_subdecks: None,
        page: PageRequest {
            cursor: page.next_cursor.clone(),
            limit: Some(2),
        },
    });
    assert_eq!(rest.items.len(), 1);
    assert!(rest.next_cursor.is_none());
    assert_eq!(rest.items[0].card_count, 1);
    let e = t.err::<NotesList>(ListNotes {
        deck_id: Some(DeckId(uuid::Uuid::from_u128(5))),
        include_subdecks: None,
        page: PageRequest::default(),
    });
    assert_eq!(e.code, ErrorCode::NotFoundDeck);
}

pub fn notetypes_list_and_get(f: &StoreFactory) {
    let t = app(f);
    let nt = t.ok::<NotetypesGet>(fc_api::v1::notetypes::NoteTypeRef {
        id: builtin::CLASSIC_ID,
    });
    assert_eq!(nt.builtin.as_deref(), Some(builtin::CLASSIC));
    assert_eq!(nt.templates.len(), 1);
    let e = t.err::<NotetypesGet>(fc_api::v1::notetypes::NoteTypeRef {
        id: fc_core::NoteTypeId(uuid::Uuid::from_u128(3)),
    });
    assert_eq!(e.code, ErrorCode::NotFoundNoteType);
}

// ------------------------------------------------------------------ tags

pub fn tags_rename_merge_color_undo(f: &StoreFactory) {
    let t = app(f);
    let d = deck(&t, "Dil");
    let tagged = |front: &str, tags: &[&str]| {
        t.ok::<NotesCreate>(CreateNote {
            tags: tags.iter().map(|s| (*s).to_owned()).collect(),
            ..new_note(d.id, front)
        })
    };
    tagged("a", &["fiil::düzensiz", "x"]);
    tagged("b", &["fiil", "y"]);
    tagged("c", &["isim"]);
    let names = |t: &TestApp| -> Vec<String> {
        t.ok::<TagsList>(Empty {})
            .tags
            .into_iter()
            .map(|t| format!("{}:{}", t.name, t.note_count))
            .collect()
    };
    assert_eq!(
        names(&t),
        ["fiil:1", "fiil::düzensiz:1", "isim:1", "x:1", "y:1"]
    );
    assert_undo_redo(&t, |t| {
        let r = t.ok::<TagsSetColor>(SetTagColor {
            name: "FIIL".into(),
            color: Some("red".into()),
        });
        assert_eq!(r.note_count, 1);
    });
    assert_undo_redo(&t, |t| {
        let r = t.ok::<TagsRename>(RenameTag {
            from: "fiil".into(),
            to: "verb".into(),
        });
        assert_eq!(r.notes_affected, 2);
        assert_eq!(
            names(t),
            ["isim:1", "verb:1", "verb::düzensiz:1", "x:1", "y:1"]
        );
        let color = t
            .ok::<TagsList>(Empty {})
            .tags
            .into_iter()
            .find(|t| t.name.as_str() == "verb")
            .and_then(|t| t.color);
        assert_eq!(color.as_deref(), Some("red"), "color follows the rename");
    });
    assert_undo_redo(&t, |t| {
        let r = t.ok::<TagsMerge>(MergeTags {
            sources: vec!["x".into(), "y".into()],
            into: "isim".into(),
        });
        assert_eq!(r.notes_affected, 2);
        assert_eq!(names(t), ["isim:3", "verb:1", "verb::düzensiz:1"]);
    });
    let e = t.err::<TagsRename>(RenameTag {
        from: "yok".into(),
        to: "z".into(),
    });
    assert_eq!(e.code, ErrorCode::NotFoundTag);
    let e = t.err::<TagsSetColor>(SetTagColor {
        name: "yok".into(),
        color: None,
    });
    assert_eq!(e.code, ErrorCode::NotFoundTag);
}

// ------------------------------------------------------------------ attributes

fn option_id(def: &AttributeDefDto, label: &str) -> AttributeOptionId {
    def.options
        .iter()
        .find(|o| o.label == label)
        .unwrap_or_else(|| panic!("no option {label}"))
        .id
}

fn define(
    t: &TestApp,
    scope: Option<DeckId>,
    name: &str,
    kind: AttributeKind,
    opts: &[&str],
) -> AttributeDefDto {
    t.ok::<AttributesDefine>(DefineAttribute {
        scope_deck_id: scope,
        key: None,
        name: name.into(),
        kind,
        target: None,
        required: None,
        options: opts
            .iter()
            .map(|l| NewAttributeOption {
                label: (*l).into(),
                color: None,
            })
            .collect(),
    })
}

fn values(t: &TestApp, entity: EntityRef) -> Vec<AttributeAssignment> {
    t.ok::<AttributesValuesGet>(GetAttributeValues { entity })
        .values
}

/// v0.1 acceptance: a "Level" (A1–C2) classification on a deck is selectable when adding a
/// note in a sub-deck; renaming/reordering options is undoable.
pub fn attribute_level_acceptance(f: &StoreFactory) {
    let t = app(f);
    let en = deck(&t, "İngilizce");
    let verbs = deck(&t, "İngilizce::Fiiller");
    let other = deck(&t, "Matematik");
    let level = t.ok::<AttributesDefineFromTemplate>(DefineAttributeFromTemplate {
        template_id: "cefr".into(),
        scope_deck_id: Some(en.id),
        locale: None,
    });
    assert_eq!(level.key, "level");
    assert_eq!(level.kind, AttributeKind::OrderedEnum);
    let labels: Vec<&str> = level.options.iter().map(|o| o.label.as_str()).collect();
    assert_eq!(labels, ["A1", "A2", "B1", "B2", "C1", "C2"]);
    assert_eq!(level.template_id.as_deref(), Some("cefr"));

    let eff = t.ok::<AttributesEffective>(EffectiveAttributes {
        deck_id: Some(verbs.id),
        target: AttributeTarget::Note,
    });
    assert_eq!(eff.attributes.len(), 1);
    assert!(eff.attributes[0].inherited);
    assert_eq!(eff.attributes[0].def.id, level.id);
    let own = t.ok::<AttributesEffective>(EffectiveAttributes {
        deck_id: Some(en.id),
        target: AttributeTarget::Note,
    });
    assert!(!own.attributes[0].inherited);
    assert!(t
        .ok::<AttributesEffective>(EffectiveAttributes {
            deck_id: Some(other.id),
            target: AttributeTarget::Note,
        })
        .attributes
        .is_empty());

    let b1 = option_id(&level, "B1");
    let n = t.ok::<NotesCreate>(CreateNote {
        attributes: vec![AttributeAssignment {
            attr_id: level.id,
            value: Some(AttributeValue::Choice(b1)),
        }],
        ..new_note(verbs.id, "run")
    });
    assert_eq!(n.attributes[0].value, Some(AttributeValue::Choice(b1)));
    // Not applicable outside the deck group.
    let e = t.err::<NotesCreate>(CreateNote {
        attributes: vec![AttributeAssignment {
            attr_id: level.id,
            value: Some(AttributeValue::Choice(b1)),
        }],
        ..new_note(other.id, "x")
    });
    assert_eq!(e.code, ErrorCode::ValidationAttributeNotApplicable);

    assert_undo_redo(&t, |t| {
        let d = t.ok::<AttributesOptionRename>(RenameAttributeOption {
            attr_id: level.id,
            option_id: b1,
            label: "Orta (B1)".into(),
        });
        assert_eq!(d.options[2].label, "Orta (B1)");
    });
    let mut order: Vec<_> = level.options.iter().map(|o| o.id).collect();
    order.reverse();
    assert_undo_redo(&t, |t| {
        let d = t.ok::<AttributesOptionReorder>(ReorderAttributeOptions {
            attr_id: level.id,
            order: order.clone(),
        });
        assert_eq!(d.options[0].label, "C2");
    });
    assert_eq!(
        values(&t, EntityRef::Note(n.id)),
        vec![AttributeAssignment {
            attr_id: level.id,
            value: Some(AttributeValue::Choice(b1))
        }],
        "ids are stable across rename/reorder"
    );
    // remember_last: suggest the value of the latest note in the deck.
    t.ok::<SettingsSet>(SetSetting {
        scope: SettingScope::Deck(verbs.id),
        key: "attributes.remember_last".into(),
        value: json!(true),
    });
    let eff = t.ok::<AttributesEffective>(EffectiveAttributes {
        deck_id: Some(verbs.id),
        target: AttributeTarget::Note,
    });
    assert_eq!(
        eff.attributes[0].suggested_value,
        Some(AttributeValue::Choice(b1))
    );
}

pub fn attribute_define_validates(f: &StoreFactory) {
    let t = app(f);
    let d = deck(&t, "D");
    let exam = define(
        &t,
        None,
        "  Sınav  Türü ",
        AttributeKind::Enum,
        &["TYT", "AYT"],
    );
    assert_eq!(exam.key, "sinav_turu");
    assert_eq!(exam.name, "Sınav Türü");
    assert_eq!(exam.position, 0);
    let e = t.err::<AttributesDefine>(DefineAttribute {
        scope_deck_id: None,
        key: Some("sinav_turu".into()),
        name: "Başka".into(),
        kind: AttributeKind::Text,
        target: None,
        required: None,
        options: vec![],
    });
    assert_eq!(e.code, ErrorCode::ConflictAttributeKeyExists);
    // Same key on a deck is allowed (it shadows the collection one there).
    let shadow = define(&t, Some(d.id), "Sınav Türü", AttributeKind::Enum, &["LGS"]);
    let eff = t.ok::<AttributesEffective>(EffectiveAttributes {
        deck_id: Some(d.id),
        target: AttributeTarget::Note,
    });
    assert_eq!(eff.attributes.len(), 1);
    assert_eq!(eff.attributes[0].def.id, shadow.id);
    let e = t.err::<AttributesDefine>(DefineAttribute {
        scope_deck_id: None,
        key: None,
        name: "Sayfa".into(),
        kind: AttributeKind::Number,
        target: None,
        required: None,
        options: vec![NewAttributeOption {
            label: "x".into(),
            color: None,
        }],
    });
    assert_eq!(e.code, ErrorCode::ValidationAttributeDef);
    let e = t.err::<AttributesDefine>(DefineAttribute {
        scope_deck_id: None,
        key: None,
        name: "Boş".into(),
        kind: AttributeKind::Enum,
        target: None,
        required: None,
        options: vec![],
    });
    assert_eq!(e.code, ErrorCode::ValidationAttributeDef);
    let e = t.err::<AttributesDefineFromTemplate>(DefineAttributeFromTemplate {
        template_id: "nope".into(),
        scope_deck_id: None,
        locale: None,
    });
    assert_eq!(e.code, ErrorCode::NotFoundAttributeTemplate);
    let tpl = t.ok::<AttributesTemplates>(ListAttributeTemplates {
        locale: Some(Locale::En),
    });
    assert!(tpl.templates.iter().any(|t| t.id == "cefr"));
    assert!(tpl.templates.iter().any(|t| t.id == "yks"));
    let yks = t.ok::<AttributesDefineFromTemplate>(DefineAttributeFromTemplate {
        template_id: "yks".into(),
        scope_deck_id: None,
        locale: Some(Locale::Tr),
    });
    assert_eq!(yks.position, 1);
    assert_undo_redo(&t, |t| {
        let u = t.ok::<AttributesUpdate>(UpdateAttribute {
            id: exam.id,
            name: "Sınav".into(),
            required: true,
            default_value: Some(AttributeValue::Choice(option_id(&exam, "AYT"))),
            position: 7,
        });
        assert!(u.required);
    });
    let e = t.err::<AttributesUpdate>(UpdateAttribute {
        id: exam.id,
        name: "Sınav".into(),
        required: false,
        default_value: Some(AttributeValue::Number(1.0)),
        position: 0,
    });
    assert_eq!(e.code, ErrorCode::ValidationAttributeValue);
    assert_undo_redo(&t, |t| {
        t.ok::<AttributesOptionAdd>(AddAttributeOption {
            attr_id: exam.id,
            label: "YDT".into(),
            color: Some("blue".into()),
            after: Some(option_id(&exam, "TYT")),
            id: None,
        });
    });
    assert_undo_redo(&t, |t| {
        t.ok::<AttributesOptionSetColor>(SetAttributeOptionColor {
            attr_id: exam.id,
            option_id: option_id(&exam, "TYT"),
            color: Some("#00ff00".into()),
        });
    });
}

pub fn attribute_option_merge_and_delete_undo(f: &StoreFactory) {
    let t = app(f);
    let d = deck(&t, "D");
    let lvl = define(
        &t,
        Some(d.id),
        "Seviye",
        AttributeKind::OrderedEnum,
        &["A1", "A2", "B1"],
    );
    let topics = define(
        &t,
        Some(d.id),
        "Konu",
        AttributeKind::MultiEnum,
        &["x", "y", "z"],
    );
    let (a1, a2, b1) = (
        option_id(&lvl, "A1"),
        option_id(&lvl, "A2"),
        option_id(&lvl, "B1"),
    );
    let (x, y) = (option_id(&topics, "x"), option_id(&topics, "y"));
    t.ok::<AttributesUpdate>(UpdateAttribute {
        id: lvl.id,
        name: "Seviye".into(),
        required: false,
        default_value: Some(AttributeValue::Choice(a1)),
        position: 0,
    });
    let mk = |front: &str, l: AttributeOptionId, ts: Vec<AttributeOptionId>| {
        t.ok::<NotesCreate>(CreateNote {
            attributes: vec![
                AttributeAssignment {
                    attr_id: lvl.id,
                    value: Some(AttributeValue::Choice(l)),
                },
                AttributeAssignment {
                    attr_id: topics.id,
                    value: Some(AttributeValue::Choices(ts)),
                },
            ],
            ..new_note(d.id, front)
        })
    };
    let n1 = mk("one", a1, vec![x, y]);
    let n2 = mk("two", a2, vec![x]);
    let n3 = mk("three", b1, vec![y]);
    // A trashed note keeps its values and gets them back on undo too.
    t.ok::<NotesDelete>(NoteIds { ids: vec![n3.id] });

    assert_undo_redo(&t, |t| {
        let r = t.ok::<AttributesOptionMerge>(MergeAttributeOptions {
            attr_id: lvl.id,
            from: a1,
            into: a2,
        });
        assert_eq!(r.values_affected, 1);
        let def = t.ok::<AttributesGet>(AttributeRef { id: lvl.id });
        assert_eq!(def.options.len(), 2);
        assert_eq!(def.default_value, Some(AttributeValue::Choice(a2)));
        assert_eq!(
            values(t, EntityRef::Note(n1.id))
                .into_iter()
                .find(|v| v.attr_id == lvl.id)
                .unwrap()
                .value,
            Some(AttributeValue::Choice(a2))
        );
    });
    undo(&t);
    assert_undo_redo(&t, |t| {
        let r = t.ok::<AttributesOptionMerge>(MergeAttributeOptions {
            attr_id: topics.id,
            from: x,
            into: y,
        });
        assert_eq!(r.values_affected, 2);
        let v = values(t, EntityRef::Note(n1.id));
        assert!(v.contains(&AttributeAssignment {
            attr_id: topics.id,
            value: Some(AttributeValue::Choices(vec![y]))
        }));
    });
    undo(&t);
    assert_undo_redo(&t, |t| {
        let r = t.ok::<AttributesOptionDelete>(DeleteAttributeOption {
            attr_id: lvl.id,
            option_id: b1,
            replace_with: None,
        });
        assert_eq!(
            r.values_affected, 1,
            "the trashed note's value is removed too"
        );
    });
    undo(&t);
    assert_undo_redo(&t, |t| {
        let r = t.ok::<AttributesOptionDelete>(DeleteAttributeOption {
            attr_id: topics.id,
            option_id: x,
            replace_with: None,
        });
        assert_eq!(r.values_affected, 2);
        let v = values(t, EntityRef::Note(n2.id));
        assert!(
            !v.iter().any(|a| a.attr_id == topics.id),
            "emptied multi value is cleared"
        );
    });
    undo(&t);
    let n3_values = values(&t, EntityRef::Note(n3.id));
    assert_eq!(n3_values.len(), 2);

    let e = t.err::<AttributesOptionMerge>(MergeAttributeOptions {
        attr_id: lvl.id,
        from: a1,
        into: a1,
    });
    assert_eq!(e.code, ErrorCode::ValidationInvalidParams);
    let e = t.err::<AttributesOptionDelete>(DeleteAttributeOption {
        attr_id: lvl.id,
        option_id: AttributeOptionId(uuid::Uuid::from_u128(77)),
        replace_with: None,
    });
    assert_eq!(e.code, ErrorCode::NotFoundAttributeOption);
    t.ok::<AttributesUpdate>(UpdateAttribute {
        id: lvl.id,
        name: "Seviye".into(),
        required: true,
        default_value: None,
        position: 0,
    });
    let e = t.err::<AttributesOptionDelete>(DeleteAttributeOption {
        attr_id: lvl.id,
        option_id: a2,
        replace_with: None,
    });
    assert_eq!(e.code, ErrorCode::ConflictAttributeOptionInUse);
    let r = t.ok::<AttributesOptionDelete>(DeleteAttributeOption {
        attr_id: lvl.id,
        option_id: a2,
        replace_with: Some(b1),
    });
    assert_eq!(r.values_affected, 1);
}

pub fn attribute_values_required_and_applicability(f: &StoreFactory) {
    let t = app(f);
    let d = deck(&t, "D");
    let other = deck(&t, "O");
    let page = t.ok::<AttributesDefine>(DefineAttribute {
        scope_deck_id: Some(d.id),
        key: Some("page".into()),
        name: "Sayfa".into(),
        kind: AttributeKind::Number,
        target: None,
        required: Some(true),
        options: vec![],
    });
    let e = t.err::<NotesCreate>(new_note(d.id, "x"));
    assert_eq!(e.code, ErrorCode::ValidationAttributeRequired);
    let n = t.ok::<NotesCreate>(CreateNote {
        attributes: vec![AttributeAssignment {
            attr_id: page.id,
            value: Some(AttributeValue::Number(12.0)),
        }],
        ..new_note(d.id, "x")
    });
    let entity = EntityRef::Note(n.id);
    let e = t.err::<AttributesValuesSet>(SetAttributeValues {
        entity,
        values: vec![AttributeAssignment {
            attr_id: page.id,
            value: None,
        }],
    });
    assert_eq!(e.code, ErrorCode::ValidationAttributeRequired);
    let e = t.err::<AttributesValuesSet>(SetAttributeValues {
        entity,
        values: vec![AttributeAssignment {
            attr_id: page.id,
            value: Some(AttributeValue::Text("x".into())),
        }],
    });
    assert_eq!(e.code, ErrorCode::ValidationAttributeValue);
    assert_undo_redo(&t, |t| {
        let r = t.ok::<AttributesValuesSet>(SetAttributeValues {
            entity,
            values: vec![AttributeAssignment {
                attr_id: page.id,
                value: Some(AttributeValue::Number(13.0)),
            }],
        });
        assert_eq!(r.values[0].value, Some(AttributeValue::Number(13.0)));
    });
    // Notes in other decks do not see it; card-target attributes do not apply to notes.
    let n2 = note(&t, other.id, "y");
    let e = t.err::<AttributesValuesSet>(SetAttributeValues {
        entity: EntityRef::Note(n2.id),
        values: vec![AttributeAssignment {
            attr_id: page.id,
            value: Some(AttributeValue::Number(1.0)),
        }],
    });
    assert_eq!(e.code, ErrorCode::ValidationAttributeNotApplicable);
    let hard = t.ok::<AttributesDefine>(DefineAttribute {
        scope_deck_id: None,
        key: None,
        name: "Zor".into(),
        kind: AttributeKind::Bool,
        target: Some(AttributeTarget::Card),
        required: None,
        options: vec![],
    });
    let e = t.err::<AttributesValuesSet>(SetAttributeValues {
        entity: EntityRef::Note(n2.id),
        values: vec![AttributeAssignment {
            attr_id: hard.id,
            value: Some(AttributeValue::Bool(true)),
        }],
    });
    assert_eq!(e.code, ErrorCode::ValidationAttributeNotApplicable);
    let card = EntityRef::Card(n2.cards[0].id);
    assert_undo_redo(&t, |t| {
        t.ok::<AttributesValuesSet>(SetAttributeValues {
            entity: card,
            values: vec![AttributeAssignment {
                attr_id: hard.id,
                value: Some(AttributeValue::Bool(true)),
            }],
        });
    });
    assert_eq!(values(&t, card).len(), 1);
    // notes.update sets listed attributes and leaves the rest alone.
    assert_undo_redo(&t, |t| {
        let u = t.ok::<NotesUpdate>(UpdateNote {
            id: n.id,
            fields: fields("x", "y"),
            tags: vec![],
            attributes: vec![AttributeAssignment {
                attr_id: page.id,
                value: Some(AttributeValue::Number(99.0)),
            }],
            deck_id: None,
        });
        assert_eq!(u.attributes[0].value, Some(AttributeValue::Number(99.0)));
    });
    let e = t.err::<AttributesValuesGet>(GetAttributeValues {
        entity: EntityRef::Note(fc_core::NoteId(uuid::Uuid::from_u128(4))),
    });
    assert_eq!(e.code, ErrorCode::NotFoundNote);
}

pub fn attribute_delete_restore_undo(f: &StoreFactory) {
    let t = app(f);
    let d = deck(&t, "D");
    let lvl = define(&t, None, "Seviye", AttributeKind::Enum, &["a", "b"]);
    let n = t.ok::<NotesCreate>(CreateNote {
        attributes: vec![AttributeAssignment {
            attr_id: lvl.id,
            value: Some(AttributeValue::Choice(option_id(&lvl, "a"))),
        }],
        ..new_note(d.id, "x")
    });
    assert_undo_redo(&t, |t| {
        t.ok::<AttributesDelete>(AttributeRef { id: lvl.id });
        assert!(values(t, EntityRef::Note(n.id)).is_empty());
        assert!(t
            .ok::<AttributesList>(ListAttributes {
                scope_deck_id: None,
                include_deleted: None
            })
            .defs
            .is_empty());
    });
    // Define the same key while trashed → restore conflicts.
    let again = define(&t, None, "Seviye", AttributeKind::Text, &[]);
    let e = t.err::<AttributesRestore>(AttributeRef { id: lvl.id });
    assert_eq!(e.code, ErrorCode::ConflictAttributeKeyExists);
    t.ok::<AttributesDelete>(AttributeRef { id: again.id });
    t.ok::<AttributesRestore>(AttributeRef { id: lvl.id });
    assert_eq!(
        values(&t, EntityRef::Note(n.id)).len(),
        1,
        "values come back"
    );
    let e = t.err::<AttributesOptionAdd>(AddAttributeOption {
        attr_id: again.id,
        label: "x".into(),
        color: None,
        after: None,
        id: None,
    });
    assert_eq!(e.code, ErrorCode::ConflictDeleted);
}

// ------------------------------------------------------------------ settings & modules

pub fn settings_resolve_set_reset_undo(f: &StoreFactory) {
    let t = app(f);
    let d = deck(&t, "D");
    let get = |scope: SettingScope, key: &str| {
        t.ok::<SettingsGet>(GetSettings {
            scope,
            keys: Some(vec![key.into()]),
        })
        .values
        .remove(0)
    };
    let v = get(SettingScope::Deck(d.id), "decks.include_subdecks");
    assert_eq!((v.value, v.source), (json!(true), None));
    assert_undo_redo(&t, |t| {
        t.ok::<SettingsSet>(SetSetting {
            scope: SettingScope::App,
            key: "decks.include_subdecks".into(),
            value: json!(false),
        });
    });
    t.ok::<SettingsSet>(SetSetting {
        scope: SettingScope::Deck(d.id),
        key: "decks.include_subdecks".into(),
        value: json!(true),
    });
    let v = get(SettingScope::Deck(d.id), "decks.include_subdecks");
    assert_eq!(v.source, Some(SettingScope::Deck(d.id)));
    let v = get(SettingScope::App, "decks.include_subdecks");
    assert_eq!((v.value, v.source), (json!(false), Some(SettingScope::App)));
    let r = t.ok::<SettingsReset>(ResetSetting {
        scope: SettingScope::Deck(d.id),
        key: "decks.include_subdecks".into(),
    });
    assert_eq!(
        r.source,
        Some(SettingScope::App),
        "falls back along the chain"
    );
    undo(&t);
    assert_eq!(
        get(SettingScope::Deck(d.id), "decks.include_subdecks").source,
        Some(SettingScope::Deck(d.id))
    );

    let set = |scope, key: &str, value| {
        t.err::<SettingsSet>(SetSetting {
            scope,
            key: key.into(),
            value,
        })
        .code
    };
    assert_eq!(
        set(SettingScope::Deck(d.id), "app.locale", json!("en")),
        ErrorCode::ValidationSettingValue
    );
    assert_eq!(
        set(SettingScope::App, "app.locale", json!("de")),
        ErrorCode::ValidationSettingValue
    );
    assert_eq!(
        set(SettingScope::App, "nope", json!(1)),
        ErrorCode::NotFoundSetting
    );
    assert_eq!(
        set(
            SettingScope::Deck(DeckId(uuid::Uuid::from_u128(1))),
            "decks.include_subdecks",
            json!(true)
        ),
        ErrorCode::NotFoundDeck
    );

    // The locale setting drives the app locale, also through undo.
    assert_eq!(t.app.locale(), Locale::Tr);
    t.ok::<SettingsSet>(SetSetting {
        scope: SettingScope::App,
        key: "app.locale".into(),
        value: json!("en"),
    });
    assert_eq!(t.app.locale(), Locale::En);
    let tpl = t.ok::<AttributesTemplates>(ListAttributeTemplates { locale: None });
    let cefr = tpl.templates.iter().find(|t| t.id == "cefr").unwrap();
    assert_eq!(cefr.name, "Level");
    undo(&t);
    assert_eq!(t.app.locale(), Locale::Tr);
    let all = t.ok::<SettingsGet>(GetSettings {
        scope: SettingScope::App,
        keys: None,
    });
    assert!(all
        .values
        .iter()
        .any(|v| v.key == "attributes.remember_last"));
    let schema = t.ok::<SettingsSchemaGet>(Empty {});
    assert!(schema.settings.iter().any(|s| s.module == "decks"));
}

pub fn modules_toggle_and_undo(f: &StoreFactory) {
    let t = app(f);
    let d = deck(&t, "D");
    note(&t, d.id, "x");
    assert_undo_redo(&t, |t| {
        let m = t.ok::<ModulesSetEnabled>(SetModuleEnabled {
            id: "tags".into(),
            enabled: false,
        });
        assert!(!m.enabled);
        let e = t
            .app
            .invoke("v1.tags.list", json!({}), &Actor::User)
            .unwrap_err();
        assert_eq!(e.code, ErrorCode::ConflictModuleDisabled);
        assert!(!t.app.is_module_enabled("tags"));
    });
    assert!(!t.app.is_module_enabled("tags"), "redo disabled it again");
    undo(&t);
    assert!(t.app.is_module_enabled("tags"));
    t.ok::<ModulesSetEnabled>(SetModuleEnabled {
        id: "attributes".into(),
        enabled: false,
    });
    let list = t.ok::<ModulesList>(Empty {});
    let attrs = list.modules.iter().find(|m| m.id == "attributes").unwrap();
    assert!(!attrs.enabled && attrs.ui.is_empty());
    assert!(attrs.methods.contains(&"v1.attributes.define".to_owned()));
    let cmds = t.ok::<CommandsList>(Empty {});
    let define = cmds
        .methods
        .iter()
        .find(|m| m.name == "v1.attributes.define")
        .unwrap();
    assert!(!define.enabled);
    let schema = t.ok::<SettingsSchemaGet>(Empty {});
    assert!(!schema.settings.iter().any(|s| s.module == "attributes"));
    let e = t.err::<ModulesSetEnabled>(SetModuleEnabled {
        id: "decks".into(),
        enabled: false,
    });
    assert_eq!(e.code, ErrorCode::ConflictModuleRequired);
    let e = t.err::<ModulesSetEnabled>(SetModuleEnabled {
        id: "nope".into(),
        enabled: true,
    });
    assert_eq!(e.code, ErrorCode::NotFoundModule);
    // The switch survives a restart.
    let t2 = TestApp::new(t.store.clone());
    assert!(!t2.app.is_module_enabled("attributes"));
}

// ------------------------------------------------------------------ undo / redo / batch

pub fn undo_redo_stack_semantics(f: &StoreFactory) {
    let t = app(f);
    assert_eq!(
        code(t.err::<OplogUndo>(Empty {})),
        ErrorCode::ConflictNothingToUndo
    );
    assert_eq!(
        code(t.err::<OplogRedo>(Empty {})),
        ErrorCode::ConflictNothingToRedo
    );
    let s0 = snapshot(&t);
    deck(&t, "X");
    let s1 = snapshot(&t);
    deck(&t, "Y");
    let s2 = snapshot(&t);
    let status = t.ok::<OplogStatus>(Empty {});
    assert!(status.can_undo && !status.can_redo);
    assert_eq!(
        status.undo_summary.unwrap().key,
        "oplog.summary.decks.create"
    );
    let r = t.ok::<OplogUndo>(Empty {});
    assert_eq!(r.summary.params.get("path").map(String::as_str), Some("Y"));
    assert_eq!(snapshot(&t), s1);
    undo(&t);
    assert_eq!(snapshot(&t), s0);
    assert_eq!(
        code(t.err::<OplogUndo>(Empty {})),
        ErrorCode::ConflictNothingToUndo
    );
    let status = t.ok::<OplogStatus>(Empty {});
    assert_eq!(
        status
            .redo_summary
            .unwrap()
            .params
            .get("path")
            .map(String::as_str),
        Some("X")
    );
    redo(&t);
    assert_eq!(snapshot(&t), s1);
    redo(&t);
    assert_eq!(snapshot(&t), s2);
    assert_eq!(
        code(t.err::<OplogRedo>(Empty {})),
        ErrorCode::ConflictNothingToRedo
    );
    // Undo after redo, then a new action clears the redo stack.
    undo(&t);
    assert_eq!(snapshot(&t), s1);
    deck(&t, "Z");
    assert_eq!(
        code(t.err::<OplogRedo>(Empty {})),
        ErrorCode::ConflictNothingToRedo
    );
    undo(&t);
    undo(&t);
    assert_eq!(snapshot(&t), s0);
    // The log is append-only: do/undo/redo entries are all there.
    let log = t.ok::<OplogList>(ListOplog::default());
    let kinds: Vec<OplogKind> = log.items.iter().map(|e| e.kind).collect();
    assert!(kinds.contains(&OplogKind::Redo));
    assert!(log.items.iter().all(|e| e.actor == Actor::User));
    assert!(log
        .items
        .iter()
        .filter(|e| e.kind != OplogKind::Do)
        .all(|e| e.reverts_batch_id.is_some()));
}

pub fn batch_is_atomic_and_one_undo_step(f: &StoreFactory) {
    let t = app(f);
    let call = |m: &str, p: Value| InvokeRequest {
        method: format!("v1.{m}"),
        params: p,
    };
    let s0 = snapshot(&t);
    let r = t.ok::<BatchExecute>(ExecuteBatch {
        calls: vec![
            call("decks.create", json!({"path": "A"})),
            call("decks.create", json!({"path": "B"})),
        ],
    });
    assert_eq!(r.results.len(), 2);
    assert_eq!(r.results[0]["path"], "A");
    assert_eq!(t.ok::<DecksList>(Empty {}).decks.len(), 2);
    undo(&t);
    assert_eq!(snapshot(&t), s0, "one undo reverts the whole batch");
    redo(&t);
    assert_eq!(t.ok::<DecksList>(Empty {}).decks.len(), 2);

    let s1 = snapshot(&t);
    let e = t.err::<BatchExecute>(ExecuteBatch {
        calls: vec![
            call("decks.create", json!({"path": "C"})),
            call("decks.create", json!({"path": "A"})),
        ],
    });
    assert_eq!(e.code, ErrorCode::ConflictDeckExists);
    assert_eq!(snapshot(&t), s1, "all or nothing");
    for bad in [call("oplog.undo", json!({})), call("decks.list", json!({}))] {
        let e = t.err::<BatchExecute>(ExecuteBatch { calls: vec![bad] });
        assert_eq!(e.code, ErrorCode::ValidationInvalidParams);
    }
    t.ok::<ModulesSetEnabled>(SetModuleEnabled {
        id: "tags".into(),
        enabled: false,
    });
    let e = t.err::<BatchExecute>(ExecuteBatch {
        calls: vec![call("tags.merge", json!({"sources": ["a"], "into": "b"}))],
    });
    assert_eq!(e.code, ErrorCode::ConflictModuleDisabled);
}

pub fn events_follow_commits(f: &StoreFactory) {
    let t = app(f);
    t.take_events();
    let d = deck(&t, "A");
    let ev = t.take_events();
    assert_eq!(ev.len(), 1);
    assert!(ev[0].batch_id.is_some());
    assert_eq!(ev[0].event, DomainEvent::DeckCreated { deck: d.clone() });
    t.err::<DecksCreate>(CreateDeck {
        path: "A".into(),
        parent_id: None,
        description: None,
        icon: None,
        color: None,
    });
    assert!(
        t.take_events().is_empty(),
        "failed commands publish nothing"
    );
    undo(&t);
    let ev = t.take_events();
    assert!(matches!(ev[0].event, DomainEvent::DeckDeleted { .. }));
    assert!(matches!(
        ev.last().unwrap().event,
        DomainEvent::Undone { .. }
    ));
    assert!(ev.windows(2).all(|w| w[1].seq == w[0].seq + 1));
}

pub fn jobs_interrupted_and_cancel(f: &StoreFactory) {
    let store = f();
    let id = JobId(uuid::Uuid::from_u128(11));
    {
        let mut tx = store.write().unwrap();
        tx.upsert_job(&JobRecord {
            id,
            kind: "import.text".into(),
            status: "running".into(),
            progress: Some(0.5),
            payload: json!({}),
            result: None,
            error: None,
            created_at: Timestamp(1),
            updated_at: Timestamp(1),
        })
        .unwrap();
        tx.commit().unwrap();
    }
    let t = TestApp::new(store);
    let job = t.ok::<JobsGet>(fc_api::v1::jobs::JobRef { id });
    assert_eq!(job.status, fc_api::v1::jobs::JobStatus::Interrupted);
    assert_eq!(t.ok::<JobsList>(Empty {}).jobs.len(), 1);
    let job = t.ok::<JobsCancel>(fc_api::v1::jobs::JobRef { id });
    assert_eq!(job.status, fc_api::v1::jobs::JobStatus::Cancelled);
    assert!(t.ok::<OplogList>(ListOplog::default()).items.is_empty());
    let e = t.err::<JobsGet>(fc_api::v1::jobs::JobRef {
        id: JobId(uuid::Uuid::from_u128(12)),
    });
    assert_eq!(e.code, ErrorCode::NotFoundJob);
}

pub fn noop_commands_are_not_undo_steps(f: &StoreFactory) {
    let t = app(f);
    t.ok::<SettingsReset>(ResetSetting {
        scope: SettingScope::App,
        key: "app.theme".into(),
    });
    t.ok::<ModulesSetEnabled>(SetModuleEnabled {
        id: "tags".into(),
        enabled: true,
    });
    assert_eq!(
        code(t.err::<OplogUndo>(Empty {})),
        ErrorCode::ConflictNothingToUndo
    );
    assert!(t.ok::<OplogList>(ListOplog::default()).items.is_empty());
}

// ------------------------------------------------------------------ contract review (L3b)

/// After `action`: undo → redo → undo → redo, checking the exact snapshots each time (redo
/// batches are themselves undoable). Leaves the action applied.
fn assert_undo_chain(t: &TestApp, action: impl FnOnce(&TestApp)) {
    let before = snapshot(t);
    action(t);
    let after = snapshot(t);
    for round in 0..2 {
        undo(t);
        assert_same(&snapshot(t), &before, &format!("undo #{round}"));
        redo(t);
        assert_same(&snapshot(t), &after, &format!("redo #{round}"));
    }
}

/// Commands of the newest oplog batch, in `seq` order, with their inverse method names.
fn last_batch(t: &TestApp) -> Vec<(String, Vec<InvokeRequest>)> {
    let tx = t.store.read().unwrap();
    let batch = tx.list_oplog_batches(None, 1).unwrap()[0].batch_id;
    tx.oplog_batch_entries(batch)
        .unwrap()
        .into_iter()
        .map(|r| (r.command, serde_json::from_value(r.inverse).unwrap()))
        .collect()
}

pub fn option_restore_is_exact(f: &StoreFactory) {
    let t = app(f);
    let d = deck(&t, "D");
    let topics = define(
        &t,
        Some(d.id),
        "Konu",
        AttributeKind::MultiEnum,
        &["x", "y", "z", "w"],
    );
    let [x, y, z, w] = ["x", "y", "z", "w"].map(|l| option_id(&topics, l));
    t.ok::<AttributesUpdate>(UpdateAttribute {
        id: topics.id,
        name: "Konu".into(),
        required: false,
        default_value: Some(AttributeValue::Choices(vec![y, z])),
        position: 0,
    });
    let mk = |front: &str, ts: Vec<AttributeOptionId>| {
        t.ok::<NotesCreate>(CreateNote {
            attributes: vec![AttributeAssignment {
                attr_id: topics.id,
                value: Some(AttributeValue::Choices(ts)),
            }],
            ..new_note(d.id, front)
        })
    };
    let n1 = mk("one", vec![w, z, x]);
    let n2 = mk("two", vec![z]);
    let n3 = mk("three", vec![y]);
    let topics_of = |t: &TestApp, n: &NoteDto| {
        values(t, EntityRef::Note(n.id))
            .into_iter()
            .find(|a| a.attr_id == topics.id)
            .and_then(|a| a.value)
    };
    assert_eq!(
        topics_of(&t, &n1),
        Some(AttributeValue::Choices(vec![x, z, w])),
        "multi values come back in option order"
    );

    // Delete a middle option: the inverse is the internal restore (+ default update), and
    // undo puts the option back at index 2 under its id with every value verbatim.
    assert_undo_chain(&t, |t| {
        let r = t.ok::<AttributesOptionDelete>(DeleteAttributeOption {
            attr_id: topics.id,
            option_id: z,
            replace_with: None,
        });
        assert_eq!(r.values_affected, 2);
        let entries = last_batch(t);
        assert_eq!(entries.len(), 1);
        let inverse: Vec<&str> = entries[0].1.iter().map(|c| c.method.as_str()).collect();
        assert_eq!(
            inverse,
            ["v1.attributes.option.restore", "v1.attributes.update"]
        );
        assert_eq!(entries[0].1[0].params["index"], 2);
        assert_eq!(topics_of(t, &n2), None, "emptied value removed");
    });
    undo(&t);
    assert_eq!(last_batch(&t)[0].0, "v1.attributes.option.restore");
    let order: Vec<AttributeOptionId> = t
        .ok::<AttributesGet>(AttributeRef { id: topics.id })
        .options
        .iter()
        .map(|o| o.id)
        .collect();
    assert_eq!(order, [x, y, z, w]);

    // Merge the first option into the last after a reorder; undo/redo/undo is exact.
    t.ok::<AttributesOptionReorder>(ReorderAttributeOptions {
        attr_id: topics.id,
        order: vec![w, x, y, z],
    });
    assert_eq!(
        topics_of(&t, &n1),
        Some(AttributeValue::Choices(vec![w, x, z])),
        "reorder changes the read order of stored values"
    );
    assert_undo_chain(&t, |t| {
        let r = t.ok::<AttributesOptionMerge>(MergeAttributeOptions {
            attr_id: topics.id,
            from: y,
            into: w,
        });
        assert_eq!(r.values_affected, 1);
        assert_eq!(topics_of(t, &n3), Some(AttributeValue::Choices(vec![w])));
        let def = t.ok::<AttributesGet>(AttributeRef { id: topics.id });
        assert_eq!(def.default_value, Some(AttributeValue::Choices(vec![w, z])));
    });
    // The restore itself is internal: unreachable from a transport.
    let e = t
        .app
        .invoke("v1.attributes.option.restore", json!({}), &Actor::User)
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::PermissionDenied);
}

pub fn deck_delete_inverse_carries_ids(f: &StoreFactory) {
    let t = app(f);
    let root = deck(&t, "Dil");
    let sub = deck(&t, "Dil::Fiiller");
    deck(&t, "Matematik");
    let a = note(&t, root.id, "a");
    let b = note(&t, sub.id, "b");
    assert_undo_chain(&t, |t| {
        t.ok::<DecksDelete>(DeckRef { id: root.id });
        let entries = last_batch(t);
        assert_eq!(entries[0].0, "v1.decks.delete");
        let inv = &entries[0].1[0];
        assert_eq!(inv.method, "v1.decks.restore_affected");
        let p: RestoreDeckAffected = serde_json::from_value(inv.params.clone()).unwrap();
        assert_eq!(p.id, root.id);
        let mut decks = p.decks.clone();
        decks.sort();
        let mut want = vec![root.id, sub.id];
        want.sort();
        assert_eq!(decks, want);
        let mut notes = p.notes.clone();
        notes.sort();
        let mut want = vec![a.id, b.id];
        want.sort();
        assert_eq!(notes, want);
        assert_eq!(p.cards.len(), 2);
    });
    // Undo then a new delete: the trash flow restores by `deleted_at` and is undoable too.
    undo(&t);
    let before = snapshot(&t);
    t.ok::<DecksDelete>(DeckRef { id: root.id });
    let trashed = snapshot(&t);
    let r = t.ok::<DecksRestore>(DeckRef { id: root.id });
    assert_eq!(
        r,
        AffectedCount {
            decks: 2,
            notes: 2,
            cards: 2
        }
    );
    assert_same(&snapshot(&t), &before, "restore from trash");
    undo(&t);
    assert_same(&snapshot(&t), &trashed, "undo of restore re-trashes");
    // The internal restore validates its input.
    let e = t
        .app
        .invoke(
            "v1.decks.restore_affected",
            json!({"id": root.id, "decks": [], "notes": [], "cards": []}),
            &Actor::User,
        )
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::PermissionDenied);
}

pub fn note_home_deck_drives_note_attributes(f: &StoreFactory) {
    let t = app(f);
    let d = deck(&t, "D");
    let o = deck(&t, "O");
    let lvl = define(&t, Some(d.id), "Seviye", AttributeKind::Enum, &["a", "b"]);
    let side = t.ok::<AttributesDefine>(DefineAttribute {
        scope_deck_id: Some(d.id),
        key: None,
        name: "Yön".into(),
        kind: AttributeKind::Text,
        target: Some(AttributeTarget::Card),
        required: None,
        options: vec![],
    });
    let a = option_id(&lvl, "a");
    let n = t.ok::<NotesCreate>(CreateNote {
        attributes: vec![AttributeAssignment {
            attr_id: lvl.id,
            value: Some(AttributeValue::Choice(a)),
        }],
        ..new_note(d.id, "x")
    });
    assert_eq!(n.home_deck_id, d.id);
    let card = EntityRef::Card(n.cards[0].id);
    let update = |deck_id: Option<DeckId>, attrs: Vec<AttributeAssignment>| UpdateNote {
        id: n.id,
        fields: fields("x", "arka"),
        tags: vec![],
        attributes: attrs,
        deck_id,
    };
    // Moving the home deck is undoable; the card stays in its deck.
    assert_undo_chain(&t, |t| {
        let u = t.ok::<NotesUpdate>(update(Some(o.id), vec![]));
        assert_eq!(u.home_deck_id, o.id);
        assert_eq!(u.cards[0].deck_id, d.id);
    });
    // Note-level definitions now resolve from O (none there)…
    let e = t.err::<NotesUpdate>(update(
        None,
        vec![AttributeAssignment {
            attr_id: lvl.id,
            value: Some(AttributeValue::Choice(a)),
        }],
    ));
    assert_eq!(e.code, ErrorCode::ValidationAttributeNotApplicable);
    // …card-level ones still from the card's deck D.
    t.ok::<AttributesValuesSet>(SetAttributeValues {
        entity: card,
        values: vec![AttributeAssignment {
            attr_id: side.id,
            value: Some(AttributeValue::Text("ileri".into())),
        }],
    });
    // Moving back with a value in the same call resolves against the new home deck.
    let u = t.ok::<NotesUpdate>(update(
        Some(d.id),
        vec![AttributeAssignment {
            attr_id: lvl.id,
            value: None,
        }],
    ));
    assert_eq!(u.home_deck_id, d.id);
    assert!(u.attributes.is_empty());
    // Unknown / trashed decks are rejected.
    let ghost = DeckId(uuid::Uuid::from_u128(5));
    assert_eq!(
        t.err::<NotesUpdate>(update(Some(ghost), vec![])).code,
        ErrorCode::NotFoundDeck
    );
    t.ok::<DecksDelete>(DeckRef { id: o.id });
    assert_eq!(
        t.err::<NotesUpdate>(update(Some(o.id), vec![])).code,
        ErrorCode::ConflictDeleted
    );
}

pub fn tag_ids_are_minted_by_the_app(f: &StoreFactory) {
    let t = app(f);
    let d = deck(&t, "D");
    let n = t.ok::<NotesCreate>(CreateNote {
        tags: vec!["dil::fiil".into(), "Kolay".into()],
        ..new_note(d.id, "x")
    });
    let tags = t.ok::<TagsList>(Empty {}).tags;
    assert_eq!(tags.len(), 2);
    for tag in &tags {
        assert_eq!(tag.id.0.get_version_num(), 7, "UUIDv7 from the app's IdGen");
    }
    assert_ne!(tags[0].id, tags[1].id);
    // Renaming to a brand-new tag creates its row through the app as well.
    t.ok::<TagsRename>(RenameTag {
        from: "kolay".into(),
        to: "zor".into(),
    });
    let tags = t.ok::<TagsList>(Empty {}).tags;
    let zor = tags.iter().find(|t| t.name.as_str() == "zor").unwrap();
    assert_eq!(zor.id.0.get_version_num(), 7);
    assert_eq!(zor.note_count, 1);
    // Updating keeps existing tag ids.
    let before = tags
        .iter()
        .find(|t| t.name.as_str() == "dil::fiil")
        .unwrap()
        .id;
    t.ok::<NotesUpdate>(UpdateNote {
        id: n.id,
        fields: fields("x", "y"),
        tags: vec!["dil::fiil".into()],
        attributes: vec![],
        deck_id: None,
    });
    let after = t.ok::<TagsList>(Empty {}).tags;
    assert_eq!(
        after
            .iter()
            .find(|t| t.name.as_str() == "dil::fiil")
            .unwrap()
            .id,
        before
    );
}

pub fn undo_redo_chains(f: &StoreFactory) {
    let t = app(f);
    let s0 = snapshot(&t);
    deck(&t, "A");
    let s1 = snapshot(&t);
    deck(&t, "B");
    let s2 = snapshot(&t);
    let status = |t: &TestApp| {
        let s = t.ok::<OplogStatus>(Empty {});
        (s.can_undo, s.can_redo)
    };
    // undo B → redo B → undo (the redo) → undo A → redo A → redo B.
    undo(&t);
    assert_eq!(snapshot(&t), s1);
    redo(&t);
    assert_eq!(snapshot(&t), s2);
    assert_eq!(status(&t), (true, false));
    undo(&t);
    assert_eq!(snapshot(&t), s1, "a redo batch is itself undoable");
    undo(&t);
    assert_eq!(snapshot(&t), s0);
    assert_eq!(status(&t), (false, true));
    redo(&t);
    assert_eq!(snapshot(&t), s1);
    redo(&t);
    assert_eq!(snapshot(&t), s2);
    assert_eq!(status(&t), (true, false));
    // Undo twice, redo once, then a new action: the remaining redo is gone.
    undo(&t);
    undo(&t);
    redo(&t);
    assert_eq!(snapshot(&t), s1);
    assert_eq!(status(&t), (true, true));
    deck(&t, "C");
    assert_eq!(status(&t), (true, false));
    assert_eq!(
        code(t.err::<OplogRedo>(Empty {})),
        ErrorCode::ConflictNothingToRedo
    );
    undo(&t);
    assert_eq!(snapshot(&t), s1);
    assert_eq!(status(&t), (true, true));
    undo(&t);
    assert_eq!(snapshot(&t), s0);
    assert_eq!(
        code(t.err::<OplogUndo>(Empty {})),
        ErrorCode::ConflictNothingToUndo
    );
    // Redo walks forward again: A, then C (B was dropped by the new action).
    redo(&t);
    assert_eq!(snapshot(&t), s1);
    redo(&t);
    let names: Vec<String> = t
        .ok::<DecksList>(Empty {})
        .decks
        .into_iter()
        .map(|d| d.name)
        .collect();
    assert_eq!(names, ["A", "C"]);
}
