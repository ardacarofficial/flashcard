//! `notes.create_many` (v1.1 Create screen): one command, one undo step.

use crate::testing::suite::snapshot;
use crate::testing::TestApp;
use fc_api::v1::attributes::{DefineAttribute, EffectiveAttributes, NewAttributeOption};
use fc_api::v1::common::Empty;
use fc_api::v1::decks::{CreateDeck, DeckDto};
use fc_api::v1::methods::*;
use fc_api::v1::notes::*;
use fc_api::ErrorCode;
use fc_core::attribute::{AttributeKind, AttributeTarget};
use fc_core::notetype::builtin::CLASSIC_ID;
use fc_core::DeckId;
use serde_json::json;
use std::collections::BTreeMap;

fn row(front: &str, back: &str) -> NoteFieldsDraft {
    NoteFieldsDraft {
        fields: BTreeMap::from([("front".into(), front.into()), ("back".into(), back.into())]),
    }
}

fn many(target: NotesTarget, notes: Vec<NoteFieldsDraft>) -> CreateNotes {
    CreateNotes {
        notetype_id: CLASSIC_ID,
        target,
        notes,
        tags: vec!["set".into()],
        attributes: vec![],
        allow_duplicate: None,
    }
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

#[test]
fn new_deck_and_rows_are_one_undo_step() {
    let t = TestApp::mem();
    let parent = deck(&t, "Dil");
    let before = snapshot(&t);
    let r = t.ok::<NotesCreateMany>(many(
        NotesTarget::NewDeck {
            path: "Fiiller".into(),
            parent_id: Some(parent.id),
            description: Some("Düzensiz fiiller".into()),
        },
        vec![row("go", "gitmek"), row("", " "), row("see", "görmek"), row("go", "gitmek")],
    ));
    assert_eq!(r.note_ids.len(), 2);
    assert_eq!(r.empty_rows, vec![1]);
    assert_eq!(r.duplicate_rows, vec![3]);
    let new_deck = t.ok::<DecksGet>(fc_api::v1::decks::DeckRef { id: r.deck_id });
    assert_eq!(new_deck.path, "Dil::Fiiller");
    assert_eq!(new_deck.description, "Düzensiz fiiller");
    let note = t.ok::<NotesGet>(NoteRef { id: r.note_ids[1] });
    assert_eq!(note.home_deck_id, r.deck_id);
    assert_eq!(note.sort_field, "see");
    assert_eq!(note.tags[0].as_str(), "set");

    // One history entry, named after the whole save.
    let status = t.ok::<OplogStatus>(Empty {});
    assert_eq!(
        status.undo_summary.map(|s| s.key),
        Some("oplog.summary.notes.create_many".to_owned())
    );
    let after = snapshot(&t);
    t.ok::<OplogUndo>(Empty {});
    assert_eq!(snapshot(&t), before, "one undo removes the deck and every note");
    // The step before is the parent deck's creation.
    assert_eq!(
        t.ok::<OplogStatus>(Empty {}).undo_summary.map(|s| s.key),
        Some("oplog.summary.decks.create".to_owned())
    );
    t.ok::<OplogRedo>(Empty {});
    assert_eq!(snapshot(&t), after);
}

#[test]
fn existing_deck_duplicates_and_allow_duplicate() {
    let t = TestApp::mem();
    let d = deck(&t, "Bio");
    let target = NotesTarget::Deck { id: d.id };
    t.ok::<NotesCreateMany>(many(target.clone(), vec![row("cell", "hücre")]));
    let again = t.ok::<NotesCreateMany>(many(target.clone(), vec![row("cell", "x"), row("atom", "atom")]));
    assert_eq!((again.note_ids.len(), again.duplicate_rows.clone()), (1, vec![0]));
    let forced = t.ok::<NotesCreateMany>(CreateNotes {
        allow_duplicate: Some(true),
        ..many(target, vec![row("cell", "y")])
    });
    assert_eq!((forced.note_ids.len(), forced.deck_id), (1, d.id));
}

#[test]
fn a_bad_row_rolls_back_everything_and_names_the_row() {
    let t = TestApp::mem();
    let before = snapshot(&t);
    let e = t.err::<NotesCreateMany>(many(
        NotesTarget::NewDeck {
            path: "Kimya".into(),
            parent_id: None,
            description: None,
        },
        vec![row("H", "hidrojen"), row("", "front is required")],
    ));
    assert_eq!(e.code, ErrorCode::ValidationFieldRequired);
    assert_eq!(e.details.get("row"), Some(&json!(1)));
    assert_eq!(snapshot(&t), before, "nothing is written, not even the deck");

    // No rows / only blank rows / too many rows are refused.
    let d = deck(&t, "Kimya");
    let target = NotesTarget::Deck { id: d.id };
    assert_eq!(
        t.err::<NotesCreateMany>(many(target.clone(), vec![])).code,
        ErrorCode::ValidationInvalidParams
    );
    assert_eq!(
        t.err::<NotesCreateMany>(many(target.clone(), vec![row(" ", "")])).code,
        ErrorCode::ValidationFieldRequired
    );
    assert_eq!(
        t.err::<NotesCreateMany>(many(target, vec![row("a", "b"); 1001])).code,
        ErrorCode::ValidationInvalidParams
    );
    let gone = NotesTarget::Deck {
        id: DeckId(uuid::Uuid::from_u128(7)),
    };
    assert_eq!(
        t.err::<NotesCreateMany>(many(gone, vec![row("a", "b")])).code,
        ErrorCode::NotFoundDeck
    );
}

#[test]
fn effective_attributes_without_a_deck_are_the_collection_wide_ones() {
    let t = TestApp::mem();
    let d = deck(&t, "YKS");
    let define = |scope: Option<DeckId>, key: &str| {
        t.ok::<AttributesDefine>(DefineAttribute {
            scope_deck_id: scope,
            key: Some(key.into()),
            name: key.into(),
            kind: AttributeKind::Enum,
            options: vec![NewAttributeOption {
                label: "A".into(),
                color: None,
            }],
            target: Some(AttributeTarget::Note),
            required: None,
        })
    };
    define(None, "level");
    define(Some(d.id), "exam");
    let keys = |deck_id| {
        t.ok::<AttributesEffective>(EffectiveAttributes {
            deck_id,
            target: AttributeTarget::Note,
        })
        .attributes
        .into_iter()
        .map(|a| a.def.key)
        .collect::<Vec<_>>()
    };
    assert_eq!(keys(None), vec!["level"]);
    assert_eq!(keys(Some(d.id)).len(), 2);
}
