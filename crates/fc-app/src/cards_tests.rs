//! v0.3 service tests (card types & content), run against the in-memory store, SQLite in
//! memory and SQLite on a temp file. The docs/04 v0.3 acceptance criteria are marked
//! `ACCEPTANCE`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::events::InMemoryEventBus;
use crate::registry::Registry;
use crate::testing::suite::{assert_same, snapshot};
use crate::testing::{MemStore, TempSqlite, TestApp, TickClock};
use crate::{App, AppDeps};
use fc_api::v1::cardtypes::*;
use fc_api::v1::common::Empty;
use fc_api::v1::decks::{CreateDeck, DeckDto};
use fc_api::v1::methods::*;
use fc_api::v1::modules::SetModuleEnabled;
use fc_api::v1::notes::{CreateNote, NoteDto, NoteRef, UpdateNote};
use fc_api::v1::notetypes::{NoteTypeDto, NoteTypeRef};
use fc_api::ErrorCode;
use fc_core::card::CardSpec;
use fc_core::content::builtin;
use fc_core::note::Note;
use fc_core::notetype::{CardTemplate, FieldDef, FieldKey, FieldKind, NoteType, NoteTypeKind};
use fc_core::rng::SplitMix64;
use fc_core::{CardId, Clock, NoteTypeId, V7IdGen};
use fc_providers::strategy::{
    CardInput, CardTypeEngine, Interaction, RenderContext, RenderedCard, StrategyResult,
};
use fc_providers::ProviderInfo;
use fc_storage::{SqliteStore, Store};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Arc;

type Factory = dyn Fn() -> Arc<dyn Store>;

// ------------------------------------------------------------------ helpers

fn deck(t: &TestApp, path: &str) -> DeckDto {
    t.ok::<DecksCreate>(CreateDeck {
        path: path.into(),
        parent_id: None,
        description: None,
        icon: None,
        color: None,
    })
}

fn fields(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect()
}

fn create_note(t: &TestApp, nt: NoteTypeId, deck: &DeckDto, pairs: &[(&str, &str)]) -> NoteDto {
    t.ok::<NotesCreate>(CreateNote {
        notetype_id: nt,
        deck_id: deck.id,
        fields: fields(pairs),
        tags: vec![],
        attributes: vec![],
        allow_duplicate: Some(true),
    })
}

fn update_fields(t: &TestApp, note: &NoteDto, pairs: &[(&str, &str)]) -> NoteDto {
    let mut f = t.ok::<NotesGet>(NoteRef { id: note.id }).fields;
    for (k, v) in pairs {
        f.insert((*k).to_owned(), (*v).to_owned());
    }
    t.ok::<NotesUpdate>(UpdateNote {
        id: note.id,
        deck_id: None,
        fields: f,
        tags: vec![],
        attributes: vec![],
    })
}

fn card_keys(t: &TestApp, note: &NoteDto) -> Vec<String> {
    let mut v: Vec<String> = t
        .ok::<NotesGet>(NoteRef { id: note.id })
        .cards
        .into_iter()
        .map(|c| c.template_key)
        .collect();
    v.sort();
    v
}

fn render(t: &TestApp, id: CardId) -> RenderedCardDto {
    t.ok::<CardsRender>(CardRef { id })
}

fn undo(t: &TestApp) {
    t.ok::<OplogUndo>(Empty {});
}

fn redo(t: &TestApp) {
    t.ok::<OplogRedo>(Empty {});
}

fn input_of(nt: &NoteTypeDto) -> NoteTypeInput {
    NoteTypeInput {
        name: nt.name.clone(),
        kind: nt.kind,
        fields: nt.fields.clone(),
        templates: nt.templates.clone(),
        css: nt.css.clone(),
        sort_field: nt.sort_field.clone(),
    }
}

fn field(name: &str, kind: FieldKind) -> FieldDef {
    FieldDef {
        key: FieldKey::new(""),
        name: name.into(),
        kind,
        required: false,
        unique: false,
        searchable: true,
        default_value: None,
        role: None,
    }
}

/// Note types, flags and media on top of the v0.1 snapshot.
fn content_snapshot(t: &TestApp) -> Value {
    let mut v = snapshot(t);
    let mut nts = serde_json::to_value(t.ok::<NotetypesList>(Empty {})).unwrap();
    strip_updated(&mut nts);
    v["notetypes"] = nts;
    v["flags"] = serde_json::to_value(t.ok::<FlagsList>(Empty {})).unwrap();
    v["media"] = serde_json::to_value(t.ok::<MediaList>(Empty {})).unwrap();
    let metas: Vec<Value> = v["notes"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .map(|n| {
            let id = serde_json::from_value(n["id"].clone()).unwrap();
            serde_json::to_value(t.ok::<CardsMeta>(CardsOfNote { note_id: id })).unwrap()
        })
        .collect();
    v["card_meta"] = Value::Array(metas);
    v
}

fn strip_updated(v: &mut Value) {
    match v {
        Value::Object(m) => {
            m.remove("updated_at");
            m.values_mut().for_each(strip_updated);
        }
        Value::Array(a) => a.iter_mut().for_each(strip_updated),
        _ => {}
    }
}

/// Runs `action`; undo must restore the previous state, redo the next one.
fn assert_undo_redo(t: &TestApp, action: impl FnOnce(&TestApp)) {
    let before = content_snapshot(t);
    action(t);
    let after = content_snapshot(t);
    assert_ne!(before, after, "action changed nothing");
    undo(t);
    assert_same(&content_snapshot(t), &before, "undo");
    redo(t);
    assert_same(&content_snapshot(t), &after, "redo");
}

// ------------------------------------------------------------------ tests

/// ACCEPTANCE: "Not türüne alan eklemek/çıkarmak mevcut kartları ve geçmişi bozmaz."
pub fn notetype_field_add_remove_keeps_cards_and_history(f: &Factory) {
    let t = TestApp::new(f());
    let d = deck(&t, "Words");
    let nt = t.ok::<NotetypesCreate>(CreateNoteType {
        name: Some("Mine".into()),
        copy_from: Some(builtin::find("classic_reverse").unwrap().id),
        definition: None,
        id: None,
    });
    let note = create_note(&t, nt.id, &d, &[("front", "dog"), ("back", "köpek")]);
    // History: an edit in the oplog and review state on a card.
    let note = update_fields(&t, &note, &[("back", "köpek (hayvan)")]);
    let card_id = note.cards[0].id;
    {
        let mut tx = t.store.write().unwrap();
        let mut c = tx.get_card(card_id).unwrap().unwrap();
        c.scheduling.reps = 4;
        c.scheduling.stability = Some(12.5);
        c.scheduling.state = fc_core::card::CardState::Review;
        tx.update_card(&c).unwrap();
        tx.commit().unwrap();
    }
    let cards_before = t.ok::<NotesGet>(NoteRef { id: note.id }).cards;
    let sched_before = t.store.read().unwrap().get_card(card_id).unwrap().unwrap();

    // Add a field.
    let mut def = input_of(&nt);
    def.fields.push(field("Example", FieldKind::RichText));
    let nt2 = t.ok::<NotetypesUpdate>(UpdateNoteType {
        id: nt.id,
        definition: def,
    });
    assert_eq!(nt2.fields[2].key.as_str(), "example");
    assert_eq!(nt2.version, nt.version + 1);
    let after_add = t.ok::<NotesGet>(NoteRef { id: note.id });
    assert_eq!(after_add.fields["example"], "");
    assert_eq!(after_add.fields["back"], "köpek (hayvan)");
    assert_eq!(after_add.cards, cards_before);

    // Remove it again plus use it nowhere; then remove `back` (templates updated with it).
    let before_remove = content_snapshot(&t);
    let mut def = input_of(&nt2);
    def.fields.retain(|f| f.key.as_str() != "back");
    def.templates[0].back = "{{FrontSide}}<hr>{{example}}".into();
    def.templates[1].front = "{{example}}".into();
    def.templates[1].back = "{{FrontSide}}<hr>{{front}}".into();
    t.ok::<NotetypesUpdate>(UpdateNoteType {
        id: nt.id,
        definition: def,
    });
    let after_remove = t.ok::<NotesGet>(NoteRef { id: note.id });
    assert!(!after_remove.fields.contains_key("back"));
    // v1.3: the forward card asked for `back`; with the field gone it asks for its answer side.
    let nt3 = t.ok::<NotetypesGet>(NoteTypeRef { id: nt.id });
    assert_eq!(nt3.templates[0].answer_field, None);
    assert_eq!(
        nt3.templates[1].answer_field.as_ref().map(|k| k.as_str()),
        Some("front")
    );
    // Same cards (ids, keys, states) and the same scheduling history.
    assert_eq!(after_remove.cards, cards_before);
    let sched_after = t.store.read().unwrap().get_card(card_id).unwrap().unwrap();
    assert_eq!(sched_after.scheduling, sched_before.scheduling);
    assert_eq!(sched_after.created_at, sched_before.created_at);
    // Cards still render (the reverse card's question is now empty but it stays).
    assert!(render(&t, card_id).sides.question.contains("dog"));

    // Undo brings the removed values back exactly; redo removes them again.
    let removed_state = content_snapshot(&t);
    undo(&t);
    assert_same(&content_snapshot(&t), &before_remove, "undo field removal");
    assert_eq!(
        t.ok::<NotesGet>(NoteRef { id: note.id }).fields["back"],
        "köpek (hayvan)"
    );
    redo(&t);
    assert_same(&content_snapshot(&t), &removed_state, "redo field removal");
    // Older history still undoes cleanly through the schema changes.
    undo(&t); // field removal
    undo(&t); // field add
    undo(&t); // note edit
    assert_eq!(
        t.ok::<NotesGet>(NoteRef { id: note.id }).fields["back"],
        "köpek"
    );
    assert_eq!(
        t.ok::<NotesGet>(NoteRef { id: note.id }).cards,
        cards_before
    );
}

pub fn notetype_template_rules(f: &Factory) {
    let t = TestApp::new(f());
    let d = deck(&t, "D");
    let nt = t.ok::<NotetypesCreate>(CreateNoteType {
        name: Some("T".into()),
        copy_from: None,
        definition: None,
        id: None,
    });
    let note = create_note(&t, nt.id, &d, &[("front", "a"), ("back", "b")]);
    assert_eq!(card_keys(&t, &note), ["forward"]);

    // Removing a template that has cards is refused.
    let mut def = input_of(&nt);
    def.templates[0].key = "other".into();
    def.templates.push(CardTemplate {
        key: "x".into(),
        ..def.templates[0].clone()
    });
    let e = t.err::<NotetypesUpdate>(UpdateNoteType {
        id: nt.id,
        definition: def,
    });
    assert_eq!(e.code, ErrorCode::ConflictTemplateInUse);

    // Adding a template generates its cards for existing notes; undo trashes them.
    assert_undo_redo(&t, |t| {
        let mut def = input_of(&nt);
        def.templates.push(CardTemplate {
            key: String::new(),
            name: "Reverse".into(),
            front: "{{back}}".into(),
            back: "{{FrontSide}}<hr>{{front}}".into(),
            card_type: "classic".into(),
            answer_field: None,
        });
        t.ok::<NotetypesUpdate>(UpdateNoteType {
            id: nt.id,
            definition: def,
        });
        assert_eq!(card_keys(t, &note), ["forward", "reverse"]);
    });

    // Switching a template's card type re-tags its cards; undo switches back.
    assert_undo_redo(&t, |t| {
        let current = t.ok::<NotetypesGet>(NoteTypeRef { id: nt.id });
        let mut def = input_of(&current);
        def.templates[0].card_type = "typing".into();
        def.templates[0].front = "{{front}} {{type:back}}".into();
        t.ok::<NotetypesUpdate>(UpdateNoteType {
            id: nt.id,
            definition: def,
        });
        let meta = t.ok::<CardsMeta>(CardsOfNote { note_id: note.id }).items;
        assert_eq!(
            meta.iter()
                .find(|m| m.template_key == "forward")
                .unwrap()
                .card_type,
            "typing"
        );
        let fwd = meta
            .iter()
            .find(|m| m.template_key == "forward")
            .unwrap()
            .id;
        assert_eq!(
            render(t, fwd).sides.interaction,
            CardInteraction::Type {
                expected: "b".into()
            }
        );
    });

    // Invalid templates and unknown engines are rejected.
    let mut bad = input_of(&nt);
    bad.templates[0].front = "{{nope}}".into();
    assert_eq!(
        t.err::<NotetypesUpdate>(UpdateNoteType {
            id: nt.id,
            definition: bad
        })
        .code,
        ErrorCode::ValidationNoteType
    );
    let mut bad = input_of(&nt);
    bad.templates[0].card_type = "nonexistent".into();
    assert_eq!(
        t.err::<NotetypesUpdate>(UpdateNoteType {
            id: nt.id,
            definition: bad
        })
        .code,
        ErrorCode::NotFoundCardType
    );
    let mut bad = input_of(&nt);
    bad.kind = NoteTypeKind::Cloze;
    assert_eq!(
        t.err::<NotetypesUpdate>(UpdateNoteType {
            id: nt.id,
            definition: bad
        })
        .code,
        ErrorCode::ValidationNoteType
    );
}

pub fn notetype_create_delete_restore(f: &Factory) {
    let t = TestApp::new(f());
    let d = deck(&t, "D");
    let count = |t: &TestApp| t.ok::<NotetypesList>(Empty {}).notetypes.len();
    let base = count(&t);
    let nt = t.ok::<NotetypesCreate>(CreateNoteType {
        name: None,
        copy_from: None,
        definition: Some(NoteTypeInput {
            name: "Capitals".into(),
            kind: NoteTypeKind::Standard,
            fields: vec![
                field("Country", FieldKind::Text),
                field("Capital", FieldKind::Text),
            ],
            templates: vec![CardTemplate {
                key: String::new(),
                name: "Ask".into(),
                front: "{{country}}".into(),
                back: "{{FrontSide}}<hr>{{capital}}".into(),
                card_type: "classic".into(),
                answer_field: None,
            }],
            css: ".card { color: red }".into(),
            sort_field: FieldKey::new(""),
        }),
        id: None,
    });
    assert_eq!(nt.sort_field.as_str(), "country");
    assert_eq!(nt.templates[0].key, "ask");
    assert_eq!(count(&t), base + 1);
    // Classic is required; a type in use cannot go.
    assert_eq!(
        t.err::<NotetypesDelete>(NoteTypeRef {
            id: fc_core::notetype::builtin::CLASSIC_ID
        })
        .code,
        ErrorCode::ConflictNoteTypeRequired
    );
    let n = create_note(
        &t,
        nt.id,
        &d,
        &[("country", "Türkiye"), ("capital", "Ankara")],
    );
    assert_eq!(
        t.err::<NotetypesDelete>(NoteTypeRef { id: nt.id }).code,
        ErrorCode::ConflictNoteTypeInUse
    );
    let usage = t.ok::<NotetypesUsage>(NoteTypeRef { id: nt.id });
    assert_eq!(usage.notes, 1);
    assert_eq!(usage.cards_by_template["ask"], 1);
    t.ok::<NotesDelete>(fc_api::v1::notes::NoteIds { ids: vec![n.id] });
    assert_undo_redo(&t, |t| {
        t.ok::<NotetypesDelete>(NoteTypeRef { id: nt.id });
        assert_eq!(count(t), base);
    });
    t.ok::<NotetypesRestore>(NoteTypeRef { id: nt.id });
    assert_eq!(count(&t), base + 1);
    // Notes cannot use a trashed type (v0.1 rule still holds).
    t.ok::<NotetypesDelete>(NoteTypeRef { id: nt.id });
    let e = t.err::<NotesCreate>(CreateNote {
        notetype_id: nt.id,
        deck_id: d.id,
        fields: fields(&[("country", "x")]),
        tags: vec![],
        attributes: vec![],
        allow_duplicate: None,
    });
    assert_eq!(e.code, ErrorCode::ConflictDeleted);
    // Create is undoable.
    assert_undo_redo(&t, |t| {
        t.ok::<NotetypesCreate>(CreateNoteType {
            name: Some("Copy".into()),
            copy_from: Some(fc_core::notetype::builtin::CLASSIC_ID),
            definition: None,
            id: None,
        });
    });
    let types = t.ok::<CardtypesList>(Empty {}).card_types;
    let ids: Vec<_> = types.iter().map(|c| c.id.as_str()).collect();
    assert_eq!(
        ids,
        [
            "classic",
            "cloze",
            "image_occlusion",
            "multiple_choice",
            "typing"
        ]
    );
}

pub fn cloze_cards_follow_the_text(f: &Factory) {
    let t = TestApp::new(f());
    let d = deck(&t, "Geo");
    let cloze = builtin::find("cloze").unwrap().id;
    let n = create_note(
        &t,
        cloze,
        &d,
        &[(
            "text",
            "{{c1::Ankara}} is the capital of {{c2::Türkiye::country}}",
        )],
    );
    assert_eq!(card_keys(&t, &n), ["c1", "c2"]);
    let c2 = n.cards.iter().find(|c| c.template_key == "c2").unwrap().id;
    let r = render(&t, c2);
    assert_eq!(
        r.sides.question,
        "Ankara is the capital of <span class=\"cloze\">[country]</span>"
    );
    assert!(r
        .sides
        .answer
        .contains("<span class=\"cloze\">Türkiye</span>"));
    assert_eq!(r.sides.template_name, "c2");
    // A new number makes a new card; undo trashes it, redo brings it back.
    assert_undo_redo(&t, |t| {
        update_fields(
            t,
            &n,
            &[(
                "text",
                "{{c1::Ankara}} is the capital of {{c2::Türkiye}} ({{c3::1923}})",
            )],
        );
        assert_eq!(card_keys(t, &n), ["c1", "c2", "c3"]);
    });
    // Removing a number keeps its card (history is never dropped silently), marked empty so
    // study skips it (its sides would show the answer).
    assert!(!render(&t, c2).empty);
    update_fields(&t, &n, &[("text", "{{c1::Ankara}}")]);
    assert_eq!(card_keys(&t, &n), ["c1", "c2", "c3"]);
    assert!(render(&t, c2).empty);
    let meta = t.ok::<CardsMeta>(CardsOfNote { note_id: n.id }).items;
    let empty: Vec<_> = meta
        .iter()
        .filter(|m| m.empty)
        .map(|m| m.template_key.as_str())
        .collect();
    assert_eq!(empty, ["c2", "c3"]);
    // Bringing the number back revives the same card.
    update_fields(&t, &n, &[("text", "{{c1::Ankara}} {{c2::Türkiye}}")]);
    assert!(!render(&t, c2).empty);
    // A cloze note without deletions generates nothing.
    let e = t.err::<NotesCreate>(CreateNote {
        notetype_id: cloze,
        deck_id: d.id,
        fields: fields(&[("text", "no gaps")]),
        tags: vec![],
        attributes: vec![],
        allow_duplicate: None,
    });
    assert_eq!(e.code, ErrorCode::ValidationNoteType);
}

pub fn builtin_card_types_render(f: &Factory) {
    let t = TestApp::new(f());
    let d = deck(&t, "Mixed");
    // Optional reverse: second card only when asked for.
    let opt = builtin::find("optional_reverse").unwrap().id;
    let a = create_note(&t, opt, &d, &[("front", "dog"), ("back", "köpek")]);
    assert_eq!(card_keys(&t, &a), ["forward"]);
    update_fields(&t, &a, &[("add_reverse", "y")]);
    assert_eq!(card_keys(&t, &a), ["forward", "reverse"]);

    // Typing.
    let typing = builtin::find("typing").unwrap().id;
    let b = create_note(&t, typing, &d, &[("front", "cat"), ("back", "*kedi*")]);
    let r = render(&t, b.cards[0].id);
    assert_eq!(
        r.sides.interaction,
        CardInteraction::Type {
            expected: "kedi".into()
        }
    );
    assert!(r.sides.question.contains("fc-type-answer"));

    // Multiple choice with automatic distractors from the deck.
    let mc = builtin::find("multiple_choice").unwrap().id;
    for (q, ans) in [("2+2", "4"), ("3+3", "6"), ("4+4", "8"), ("5+5", "10")] {
        create_note(&t, mc, &d, &[("question", q), ("answer", ans)]);
    }
    let last = create_note(&t, mc, &d, &[("question", "1+1"), ("answer", "2")]);
    let r = render(&t, last.cards[0].id);
    let CardInteraction::Choice { options, correct } = &r.sides.interaction else {
        panic!("not a choice card: {:?}", r.sides.interaction)
    };
    assert_eq!(options.len(), 4);
    assert_eq!(options[*correct as usize], "2");
    assert_eq!(
        render(&t, last.cards[0].id).sides.interaction,
        r.sides.interaction
    );

    // Vocabulary: two cards, works with a "level" classification on the deck.
    let level =
        t.ok::<AttributesDefineFromTemplate>(fc_api::v1::attributes::DefineAttributeFromTemplate {
            template_id: "cefr".into(),
            scope_deck_id: Some(d.id),
            locale: None,
        });
    let b1 = level.options[2].id;
    let vocab = builtin::find("vocabulary").unwrap().id;
    let v = t.ok::<NotesCreate>(CreateNote {
        notetype_id: vocab,
        deck_id: d.id,
        fields: fields(&[
            ("word", "run"),
            ("meaning", "koşmak"),
            ("example", "I *run*."),
        ]),
        tags: vec![],
        attributes: vec![fc_api::v1::attributes::AttributeAssignment {
            attr_id: level.id,
            value: Some(fc_core::attribute::AttributeValue::Choice(b1)),
        }],
        allow_duplicate: None,
    });
    assert_eq!(card_keys(&t, &v), ["production", "recognition"]);
    assert_eq!(v.attributes.len(), 1);
    let rec = v
        .cards
        .iter()
        .find(|c| c.template_key == "recognition")
        .unwrap()
        .id;
    let r = render(&t, rec);
    assert!(
        r.sides.answer.contains("<em>run</em>"),
        "{}",
        r.sides.answer
    );

    // Preview without saving (note type editor / add form).
    let p = t.ok::<CardsPreview>(PreviewCards {
        notetype_id: Some(vocab),
        definition: None,
        fields: fields(&[
            ("word", "go"),
            ("meaning", "gitmek"),
            ("unknown", "ignored"),
        ]),
        tags: vec![],
        deck_id: Some(d.id),
    });
    assert_eq!(p.cards.len(), 2);
    assert_eq!(p.cards[0].template_key, "recognition");
    assert!(p.cards[0].question.starts_with("go"));
    let empty = t.ok::<CardsPreview>(PreviewCards {
        notetype_id: Some(vocab),
        definition: None,
        fields: BTreeMap::new(),
        tags: vec![],
        deck_id: None,
    });
    assert!(empty.cards.is_empty());
}

/// ACCEPTANCE: "Aynı medya iki kez eklenince tek dosya tutulur."
pub fn same_media_twice_is_one_entry(f: &Factory) {
    let t = TestApp::new(f());
    let png = b"\x89PNG\r\n\x1a\n-pixels-";
    let data = base64_encode(png);
    let first = t.ok::<MediaAdd>(AddMedia {
        name: Some("a.png".into()),
        data_base64: data.clone(),
    });
    let second = t.ok::<MediaAdd>(AddMedia {
        name: Some("screenshot.png".into()),
        data_base64: data,
    });
    assert!(first.created);
    assert!(!second.created);
    assert_eq!(first.hash, second.hash);
    assert_eq!(first.reference, format!("media:{}", first.hash));
    assert_eq!(first.mime, "image/png");
    let all = t.ok::<MediaList>(Empty {}).items;
    assert_eq!(all.len(), 1, "one entry for the same bytes");
    assert_eq!(all[0].original_name.as_deref(), Some("a.png"));
    let content = t.ok::<MediaRead>(MediaRef {
        hash: first.hash.clone(),
    });
    assert_eq!(content.data_base64, base64_encode(png));

    // Notes count references; the image occlusion type uses it.
    let d = deck(&t, "Anatomy");
    let io = builtin::find("image_occlusion").unwrap().id;
    let masks = r#"{"mode":"hide_all","masks":[{"n":1,"x":0.1,"y":0.1,"w":0.2,"h":0.2},{"n":2,"x":0.5,"y":0.5,"w":0.2,"h":0.2}]}"#;
    let n = create_note(&t, io, &d, &[("image", &first.reference), ("masks", masks)]);
    assert_eq!(card_keys(&t, &n), ["m1", "m2"]);
    assert_eq!(
        t.ok::<MediaGet>(MediaRef {
            hash: first.hash.clone()
        })
        .ref_count,
        1
    );
    let r = render(&t, n.cards[0].id);
    assert!(r
        .sides
        .question
        .contains(&format!("src=\"media:{}\"", first.hash)));
    assert!(r.sides.question.contains("fc-io-active"));
    let other = t.ok::<MediaAdd>(AddMedia {
        name: Some("c.webp".into()),
        data_base64: base64_encode(b"RIFF\0\0\0\0WEBPVP8 other"),
    });
    assert_eq!(other.mime, "image/webp");
    update_fields(&t, &n, &[("image", &other.reference)]);
    assert_eq!(
        t.ok::<MediaGet>(MediaRef {
            hash: first.hash.clone()
        })
        .ref_count,
        0
    );
    assert_eq!(
        t.ok::<MediaGet>(MediaRef {
            hash: other.hash.clone()
        })
        .ref_count,
        1
    );

    // Unsupported content and bad input are rejected.
    let e = t.err::<MediaAdd>(AddMedia {
        name: Some("x.html".into()),
        data_base64: base64_encode(b"<script>alert(1)</script>"),
    });
    assert_eq!(e.code, ErrorCode::ValidationMediaType);
    let e = t.err::<MediaAdd>(AddMedia {
        name: None,
        data_base64: "***".into(),
    });
    assert_eq!(e.code, ErrorCode::ValidationInvalidParams);
    assert_eq!(
        t.err::<MediaGet>(MediaRef {
            hash: "../etc".into()
        })
        .code,
        ErrorCode::NotFoundMedia
    );

    // A new file is an undo step; adding a duplicate is not.
    assert_undo_redo(&t, |t| {
        let m = t.ok::<MediaAdd>(AddMedia {
            name: Some("b.gif".into()),
            data_base64: base64_encode(b"GIF89a-other"),
        });
        assert!(m.created);
    });
    // Undone again, its file is an orphan: kept for redo during the session, removed on the
    // next start; files still in use stay.
    undo(&t);
    let gif = fc_core::content::media::media_hash(b"GIF89a-other");
    let file = |h: &str| t.store.read().unwrap().read_media_file(h).unwrap();
    assert!(file(&gif).is_some());
    t.app.load_state().unwrap();
    assert!(file(&gif).is_none());
    assert!(file(&first.hash).is_some());
}

/// ACCEPTANCE (with `fc_core::content::html` tests): sanitizing applies to everything that
/// leaves the backend — field content, user templates, choice options and CSS.
pub fn rendering_is_sanitized(f: &Factory) {
    let t = TestApp::new(f());
    let d = deck(&t, "X");
    let nt = t.ok::<NotetypesCreate>(CreateNoteType {
        name: Some("Evil".into()),
        copy_from: None,
        definition: Some(NoteTypeInput {
            name: "Evil".into(),
            kind: NoteTypeKind::Standard,
            fields: vec![
                field("Front", FieldKind::RichText),
                field("Back", FieldKind::Text),
            ],
            templates: vec![CardTemplate {
                key: "t".into(),
                name: "t".into(),
                front:
                    "<script>alert(1)</script><div onclick=\"alert(2)\">{{front}}</div>\
                        <a href=\"javascript:alert(3)\">x</a><iframe src=\"https://evil\"></iframe>"
                        .into(),
                back: "{{FrontSide}}<img src=x onerror=alert(4)>{{back}}".into(),
                card_type: "classic".into(),
                answer_field: None,
            }],
            css: "@import url(https://evil/x.css); .card { background: url(https://evil/p.png) }"
                .into(),
            sort_field: FieldKey::new(""),
        }),
        id: None,
    });
    let n = create_note(
        &t,
        nt.id,
        &d,
        &[
            (
                "front",
                "**hi** <img src=x onerror=alert(5)> <svg onload=alert(6)></svg> [[<script>]]",
            ),
            ("back", "<b onmouseover=alert(7)>text</b>"),
        ],
    );
    let r = render(&t, n.cards[0].id);
    // Text fields are escaped, not interpreted.
    let escaped = "&lt;b onmouseover=alert(7)&gt;text&lt;/b&gt;";
    assert!(r.sides.answer.contains(escaped));
    for html in [
        &r.sides.question,
        &r.sides.answer.replace(escaped, ""),
        &r.sides.css,
    ] {
        let lower = html.to_ascii_lowercase();
        for bad in [
            "<script",
            "onclick",
            "onerror",
            "onload",
            "onmouseover",
            "javascript:",
            "<iframe",
            "<svg",
            "@import",
            "url(",
            "evil",
        ] {
            assert!(!lower.contains(bad), "{bad} in {html}");
        }
    }
    assert!(r.sides.question.contains("<strong>hi</strong>"));

    // Choice options are sanitized too.
    let mc = builtin::find("multiple_choice").unwrap().id;
    let n = create_note(
        &t,
        mc,
        &d,
        &[
            ("question", "q"),
            ("answer", "4 > 3"),
            (
                "distractors",
                "<script>x</script>\n<img src=x onerror=alert(1)>y",
            ),
        ],
    );
    let r = render(&t, n.cards[0].id);
    let CardInteraction::Choice { options, .. } = r.sides.interaction else {
        panic!("not a choice card")
    };
    for o in options {
        assert!(!o.contains('<'), "{o}");
    }
}

pub fn links_and_backlinks(f: &Factory) {
    let t = TestApp::new(f());
    let d = deck(&t, "Animals");
    let classic = fc_core::notetype::builtin::CLASSIC_ID;
    // Link to a note that does not exist yet.
    let cat = create_note(
        &t,
        classic,
        &d,
        &[
            ("front", "Kedi"),
            ("back", "cat, see [[Porsuk|the badger]]"),
        ],
    );
    let r = render(&t, cat.cards[0].id);
    assert!(
        r.sides.answer.contains("fc-link-missing"),
        "{}",
        r.sides.answer
    );
    let badger = create_note(&t, classic, &d, &[("front", "Porsuk"), ("back", "badger")]);
    let r = render(&t, cat.cards[0].id);
    assert!(
        r.sides
            .answer
            .contains(&format!("<a class=\"fc-link\" href=\"note:{}\"", badger.id)),
        "{}",
        r.sides.answer
    );
    let back = t
        .ok::<LinksBacklinks>(BacklinksOf { note_id: badger.id })
        .items;
    assert_eq!(
        back,
        vec![LinkedNote {
            id: cat.id,
            title: "Kedi".into()
        }]
    );
    let s = t
        .ok::<LinksSuggest>(SuggestLinks {
            prefix: "por".into(),
            limit: None,
        })
        .items;
    assert_eq!(s.len(), 1);
    assert_eq!(s[0].id, badger.id);
    // Links are by title (wiki style, docs/10): renaming the target leaves the link pointing
    // at the old title, shown as missing (never silently to another note); renaming it back
    // or creating a note with that title resolves it again.
    update_fields(&t, &badger, &[("front", "Porsuk (hayvan)")]);
    let r = render(&t, cat.cards[0].id);
    assert!(
        r.sides.answer.contains("fc-link-missing"),
        "{}",
        r.sides.answer
    );
    assert!(t
        .ok::<LinksBacklinks>(BacklinksOf { note_id: badger.id })
        .items
        .is_empty());
    update_fields(&t, &badger, &[("front", "Porsuk")]);
    let r = render(&t, cat.cards[0].id);
    assert!(r
        .sides
        .answer
        .contains(&format!("href=\"note:{}\"", badger.id)));
    // Removing the link removes the backlink; undo restores it.
    assert_undo_redo(&t, |t| {
        update_fields(t, &cat, &[("back", "cat")]);
        assert!(t
            .ok::<LinksBacklinks>(BacklinksOf { note_id: badger.id })
            .items
            .is_empty());
    });
    // Trashed notes do not link.
    t.ok::<NotesDelete>(fc_api::v1::notes::NoteIds { ids: vec![cat.id] });
    assert!(t
        .ok::<LinksBacklinks>(BacklinksOf { note_id: badger.id })
        .items
        .is_empty());
    // The module can be switched off.
    t.ok::<ModulesSetEnabled>(SetModuleEnabled {
        id: "links".into(),
        enabled: false,
    });
    assert_eq!(
        t.err::<LinksBacklinks>(BacklinksOf { note_id: badger.id })
            .code,
        ErrorCode::ConflictModuleDisabled
    );
}

pub fn flags_and_card_notes(f: &Factory) {
    let t = TestApp::new(f());
    let d = deck(&t, "F");
    let n = create_note(
        &t,
        fc_core::notetype::builtin::CLASSIC_ID,
        &d,
        &[("front", "a")],
    );
    let card = n.cards[0].id;
    assert_eq!(t.ok::<FlagsList>(Empty {}).flags.len(), 7);
    assert_undo_redo(&t, |t| {
        let f = t.ok::<FlagsUpdate>(UpdateFlag {
            flag: 1,
            name: Some(" Zor ".into()),
            color: None,
        });
        assert_eq!(f.name.as_deref(), Some("Zor"));
        assert_eq!(f.color, "red");
    });
    assert_undo_redo(&t, |t| {
        let r = t.ok::<CardsSetFlag>(SetCardFlag {
            ids: vec![card],
            flag: 3,
            on: true,
        });
        assert_eq!(r.items[0].flags, 0b100);
    });
    t.ok::<CardsSetFlag>(SetCardFlag {
        ids: vec![card],
        flag: 1,
        on: true,
    });
    assert_eq!(render(&t, card).flags, 0b101);
    t.ok::<CardsSetFlag>(SetCardFlag {
        ids: vec![card],
        flag: 3,
        on: false,
    });
    assert_eq!(render(&t, card).flags, 0b001);
    assert_eq!(
        t.err::<CardsSetFlag>(SetCardFlag {
            ids: vec![card],
            flag: 8,
            on: true
        })
        .code,
        ErrorCode::ValidationFlag
    );
    assert_eq!(
        t.err::<FlagsUpdate>(UpdateFlag {
            flag: 0,
            name: None,
            color: None
        })
        .code,
        ErrorCode::ValidationFlag
    );
    assert_undo_redo(&t, |t| {
        let m = t.ok::<CardsSetUserNote>(SetCardUserNote {
            id: card,
            text: Some(" mixed up with b ".into()),
        });
        assert_eq!(m.user_note.as_deref(), Some("mixed up with b"));
    });
    let meta = t.ok::<CardsMeta>(CardsOfNote { note_id: n.id }).items;
    assert_eq!(meta[0].user_note.as_deref(), Some("mixed up with b"));
    assert_eq!(meta[0].template_name, "1");
    assert_eq!(
        render(&t, card).user_note.as_deref(),
        Some("mixed up with b")
    );
    // Same text again: no-op, no undo step.
    let before = t
        .ok::<OplogList>(fc_api::v1::oplog::ListOplog::default())
        .items
        .len();
    t.ok::<CardsSetUserNote>(SetCardUserNote {
        id: card,
        text: Some("mixed up with b".into()),
    });
    assert_eq!(
        t.ok::<OplogList>(fc_api::v1::oplog::ListOplog::default())
            .items
            .len(),
        before
    );
}

// ------------------------------------------------------------------ a new card type

/// A test-only card type: one card per non-empty field, asking the field's text reversed.
struct ReverseWords;

impl CardTypeEngine for ReverseWords {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            id: "reverse_words".into(),
            name_key: "test.reverse_words".into(),
            capabilities: vec![],
            settings: vec![],
            secret_keys: vec![],
            local: true,
        }
    }

    fn generate_cards(
        &self,
        note: &Note,
        _notetype: &NoteType,
        template: &CardTemplate,
    ) -> StrategyResult<Vec<CardSpec>> {
        Ok(note
            .fields
            .iter()
            .filter(|(_, v)| !v.trim().is_empty())
            .map(|(k, _)| CardSpec {
                template_key: format!("rev_{}", k.as_str()),
                card_type: template.card_type.clone(),
            })
            .collect())
    }

    fn render(&self, input: &CardInput<'_>, _ctx: &RenderContext) -> StrategyResult<RenderedCard> {
        let key = input.template_key.trim_start_matches("rev_");
        let text = input
            .note
            .fields
            .get(&FieldKey::new(key))
            .cloned()
            .unwrap_or_default();
        Ok(RenderedCard {
            question: text.chars().rev().collect(),
            answer: format!("{text}<script>leak()</script>"),
            css: String::new(),
            interaction: Interaction::SelfGrade,
            tts: vec![],
        })
    }
}

fn app_with_extra_engine(store: Arc<dyn Store>) -> TestApp {
    let mut registry = Registry::new();
    crate::modules::register_all(&mut registry);
    registry.card_type(Arc::new(ReverseWords));
    let clock: Arc<dyn Clock> = Arc::new(TickClock::new(1_767_225_600_000));
    let app = App::with_registry(
        AppDeps {
            store: store.clone(),
            ids: Box::new(V7IdGen::new(clock.clone(), SplitMix64::new(7))),
            clock,
            events: Arc::new(InMemoryEventBus::new()),
        },
        registry,
    )
    .unwrap();
    app.load_state().unwrap();
    TestApp {
        app,
        store,
        events: Arc::default(),
    }
}

/// ACCEPTANCE: "Yeni bir kart türü sadece `CardTypeEngine` uygulanarak eklenebilir."
/// `ReverseWords` above is the only code; note types, notes, generation, rendering,
/// sanitizing and undo all work through the existing paths.
pub fn a_new_card_type_is_just_an_engine(f: &Factory) {
    // Without registration the type is unknown.
    let plain = TestApp::new(f());
    let def = |name: &str| NoteTypeInput {
        name: name.into(),
        kind: NoteTypeKind::Standard,
        fields: vec![field("A", FieldKind::Text), field("B", FieldKind::Text)],
        templates: vec![CardTemplate {
            key: "rev".into(),
            name: "Reverse words".into(),
            front: "{{a}}".into(),
            back: "{{b}}".into(),
            card_type: "reverse_words".into(),
            answer_field: None,
        }],
        css: String::new(),
        sort_field: FieldKey::new(""),
    };
    assert_eq!(
        plain
            .err::<NotetypesCreate>(CreateNoteType {
                name: None,
                copy_from: None,
                definition: Some(def("R")),
                id: None
            })
            .code,
        ErrorCode::NotFoundCardType
    );

    let t = app_with_extra_engine(f());
    let ids: Vec<String> = t
        .ok::<CardtypesList>(Empty {})
        .card_types
        .into_iter()
        .map(|c| c.id)
        .collect();
    assert!(ids.contains(&"reverse_words".to_owned()));
    let nt = t.ok::<NotetypesCreate>(CreateNoteType {
        name: None,
        copy_from: None,
        definition: Some(def("R")),
        id: None,
    });
    let d = deck(&t, "Rev");
    let n = create_note(&t, nt.id, &d, &[("a", "kitap")]);
    assert_eq!(card_keys(&t, &n), ["rev_a"]);
    let r = render(&t, n.cards[0].id);
    assert_eq!(r.sides.question, "patik");
    assert_eq!(r.sides.card_type, "reverse_words");
    assert!(
        !r.sides.answer.contains("<script"),
        "engine output is sanitized"
    );
    // Generation on update and its undo use the same paths as the built-ins.
    assert_undo_redo(&t, |t| {
        update_fields(t, &n, &[("b", "kalem")]);
        assert_eq!(card_keys(t, &n), ["rev_a", "rev_b"]);
    });
    let p = t.ok::<CardsPreview>(PreviewCards {
        notetype_id: Some(nt.id),
        definition: None,
        fields: fields(&[("a", "abc"), ("b", "xy")]),
        tags: vec![],
        deck_id: None,
    });
    let qs: Vec<_> = p.cards.iter().map(|c| c.question.as_str()).collect();
    assert_eq!(qs, ["cba", "yx"]);
}

/// v0.3's internal methods (undo inverses); the shared test in `tests.rs` checks that every
/// internal method is undoable and unreachable from transports.
#[test]
fn v03_internal_methods() {
    assert_eq!(
        crate::tests::internal_methods_of(&TestApp::mem(), &["cardtypes", "media"]),
        [
            "cards.restore_generated",
            "cards.trash_generated",
            "media.forget",
            // v1.4 W4: where a file came from (derived metadata, never in the oplog).
            "media.origin.record",
            "media.reinstate",
            "notetypes.restore_version",
        ]
    );
}

fn base64_encode(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

// ------------------------------------------------------------------ runners

macro_rules! cards_suite {
    ($factory:expr) => {
        cards_suite!(@tests $factory;
            notetype_field_add_remove_keeps_cards_and_history,
            notetype_template_rules,
            notetype_create_delete_restore,
            cloze_cards_follow_the_text,
            builtin_card_types_render,
            same_media_twice_is_one_entry,
            rendering_is_sanitized,
            links_and_backlinks,
            flags_and_card_notes,
            a_new_card_type_is_just_an_engine,
        );
    };
    (@tests $factory:expr; $($name:ident),* $(,)?) => {
        $(
            #[test]
            fn $name() {
                let factory = $factory;
                super::$name(&factory);
            }
        )*
    };
}

mod mem {
    use super::*;
    cards_suite!(|| -> Arc<dyn Store> { Arc::new(MemStore::new()) });
}

mod sqlite_memory {
    use super::*;
    cards_suite!(|| -> Arc<dyn Store> { Arc::new(SqliteStore::open_in_memory().unwrap()) });
}

mod sqlite_file {
    use super::*;
    cards_suite!(|| -> Arc<dyn Store> { Arc::new(TempSqlite::new()) });
}

/// ACCEPTANCE: one file on disk for the same media added twice through the app.
#[test]
fn same_media_twice_is_one_file_on_disk() {
    let store = Arc::new(TempSqlite::new());
    let db = store.path();
    let t = TestApp::new(store);
    let data = base64_encode(b"\xff\xd8\xff\xe0jpeg-bytes");
    for name in ["a.jpg", "b.jpg", "a.jpg"] {
        t.ok::<MediaAdd>(AddMedia {
            name: Some(name.into()),
            data_base64: data.clone(),
        });
    }
    let media_dir = db.parent().unwrap().join("media");
    let mut files = Vec::new();
    for shard in std::fs::read_dir(&media_dir).unwrap() {
        for f in std::fs::read_dir(shard.unwrap().path()).unwrap() {
            files.push(f.unwrap().path());
        }
    }
    assert_eq!(files.len(), 1, "{files:?}");
    let hash = files[0].file_name().unwrap().to_string_lossy().into_owned();
    assert_eq!(
        fc_storage::media::media_file_path(&db, &hash).unwrap(),
        files[0]
    );
}
