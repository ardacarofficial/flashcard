//! Service tests of the `browse` and `palette` modules, each run against the in-memory store
//! and SQLite (in memory and on a temp file).
//!
//! v0.4 acceptance: `bulk_ops_on_1000_cards_undo_in_one_step` (one undo restores 1000 cards
//! exactly), `palette_lists_every_registry_command`, query errors are positioned
//! (`search_errors_are_positioned`); parser property tests live in `fc_core::query::tests`,
//! the 100k benchmark in `fc_storage::tests::search`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::testing::{MemStore, TempSqlite, TestApp};
use fc_api::v1::attributes::*;
use fc_api::v1::browse::*;
use fc_api::v1::bulk::*;
use fc_api::v1::common::{Empty, EntityRef};
use fc_api::v1::decks::*;
use fc_api::v1::methods::*;
use fc_api::v1::modules::SetModuleEnabled;
use fc_api::v1::notes::*;
use fc_api::v1::oplog::ListOplog;
use fc_api::v1::search::*;
use fc_api::{ErrorCode, Visibility};
use fc_core::attribute::{AttributeKind, AttributeValue};
use fc_core::card::{CardState, SchedulingState};
use fc_core::notetype::builtin;
use fc_core::paging::PageRequest;
use fc_core::{AttributeOptionId, CardId, DeckId, NoteId, Timestamp};
use fc_storage::repo::CardFilter;
use fc_storage::{SqliteStore, Store};
use std::collections::BTreeMap;
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
    bulk_ops_on_1000_cards_undo_in_one_step,
    search_with_classifications_and_paging,
    search_errors_are_positioned,
    saved_queries_crud_undo,
    bulk_attribute_skips_inapplicable_and_restores,
    find_replace_and_tags,
    palette_lists_every_registry_command,
    shortcuts_set_conflict_reset_undo,
    card_info_duplicates_trash,
    database_check_and_repair_undo,
    module_can_be_disabled,
);

// ------------------------------------------------------------------ helpers

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

fn note(t: &TestApp, deck_id: DeckId, front: &str, tags: &[&str]) -> NoteDto {
    t.ok::<NotesCreate>(CreateNote {
        notetype_id: builtin::CLASSIC_ID,
        deck_id,
        fields: BTreeMap::from([
            ("front".to_owned(), front.to_owned()),
            ("back".to_owned(), "arka".to_owned()),
        ]),
        tags: tags.iter().map(|s| (*s).to_owned()).collect(),
        attributes: vec![],
        allow_duplicate: Some(true),
    })
}

fn search(t: &TestApp, q: &str) -> CardSearchPage {
    t.ok::<SearchCardsMethod>(SearchCards {
        query: q.into(),
        sort: Some(CardSortKey::Created),
        descending: None,
        notes_only: None,
        offset: None,
        limit: Some(500),
    })
}

fn ids(t: &TestApp, q: &str) -> SearchIds {
    t.ok::<SearchIdsMethod>(QueryText { query: q.into() })
}

fn undo(t: &TestApp) {
    t.ok::<OplogUndo>(Empty {});
}

fn redo(t: &TestApp) {
    t.ok::<OplogRedo>(Empty {});
}

/// The newest oplog entries (newest first).
fn newest(t: &TestApp) -> Vec<fc_api::v1::oplog::OplogEntryDto> {
    t.ok::<OplogList>(ListOplog {
        page: PageRequest {
            cursor: None,
            limit: Some(5),
        },
    })
    .items
}

/// Asserts the newest batch consists of exactly one entry of `method`.
fn one_entry_batch(t: &TestApp, method: &str) {
    let top = newest(t);
    assert_eq!(top[0].command, format!("v1.{method}"));
    assert_eq!(top.iter().filter(|e| e.batch_id == top[0].batch_id).count(), 1);
}

/// Everything a bulk operation can touch, minus modification times.
fn state(t: &TestApp) -> String {
    let tx = t.store.read().unwrap();
    let mut out = String::new();
    let mut cards = tx
        .list_cards(&CardFilter {
            deck_ids: None,
            note_ids: None,
            include_deleted: true,
        })
        .unwrap();
    cards.sort_by_key(|c| c.id);
    for mut c in cards {
        c.updated_at = Timestamp(0);
        out.push_str(&format!("{c:?}\n"));
        out.push_str(&format!("{:?}\n", tx.get_attribute_values(EntityRef::Card(c.id)).unwrap()));
        if let Some(mut n) = tx.get_note(c.note_id).unwrap() {
            n.updated_at = Timestamp(0);
            out.push_str(&format!("{n:?}\n"));
            out.push_str(&format!("{:?}\n", tx.get_attribute_values(EntityRef::Note(n.id)).unwrap()));
        }
    }
    out
}

fn level(t: &TestApp, scope: Option<DeckId>) -> AttributeDefDto {
    t.ok::<AttributesDefine>(DefineAttribute {
        scope_deck_id: scope,
        key: Some("level".into()),
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
    })
}

fn option(def: &AttributeDefDto, label: &str) -> AttributeOptionId {
    def.options.iter().find(|o| o.label == label).unwrap().id
}

/// Makes cards look reviewed (only the study engine of v0.2 does that through the API).
fn make_reviewed(t: &TestApp, ids: &[CardId]) {
    let mut tx = t.store.write().unwrap();
    for (i, id) in ids.iter().enumerate() {
        let mut c = tx.get_card(*id).unwrap().unwrap();
        c.scheduling = SchedulingState {
            state: CardState::Review,
            due: Some(Timestamp(1_000 + i as i64)),
            stability: Some(3.5 + i as f64),
            difficulty: Some(5.0),
            reps: 3,
            lapses: (i % 4) as u32,
            last_review: Some(Timestamp(500)),
        };
        c.flags = (i % 3) as u32;
        tx.update_card(&c).unwrap();
    }
    tx.commit().unwrap();
}

// ------------------------------------------------------------------ tests

/// v0.4 acceptance: "1000 kartlık toplu işlem tek geri al ile tamamen geri döner".
pub(super) fn bulk_ops_on_1000_cards_undo_in_one_step(f: Factory) {
    let t = TestApp::new(f());
    let a = deck(&t, "A");
    let b = deck(&t, "B");
    let lvl = level(&t, None);
    for i in 0..1000 {
        note(&t, a, &format!("kelime {i}"), &["eski"]);
    }
    let sel = ids(&t, "deck:A");
    assert_eq!(sel.card_ids.len(), 1000);
    make_reviewed(&t, &sel.card_ids);
    let cards = sel.card_ids.clone();
    let before = state(&t);
    let one = |t: &TestApp, method: &str, run: &dyn Fn(&TestApp) -> BulkResult| {
        let r = run(t);
        assert_eq!(r.changed, 1000, "{method}: {r:?}");
        one_entry_batch(t, method);
        let after = state(t);
        assert_ne!(after, before);
        undo(t);
        assert_eq!(state(t), before, "one undo restores everything");
        redo(t);
        assert_eq!(state(t), after, "redo re-applies exactly");
        undo(t);
        assert_eq!(state(t), before);
    };
    one(&t, "bulk.move_cards", &|t| t.ok::<BulkMoveCards>(MoveCards { card_ids: cards.clone(), deck_id: b }));
    one(&t, "bulk.set_suspended", &|t| t.ok::<BulkSetSuspended>(SetSuspended { card_ids: cards.clone(), suspended: true }));
    one(&t, "bulk.set_buried", &|t| t.ok::<BulkSetBuried>(SetBuried { card_ids: cards.clone(), buried: true }));
    one(&t, "bulk.set_flag", &|t| t.ok::<BulkSetFlag>(SetFlag { card_ids: cards.clone(), flag: 5, on: true }));
    one(&t, "bulk.reset_progress", &|t| t.ok::<BulkResetProgress>(CardSelection { card_ids: cards.clone() }));
    let notes = sel.note_ids.clone();
    one(&t, "bulk.add_tags", &|t| t.ok::<BulkAddTags>(NoteTags { note_ids: notes.clone(), tags: vec!["yeni::alt".into()] }));
    one(&t, "bulk.remove_tags", &|t| t.ok::<BulkRemoveTags>(NoteTags { note_ids: notes.clone(), tags: vec!["eski".into()] }));
    one(&t, "bulk.find_replace", &|t| {
        t.ok::<BulkFindReplace>(FindReplace {
            note_ids: notes.clone(),
            find: "kelime".into(),
            replace: "söz".into(),
            field: Some("front".into()),
            regex: None,
            match_case: None,
        })
    });
    one(&t, "bulk.set_attribute", &|t| {
        t.ok::<BulkSetAttribute>(SetAttributeBulk {
            card_ids: cards.clone(),
            attr_id: lvl.id,
            value: Some(AttributeValue::Choice(option(&lvl, "B2"))),
        })
    });
    // The changes are visible to the query language.
    t.ok::<BulkSetAttribute>(SetAttributeBulk {
        card_ids: cards[..300].to_vec(),
        attr_id: lvl.id,
        value: Some(AttributeValue::Choice(option(&lvl, "C1"))),
    });
    assert_eq!(search(&t, "seviye>=B1").total, 300);
    t.ok::<BulkMoveCards>(MoveCards { card_ids: cards[..10].to_vec(), deck_id: b });
    assert_eq!(search(&t, "deck:B").total, 10);
    // Nothing changes → no undo step.
    let top = newest(&t)[0].id;
    let r = t.ok::<BulkMoveCards>(MoveCards { card_ids: cards[..10].to_vec(), deck_id: b });
    assert_eq!((r.changed, r.skipped), (0, 10));
    assert_eq!(newest(&t)[0].id, top);
}

pub(super) fn search_with_classifications_and_paging(f: Factory) {
    let t = TestApp::new(f());
    let en = deck(&t, "Dil::İngilizce");
    let yks = deck(&t, "YKS");
    let lvl = level(&t, Some(en));
    let exam = t.ok::<AttributesDefine>(DefineAttribute {
        scope_deck_id: Some(yks),
        key: None,
        name: "Sınav".into(),
        kind: AttributeKind::Enum,
        target: None,
        required: None,
        options: ["TYT", "AYT"]
            .iter()
            .map(|l| NewAttributeOption { label: (*l).into(), color: None })
            .collect(),
    });
    let mut levels = Vec::new();
    for (i, l) in ["A1", "B1", "B2", "C1"].iter().enumerate() {
        let n = t.ok::<NotesCreate>(CreateNote {
            notetype_id: builtin::CLASSIC_ID,
            deck_id: en,
            fields: BTreeMap::from([("front".into(), format!("Kılıç {i}")), ("back".into(), "x".into())]),
            tags: vec!["fiil".into()],
            attributes: vec![AttributeAssignment {
                attr_id: lvl.id,
                value: Some(AttributeValue::Choice(option(&lvl, l))),
            }],
            allow_duplicate: None,
        });
        levels.push(n.id);
    }
    let tyt = t.ok::<NotesCreate>(CreateNote {
        notetype_id: builtin::CLASSIC_ID,
        deck_id: yks,
        fields: BTreeMap::from([("front".into(), "kuvvet".into()), ("back".into(), "x".into())]),
        tags: vec![],
        attributes: vec![AttributeAssignment {
            attr_id: exam.id,
            value: Some(AttributeValue::Choice(option(&exam, "TYT"))),
        }],
        allow_duplicate: None,
    });
    let count = |q: &str| search(&t, q).total;
    assert_eq!(count(""), 5);
    assert_eq!(count("seviye:B1"), 1);
    assert_eq!(count("seviye>=B1"), 3);
    assert_eq!(count("level<b2"), 2);
    assert_eq!(count("sınav:TYT"), 1);
    assert_eq!(count("sinav:tyt -seviye:*"), 1);
    assert_eq!(count("deck:Dil"), 4);
    assert_eq!(count(r#"deck:"Dil::İngilizce" kilic"#), 4);
    assert_eq!(count("tag:fiil seviye>B1"), 2);
    assert_eq!(count("is:new"), 5);
    let ids = ids(&t, "kuvvet");
    assert_eq!(ids.note_ids, vec![tyt.id]);
    // Paging and rows.
    let p1 = t.ok::<SearchCardsMethod>(SearchCards {
        query: "deck:Dil".into(),
        sort: Some(CardSortKey::SortField),
        descending: Some(true),
        notes_only: Some(true),
        offset: Some(1),
        limit: Some(2),
    });
    assert_eq!((p1.total, p1.items.len(), p1.offset), (4, 2, 1));
    assert_eq!(p1.items[0].sort_field, "Kılıç 2");
    assert_eq!(p1.items[0].deck_path, "Dil::İngilizce");
    assert_eq!(p1.items[0].tags[0].as_str(), "fiil");
    // A saved filter over classifications keeps tracking renamed options (ids, not labels).
    let saved = t.ok::<SavedQueriesCreate>(CreateSavedQuery {
        name: "B1 ve üstü".into(),
        query: "seviye>=B1".into(),
        icon: None,
        position: None,
    });
    assert_eq!(count(&saved.query), 3);
}

pub(super) fn search_errors_are_positioned(f: Factory) {
    let t = TestApp::new(f());
    let en = deck(&t, "İngilizce");
    level(&t, Some(en));
    let e = t.err::<SearchCardsMethod>(SearchCards {
        query: "deck:İngilizce seviye:B7".into(),
        sort: None,
        descending: None,
        notes_only: None,
        offset: None,
        limit: None,
    });
    assert_eq!(e.code, ErrorCode::ValidationQuery);
    assert_eq!(e.message_key, "errors.validation.query");
    assert_eq!(e.details["kind"], "unknown_option");
    assert_eq!((e.details["start"].as_u64(), e.details["end"].as_u64()), (Some(22), Some(24)));
    assert_eq!(e.details["value"], "B7");
    let c = t.ok::<SearchCheck>(QueryText { query: r#"a ("b"#.into() });
    let err = c.error.unwrap();
    assert_eq!((err.kind.as_str(), err.span.start, err.span.end), ("unclosed_quote", 3, 5));
    let c = t.ok::<SearchCheck>(QueryText { query: "a  AND (b OR  c)".into() });
    assert_eq!(c.error, None);
    assert_eq!(c.normalized.as_deref(), Some("a (b OR c)"));
    let e = t.err::<SearchIdsMethod>(QueryText { query: "seviye>=X".into() });
    assert_eq!(e.details["kind"], "unknown_option");
}

pub(super) fn saved_queries_crud_undo(f: Factory) {
    let t = TestApp::new(f());
    // Undo is a new modification: compare without `updated_at`.
    let list = |t: &TestApp| {
        let mut v = t.ok::<SavedQueriesList>(Empty {}).items;
        for q in &mut v {
            q.updated_at = Timestamp(0);
        }
        v
    };
    let a = t.ok::<SavedQueriesCreate>(CreateSavedQuery {
        name: "  Zor   fiiller ".into(),
        query: "tag:fiil is:due".into(),
        icon: Some("star".into()),
        position: None,
    });
    assert_eq!(a.name, "Zor fiiller");
    let b = t.ok::<SavedQueriesCreate>(CreateSavedQuery {
        name: "Yeni".into(),
        query: "is:new".into(),
        icon: None,
        position: None,
    });
    assert_eq!(b.position, a.position + 1);
    let (mut a0, mut b0) = (a.clone(), b.clone());
    a0.updated_at = Timestamp(0);
    b0.updated_at = Timestamp(0);
    let e = t.err::<SavedQueriesCreate>(CreateSavedQuery {
        name: "Bozuk".into(),
        query: "(is:new".into(),
        icon: None,
        position: None,
    });
    assert_eq!((e.code, e.details["kind"].as_str()), (ErrorCode::ValidationQuery, Some("unclosed_paren")));
    let e = t.err::<SavedQueriesCreate>(CreateSavedQuery {
        name: " ".into(),
        query: "".into(),
        icon: None,
        position: None,
    });
    assert_eq!(e.code, ErrorCode::ValidationSavedQueryName);
    let upd = t.ok::<SavedQueriesUpdate>(UpdateSavedQuery {
        id: a.id,
        name: "Zor".into(),
        query: "tag:fiil".into(),
        icon: None,
        position: Some(5),
    });
    assert_eq!((upd.name.as_str(), upd.position), ("Zor", 5));
    undo(&t);
    assert_eq!(list(&t)[0], a0);
    t.ok::<SavedQueriesDelete>(SavedQueryRef { id: a.id });
    assert_eq!(list(&t), vec![b0.clone()]);
    undo(&t);
    assert_eq!(list(&t), vec![a0.clone(), b0.clone()]);
    redo(&t);
    assert_eq!(list(&t), vec![b0]);
    let e = t.err::<SavedQueriesDelete>(SavedQueryRef { id: a.id });
    assert_eq!(e.code, ErrorCode::NotFoundSavedQuery);
    // Internal inverse is not callable from outside.
    let e = t.err::<SavedQueriesRestore>(RestoreSavedQuery { query: a });
    assert_eq!(e.code, ErrorCode::PermissionDenied);
}

pub(super) fn bulk_attribute_skips_inapplicable_and_restores(f: Factory) {
    let t = TestApp::new(f());
    let en = deck(&t, "İngilizce");
    let math = deck(&t, "Matematik");
    let lvl = level(&t, Some(en));
    let n1 = note(&t, en, "one", &[]);
    let n2 = note(&t, math, "two", &[]);
    let cards: Vec<CardId> = [&n1, &n2].iter().map(|n| n.cards[0].id).collect();
    let r = t.ok::<BulkSetAttribute>(SetAttributeBulk {
        card_ids: cards.clone(),
        attr_id: lvl.id,
        value: Some(AttributeValue::Choice(option(&lvl, "A2"))),
    });
    assert_eq!((r.changed, r.skipped), (1, 1));
    assert_eq!(search(&t, "seviye:A2").total, 1);
    // Clearing is one undo step too.
    let r = t.ok::<BulkSetAttribute>(SetAttributeBulk {
        card_ids: cards.clone(),
        attr_id: lvl.id,
        value: None,
    });
    assert_eq!(r.changed, 1);
    assert_eq!(search(&t, "seviye:*").total, 0);
    undo(&t);
    assert_eq!(search(&t, "seviye:A2").total, 1);
    // Invalid values are rejected.
    let e = t.err::<BulkSetAttribute>(SetAttributeBulk {
        card_ids: cards,
        attr_id: lvl.id,
        value: Some(AttributeValue::Number(3.0)),
    });
    assert_eq!(e.code, ErrorCode::ValidationAttributeValue);
}

pub(super) fn find_replace_and_tags(f: Factory) {
    let t = TestApp::new(f());
    let d = deck(&t, "D");
    let n = note(&t, d, "Elma elma ELMA", &["meyve::kırmızı", "a"]);
    let one = vec![n.id];
    let r = t.ok::<BulkFindReplace>(FindReplace {
        note_ids: one.clone(),
        find: "elma".into(),
        replace: "armut".into(),
        field: None,
        regex: None,
        match_case: Some(true),
    });
    assert_eq!(r.changed, 1);
    let got = t.ok::<NotesGet>(NoteRef { id: n.id });
    assert_eq!(got.fields["front"], "Elma armut ELMA");
    assert_eq!(search(&t, "armut").total, 1, "search index follows");
    t.ok::<BulkFindReplace>(FindReplace {
        note_ids: one.clone(),
        find: r"(\w+) armut".into(),
        replace: "$1-armut".into(),
        field: Some("front".into()),
        regex: Some(true),
        match_case: None,
    });
    assert_eq!(t.ok::<NotesGet>(NoteRef { id: n.id }).fields["front"], "Elma-armut ELMA");
    let e = t.err::<BulkFindReplace>(FindReplace {
        note_ids: one.clone(),
        find: "(".into(),
        replace: "".into(),
        field: None,
        regex: Some(true),
        match_case: None,
    });
    assert_eq!(e.code, ErrorCode::ValidationFindReplace);
    t.ok::<BulkRemoveTags>(NoteTags { note_ids: one.clone(), tags: vec!["meyve".into()] });
    let tags = t.ok::<NotesGet>(NoteRef { id: n.id }).tags;
    assert_eq!(tags.iter().map(|t| t.as_str()).collect::<Vec<_>>(), ["a"]);
    t.ok::<BulkAddTags>(NoteTags { note_ids: one.clone(), tags: vec!["A".into(), "b".into()] });
    let tags = t.ok::<NotesGet>(NoteRef { id: n.id }).tags;
    assert_eq!(tags.iter().map(|t| t.as_str()).collect::<Vec<_>>(), ["a", "b"]);
    undo(&t);
    undo(&t);
    let tags = t.ok::<NotesGet>(NoteRef { id: n.id }).tags;
    assert_eq!(tags.iter().map(|t| t.as_str()).collect::<Vec<_>>(), ["meyve::kırmızı", "a"]);
}

/// v0.4 acceptance: "Komut paleti kayıt defterindeki tüm komutları listeler".
pub(super) fn palette_lists_every_registry_command(f: Factory) {
    let t = TestApp::new(f());
    let palette = t.ok::<PaletteListMethod>(Empty {});
    let listed: Vec<&str> = palette.entries.iter().map(|e| e.method.as_str()).collect();
    let mut expected: Vec<String> = t
        .app
        .registry()
        .methods()
        .filter(|d| d.visibility == Visibility::Public)
        .map(|d| d.qualified_name())
        .collect();
    expected.sort();
    assert_eq!(listed, expected.iter().map(String::as_str).collect::<Vec<_>>());
    assert!(listed.len() > 60);
    let get = |m: &str| palette.entries.iter().find(|e| e.method == m).unwrap();
    assert!(get("v1.oplog.undo").direct);
    assert!(get("v1.browse.repair_database").direct);
    assert!(!get("v1.decks.create").direct);
    assert_eq!(get("v1.decks.create").label_key, "commands.decks.create");
    assert!(get("v1.decks.create").enabled);
    assert!(!listed.contains(&"v1.bulk.restore_cards"), "internal methods are not offered");
    // Shortcuts show up in the palette.
    t.ok::<ShortcutsSet>(SetShortcut { action: "v1.decks.create".into(), combo: Some("ctrl+shift+d".into()) });
    let palette = t.ok::<PaletteListMethod>(Empty {});
    let e = palette.entries.iter().find(|e| e.method == "v1.decks.create").unwrap();
    assert_eq!(e.shortcut.as_deref(), Some("Ctrl+Shift+D"));
}

pub(super) fn shortcuts_set_conflict_reset_undo(f: Factory) {
    let t = TestApp::new(f());
    let list = t.ok::<ShortcutsList>(Empty {});
    let combo = |l: &ShortcutList, a: &str| l.items.iter().find(|i| i.action == a).unwrap().combo.clone();
    assert_eq!(combo(&list, "ui.palette").as_deref(), Some("Ctrl+K"));
    assert!(list.conflicts.is_empty());
    // Global Ctrl+K conflicts with everything.
    let e = t.err::<ShortcutsSet>(SetShortcut { action: "browse.info".into(), combo: Some("ctrl+k".into()) });
    assert_eq!((e.code, e.details["action"].as_str()), (ErrorCode::ConflictShortcut, Some("ui.palette")));
    let e = t.err::<ShortcutsSet>(SetShortcut { action: "browse.info".into(), combo: Some("hyper+q".into()) });
    assert_eq!(e.code, ErrorCode::ValidationShortcut);
    let e = t.err::<ShortcutsSet>(SetShortcut { action: "nope".into(), combo: None });
    assert_eq!(e.code, ErrorCode::NotFoundShortcutAction);
    let l = t.ok::<ShortcutsSet>(SetShortcut { action: "ui.palette".into(), combo: Some("Ctrl+P".into()) });
    assert_eq!(combo(&l, "ui.palette").as_deref(), Some("Ctrl+P"));
    let l = t.ok::<ShortcutsSet>(SetShortcut { action: "browse.info".into(), combo: Some("ctrl+k".into()) });
    assert_eq!(combo(&l, "browse.info").as_deref(), Some("Ctrl+K"));
    let l = t.ok::<ShortcutsSet>(SetShortcut { action: "browse.delete".into(), combo: None });
    assert_eq!(combo(&l, "browse.delete"), None);
    undo(&t);
    assert_eq!(combo(&t.ok::<ShortcutsList>(Empty {}), "browse.delete").as_deref(), Some("Delete"));
    let l = t.ok::<ShortcutsReset>(ResetShortcut { action: None });
    assert_eq!(combo(&l, "ui.palette").as_deref(), Some("Ctrl+K"));
    assert_eq!(combo(&l, "browse.info").as_deref(), Some("Ctrl+I"));
    undo(&t);
    let l = t.ok::<ShortcutsList>(Empty {});
    assert_eq!(combo(&l, "ui.palette").as_deref(), Some("Ctrl+P"));
    assert_eq!(combo(&l, "browse.info").as_deref(), Some("Ctrl+K"));
    // Resetting one action may surface a conflict, which the list reports.
    let l = t.ok::<ShortcutsReset>(ResetShortcut { action: Some("ui.palette".into()) });
    assert_eq!(l.conflicts.len(), 1);
    assert_eq!(l.conflicts[0].combo, "Ctrl+K");
}

pub(super) fn card_info_duplicates_trash(f: Factory) {
    let t = TestApp::new(f());
    let d = deck(&t, "Dil::İngilizce");
    let a = note(&t, d, "apple", &["meyve"]);
    let b = note(&t, d, "Apple", &[]);
    note(&t, d, "pear", &[]);
    let info = t.ok::<BrowseCardInfo>(CardInfoRef { card_id: a.cards[0].id });
    assert_eq!(info.card.deck_path, "Dil::İngilizce");
    assert_eq!(info.note.id, a.id);
    assert_eq!(info.retrievability, None);
    assert!(info.reviews.is_empty());
    let dups = t.ok::<BrowseDuplicates>(Empty {});
    assert_eq!(dups.groups.len(), 1);
    let mut g = dups.groups[0].note_ids.clone();
    g.sort();
    let mut want = vec![a.id, b.id];
    want.sort();
    assert_eq!(g, want);
    t.ok::<NotesDelete>(NoteIds { ids: vec![b.id] });
    assert!(t.ok::<BrowseDuplicates>(Empty {}).groups.is_empty());
    let sub = deck(&t, "Çöp::Alt");
    let parent = t.ok::<DecksList>(Empty {}).decks.into_iter().find(|x| x.name == "Çöp").unwrap();
    note(&t, sub, "gone", &[]);
    t.ok::<DecksDelete>(DeckRef { id: parent.id });
    let trash = t.ok::<BrowseTrash>(ListTrash { limit: None });
    assert_eq!(trash.decks.iter().map(|x| x.path.as_str()).collect::<Vec<_>>(), ["Çöp"]);
    let trashed: Vec<NoteId> = trash.notes.iter().map(|n| n.id).collect();
    assert_eq!(trashed.len(), 2);
    assert!(trashed.contains(&b.id));
    assert_eq!(search(&t, "").total, 2, "trashed cards never match");
}

pub(super) fn database_check_and_repair_undo(f: Factory) {
    let t = TestApp::new(f());
    let d = deck(&t, "D");
    let n = note(&t, d, "x", &[]);
    let m = note(&t, d, "y", &[]);
    let r = t.ok::<BrowseCheckDatabase>(Empty {});
    assert!(r.healthy, "{r:?}");
    // Corrupt: a live card of a trashed note, a review card without due.
    {
        let mut tx = t.store.write().unwrap();
        tx.set_notes_deleted(&[n.id], Some(Timestamp(5))).unwrap();
        let mut c = tx.get_card(m.cards[0].id).unwrap().unwrap();
        c.scheduling.state = CardState::Review;
        tx.update_card(&c).unwrap();
        tx.commit().unwrap();
    }
    let r = t.ok::<BrowseCheckDatabase>(Empty {});
    assert!(!r.healthy);
    assert_eq!(r.orphan_cards, vec![n.cards[0].id]);
    assert_eq!(r.unscheduled_cards, vec![m.cards[0].id]);
    let before = state(&t);
    let fixed = t.ok::<BrowseRepairDatabase>(Empty {});
    assert_eq!((fixed.trashed_cards, fixed.rescheduled_cards), (1, 1));
    assert!(t.ok::<BrowseCheckDatabase>(Empty {}).healthy);
    undo(&t);
    assert_eq!(state(&t), before);
}

pub(super) fn module_can_be_disabled(f: Factory) {
    let t = TestApp::new(f());
    t.ok::<ModulesSetEnabled>(SetModuleEnabled { id: "browse".into(), enabled: false });
    let e = t.err::<SearchCardsMethod>(SearchCards {
        query: String::new(),
        sort: None,
        descending: None,
        notes_only: None,
        offset: None,
        limit: None,
    });
    assert_eq!(e.code, ErrorCode::ConflictModuleDisabled);
    let modules = t.ok::<ModulesList>(Empty {}).modules;
    let browse = modules.iter().find(|m| m.id == "browse").unwrap();
    assert!(!browse.enabled && browse.ui.is_empty());
}
