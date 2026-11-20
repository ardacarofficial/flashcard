//! v0.1 acceptance, end to end on a real SQLite **file** (docs/04 "v0.1 · Temel"):
//!
//! * migration from an empty database;
//! * deck and note create / edit / delete, each undone to the exact previous state (and
//!   redone);
//! * a "Seviye" (A1–C2) classification defined on a parent deck, selectable on a note in a
//!   child deck; option rename and reorder undoable;
//! * everything survives closing and reopening the database, and the oplog still undoes after
//!   a restart.
//!
//! "Exact" is checked with [`snapshot`]: every API-observable entity, minus `updated_at`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::testing::suite::{assert_same, snapshot};
use crate::testing::TestApp;
use fc_api::v1::attributes::*;
use fc_api::v1::common::{Empty, EntityRef};
use fc_api::v1::decks::*;
use fc_api::v1::methods::*;
use fc_api::v1::notes::*;
use fc_core::attribute::{AttributeKind, AttributeTarget, AttributeValue};
use fc_core::notetype::builtin;
use fc_core::DeckId;
use fc_storage::{SqliteStore, Store};
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

fn open(path: &Path) -> TestApp {
    TestApp::new(Arc::new(SqliteStore::open(path).unwrap()))
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

fn create_note(deck_id: DeckId, front: &str, back: &str) -> CreateNote {
    CreateNote {
        notetype_id: builtin::CLASSIC_ID,
        deck_id,
        fields: BTreeMap::from([
            ("front".to_owned(), front.to_owned()),
            ("back".to_owned(), back.to_owned()),
        ]),
        tags: vec!["fiil".into()],
        attributes: vec![],
        allow_duplicate: None,
    }
}

/// Runs `action` and checks undo restores the exact previous snapshot and redo the changed
/// one. Leaves the action applied.
fn undoable(t: &TestApp, what: &str, action: impl FnOnce(&TestApp)) {
    let before = snapshot(t);
    action(t);
    let after = snapshot(t);
    assert_ne!(before, after, "{what}: changed nothing");
    t.ok::<OplogUndo>(Empty {});
    assert_same(&snapshot(t), &before, &format!("{what}: undo"));
    t.ok::<OplogRedo>(Empty {});
    assert_same(&snapshot(t), &after, &format!("{what}: redo"));
}

#[test]
fn v01_acceptance_on_sqlite_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("collection.db");

    // --- migration from an empty database
    {
        let store = SqliteStore::open(&path).unwrap();
        assert_eq!(
            store.schema_version().unwrap(),
            fc_storage::migrate::latest_version()
        );
        assert!(store.migration_backup().is_none(), "fresh db: no backup");
    }
    let t = open(&path);

    // --- decks: create (missing parents built), rename, delete — all undoable
    let mut lang = None;
    undoable(&t, "deck create", |t| {
        let d = deck(t, "Dil::İngilizce");
        assert_eq!(d.path, "Dil::İngilizce");
        lang = d.parent_id;
    });
    let lang = lang.expect("parent deck created");
    let en = t
        .ok::<DecksList>(Empty {})
        .decks
        .into_iter()
        .find(|d| d.path == "Dil::İngilizce")
        .unwrap();
    undoable(&t, "deck rename", |t| {
        let d = t.ok::<DecksRename>(RenameDeck {
            id: en.id,
            name: "English".into(),
        });
        assert_eq!(d.path, "Dil::English");
    });

    // --- classification "Seviye" (A1–C2) on the parent deck
    let levels = ["A1", "A2", "B1", "B2", "C1", "C2"];
    // (Undo of a create is a soft delete: the definition goes to the trash, so this step is
    // checked through the live view rather than the full snapshot.)
    let seviye = t.ok::<AttributesDefine>(DefineAttribute {
        scope_deck_id: Some(lang),
        key: None,
        name: "Seviye".into(),
        kind: AttributeKind::OrderedEnum,
        target: None,
        required: None,
        options: levels
            .iter()
            .map(|l| NewAttributeOption {
                label: (*l).into(),
                color: None,
            })
            .collect(),
    });
    let live = |t: &TestApp| {
        t.ok::<AttributesEffective>(EffectiveAttributes {
            deck_id: Some(en.id),
            target: AttributeTarget::Note,
        })
        .attributes
        .len()
    };
    t.ok::<OplogUndo>(Empty {});
    assert_eq!(live(&t), 0, "define undone");
    t.ok::<OplogRedo>(Empty {});
    assert_eq!(live(&t), 1, "define redone");
    let opt = |label: &str| seviye.options.iter().find(|o| o.label == label).unwrap().id;
    let eff = t.ok::<AttributesEffective>(EffectiveAttributes {
        deck_id: Some(en.id),
        target: AttributeTarget::Note,
    });
    assert_eq!(eff.attributes.len(), 1);
    assert!(eff.attributes[0].inherited, "inherited from the parent");
    assert_eq!(eff.attributes[0].def.id, seviye.id);

    // --- notes: create in the child deck with Seviye = B1, edit, delete — all undoable
    let mut note = None;
    undoable(&t, "note create", |t| {
        let n = t.ok::<NotesCreate>(CreateNote {
            attributes: vec![AttributeAssignment {
                attr_id: seviye.id,
                value: Some(AttributeValue::Choice(opt("B1"))),
            }],
            ..create_note(en.id, "run", "koşmak")
        });
        assert_eq!(n.home_deck_id, en.id);
        assert_eq!(n.cards.len(), 1);
        assert_eq!(n.cards[0].deck_id, en.id);
        note = Some(n);
    });
    let note = note.unwrap();
    undoable(&t, "note edit", |t| {
        let n = t.ok::<NotesUpdate>(UpdateNote {
            id: note.id,
            deck_id: None,
            fields: BTreeMap::from([
                ("front".to_owned(), "to run".to_owned()),
                ("back".to_owned(), "koşmak".to_owned()),
            ]),
            tags: vec!["fiil".into(), "hareket".into()],
            attributes: vec![AttributeAssignment {
                attr_id: seviye.id,
                value: Some(AttributeValue::Choice(opt("A2"))),
            }],
        });
        assert_eq!(n.fields["front"], "to run");
    });
    undoable(&t, "note delete", |t| {
        let r = t.ok::<NotesDelete>(NoteIds { ids: vec![note.id] });
        assert_eq!(r.notes, 1);
        let page = t.ok::<NotesList>(ListNotes {
            deck_id: None,
            include_subdecks: None,
            page: Default::default(),
        });
        assert!(page.items.is_empty(), "trashed notes are not listed");
    });

    // --- option rename / reorder undoable; stored values keep their option id
    undoable(&t, "option rename", |t| {
        let d = t.ok::<AttributesOptionRename>(RenameAttributeOption {
            attr_id: seviye.id,
            option_id: opt("A2"),
            label: "Temel (A2)".into(),
        });
        assert_eq!(d.options[1].label, "Temel (A2)");
    });
    undoable(&t, "option reorder", |t| {
        let mut order: Vec<_> = seviye.options.iter().map(|o| o.id).collect();
        order.reverse();
        let d = t.ok::<AttributesOptionReorder>(ReorderAttributeOptions {
            attr_id: seviye.id,
            order,
        });
        assert_eq!(d.options[0].label, "C2");
    });
    let values = t
        .ok::<AttributesValuesGet>(GetAttributeValues {
            entity: EntityRef::Note(note.id),
        })
        .values;
    assert_eq!(values[0].value, Some(AttributeValue::Choice(opt("A2"))));

    // --- deck delete cascades (sub-deck, card, note) and undo brings back exactly that; the
    // note trashed earlier stays trashed.
    t.ok::<NotesCreate>(create_note(en.id, "walk", "yürümek"));
    undoable(&t, "deck delete", |t| {
        let r = t.ok::<DecksDelete>(DeckRef { id: lang });
        assert_eq!((r.decks, r.notes, r.cards), (2, 1, 1));
    });

    // --- persistence: reopen the file; same state, no migration, undo still works
    let before_last = {
        // State before the last command (the redone deck delete).
        t.ok::<OplogUndo>(Empty {});
        let s = snapshot(&t);
        t.ok::<OplogRedo>(Empty {});
        s
    };
    let at_close = snapshot(&t);
    drop(t);
    let store = SqliteStore::open(&path).unwrap();
    assert!(
        store.migration_backup().is_none(),
        "up-to-date db: no backup"
    );
    let t = TestApp::new(Arc::new(store));
    assert_same(&snapshot(&t), &at_close, "reopened database");
    t.ok::<OplogUndo>(Empty {});
    assert_same(&snapshot(&t), &before_last, "undo after restart");
}
