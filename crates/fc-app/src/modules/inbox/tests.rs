//! Service tests of the `inbox`, `deep_links` and `clipboard` modules, each run against the
//! in-memory store and SQLite (in memory and on a temp file).
//!
//! v0.9 acceptance, service side: items proposed from outside carry classification values
//! (`Seviye = B1`) resolved against the deck on accept; accepting is one undo step (notes
//! created ↔ items pending again); deep links never write a note directly. The HTTP path
//! ("a word from the browser extension lands in the inbox") is proven end to end in
//! `fc-http/tests/server.rs`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::testing::{MemStore, TempSqlite, TestApp};
use fc_api::v1::attributes::*;
use fc_api::v1::common::Empty;
use fc_api::v1::decks::*;
use fc_api::v1::events::DomainEvent;
use fc_api::v1::inbox::*;
use fc_api::v1::methods::*;
use fc_api::v1::modules::SetModuleEnabled;
use fc_api::v1::notes::NoteRef;
use fc_api::v1::oplog::{Actor, ListOplog};
use fc_api::v1::search::SearchCards;
use fc_api::ErrorCode;
use fc_core::attribute::AttributeKind;
use fc_core::paging::PageRequest;
use fc_core::DeckId;
use fc_storage::{SqliteStore, Store};
use serde_json::json;
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
    classified_items_are_accepted_as_one_undo_step,
    accept_is_all_or_nothing_with_item_index,
    reject_and_undo,
    proposals_are_validated_and_sourced_by_the_actor,
    deep_links_navigate_or_propose_never_write_notes,
    clipboard_suggestions_are_opt_in_and_skip_secrets,
    inbox_module_can_be_switched_off,
);

// ------------------------------------------------------------------ helpers

pub(crate) fn deck(t: &TestApp, path: &str) -> DeckId {
    t.ok::<DecksCreate>(CreateDeck {
        path: path.into(),
        parent_id: None,
        description: None,
        icon: None,
        color: None,
    })
    .id
}

/// `İngilizce` with "Seviye" A1–C2 and its sub-deck `İngilizce::Fiiller` (inherits it).
pub(crate) fn english(t: &TestApp) -> (DeckId, DeckId) {
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

fn word(front: &str, deck: Option<DeckId>, level: Option<&str>) -> AddInboxItem {
    AddInboxItem {
        front: Some(front.into()),
        back: Some(format!("{front} (tr)")),
        deck_id: deck,
        attributes: level
            .map(|l| BTreeMap::from([("Seviye".to_owned(), json!(l))]))
            .unwrap_or_default(),
        context: Some(InboxContext {
            url: Some("https://example.com/a".into()),
            title: Some("A page".into()),
            sentence: Some(format!("The {front} dug a hole.")),
        }),
        ..AddInboxItem::default()
    }
}

fn pending(t: &TestApp) -> Vec<InboxItemDto> {
    t.ok::<InboxList>(ListInbox::default()).items
}

fn total(t: &TestApp, query: &str) -> u32 {
    t.ok::<SearchCardsMethod>(SearchCards {
        query: query.into(),
        sort: None,
        descending: None,
        notes_only: None,
        offset: None,
        limit: None,
    })
    .total
}

fn undo(t: &TestApp) {
    t.ok::<OplogUndo>(Empty {});
}

fn redo(t: &TestApp) {
    t.ok::<OplogRedo>(Empty {});
}

fn accept_one(id: fc_core::InboxItemId) -> AcceptInboxItem {
    AcceptInboxItem {
        id,
        deck_id: None,
        notetype_id: None,
        fields: None,
        tags: None,
        attributes: None,
    }
}

// ------------------------------------------------------------------ tests

pub(super) fn classified_items_are_accepted_as_one_undo_step(f: Factory) {
    let t = TestApp::new(f());
    let (_, verbs) = english(&t);
    let a = t.ok::<InboxAdd>(word("badger", Some(verbs), Some("B1")));
    // The classification is matched by key, name or id; the option by label (any case).
    let mut b = word("otter", None, None);
    b.attributes = BTreeMap::from([("seviye".to_owned(), json!("a2"))]);
    let b = t.ok::<InboxAdd>(b);
    let c = t.ok::<InboxAdd>(word("mole", Some(verbs), None));
    assert_eq!(a.status, InboxStatus::Pending);
    assert_eq!(a.source, InboxSource::User);
    assert_eq!(a.item.context.as_ref().unwrap().title.as_deref(), Some("A page"));
    assert_eq!(pending(&t).len(), 3);
    assert!(t
        .take_events()
        .iter()
        .any(|e| e.event == DomainEvent::InboxChanged { pending: 3 }));

    // Review: b gets its deck here, a keeps its own.
    let mut with_deck = accept_one(b.id);
    with_deck.deck_id = Some(verbs);
    let r = t.ok::<InboxAccept>(AcceptInbox {
        items: vec![accept_one(a.id), with_deck],
        allow_duplicates: None,
    });
    assert_eq!(r.accepted.len(), 2);
    let note = t.ok::<NotesGet>(NoteRef {
        id: r.accepted[0].note_id,
    });
    // v1.3 (deliberate change): an item with a captured sentence and no note type becomes a
    // Subtitle card, which keeps the sentence (Classic dropped it).
    assert_eq!(note.notetype_id, fc_core::content::builtin::find("subtitle").unwrap().id);
    assert_eq!(note.fields["word"], "badger");
    assert_eq!(note.fields["meaning"], "badger (tr)");
    assert_eq!(note.fields["sentence"], "The <b>badger</b> dug a hole.");
    assert_eq!(note.home_deck_id, verbs);
    assert_eq!(total(&t, "seviye:B1"), 1);
    assert_eq!(total(&t, "seviye:A2"), 1);
    assert_eq!(total(&t, "seviye>=A2 deck:İngilizce"), 2);
    let list = t.ok::<InboxList>(ListInbox::default());
    assert_eq!(list.items.iter().map(|i| i.id).collect::<Vec<_>>(), [c.id]);
    assert_eq!(list.pending, 1);
    let done = t.ok::<InboxList>(ListInbox {
        status: Some(InboxStatus::Accepted),
        page: PageRequest::default(),
    });
    assert_eq!(done.items.len(), 2);
    assert!(done.items.iter().all(|i| i.note_ids.len() == 1));

    // One undo step: the notes are gone, the items pending again, exactly as before.
    let before_accept = {
        let mut v = vec![a.clone(), b.clone()];
        v.sort_by_key(|i| i.id);
        v
    };
    undo(&t);
    assert_eq!(total(&t, ""), 0);
    let mut back: Vec<_> = pending(&t)
        .into_iter()
        .filter(|i| i.id != c.id)
        .collect();
    back.sort_by_key(|i| i.id);
    assert_eq!(back, before_accept);
    redo(&t);
    assert_eq!(total(&t, "seviye:B1"), 1);
    assert_eq!(pending(&t).len(), 1);

    // Each add is its own undo step (the user can take a proposal back).
    undo(&t);
    undo(&t);
    assert_eq!(pending(&t).len(), 2);
    assert_eq!(t.err::<InboxAccept>(AcceptInbox {
        items: vec![accept_one(c.id)],
        allow_duplicates: None,
    }).code, ErrorCode::NotFoundInboxItem);
}

pub(super) fn accept_is_all_or_nothing_with_item_index(f: Factory) {
    let t = TestApp::new(f());
    let (_, verbs) = english(&t);
    let a = t.ok::<InboxAdd>(word("badger", Some(verbs), Some("B1")));
    let no_deck = t.ok::<InboxAdd>(word("otter", None, None));
    let bad_level = t.ok::<InboxAdd>(word("mole", Some(verbs), Some("Z9")));
    let oplog_before = t.ok::<OplogList>(ListOplog::default()).items.len();

    let e = t.err::<InboxAccept>(AcceptInbox {
        items: vec![accept_one(a.id), accept_one(no_deck.id)],
        allow_duplicates: None,
    });
    assert_eq!(e.code, ErrorCode::ValidationInboxItem);
    assert_eq!(e.details["item_index"], 1);
    assert_eq!(e.details["field"], "deck");
    let e = t.err::<InboxAccept>(AcceptInbox {
        items: vec![accept_one(a.id), accept_one(bad_level.id)],
        allow_duplicates: None,
    });
    assert_eq!(e.code, ErrorCode::NotFoundAttributeOption);
    assert_eq!(e.details["item_index"], 1);
    // Nothing happened: no note, every item still pending, no undo step.
    assert_eq!(total(&t, ""), 0);
    assert_eq!(pending(&t).len(), 3);
    assert_eq!(
        t.ok::<OplogList>(ListOplog::default()).items.len(),
        oplog_before
    );

    // A duplicate is refused unless allowed.
    t.ok::<InboxAccept>(AcceptInbox {
        items: vec![accept_one(a.id)],
        allow_duplicates: None,
    });
    let again = t.ok::<InboxAdd>(word("badger", Some(verbs), None));
    let e = t.err::<InboxAccept>(AcceptInbox {
        items: vec![accept_one(again.id)],
        allow_duplicates: None,
    });
    assert_eq!(e.code, ErrorCode::ConflictNoteDuplicate);
    assert_eq!(e.details["item_index"], 0);
    t.ok::<InboxAccept>(AcceptInbox {
        items: vec![accept_one(again.id)],
        allow_duplicates: Some(true),
    });
    // Accepting twice is a state conflict; an item listed twice is refused.
    let e = t.err::<InboxAccept>(AcceptInbox {
        items: vec![accept_one(a.id)],
        allow_duplicates: None,
    });
    assert_eq!(e.code, ErrorCode::ConflictInboxItemState);
    let e = t.err::<InboxReject>(InboxItemIds {
        ids: vec![no_deck.id, no_deck.id],
    });
    assert_eq!(e.code, ErrorCode::ValidationInboxItem);
}

pub(super) fn reject_and_undo(f: Factory) {
    let t = TestApp::new(f());
    let a = t.ok::<InboxAdd>(word("badger", None, None));
    let b = t.ok::<InboxAdd>(word("otter", None, None));
    let r = t.ok::<InboxReject>(InboxItemIds {
        ids: vec![a.id, b.id],
    });
    assert_eq!(r.changed, 2);
    assert!(pending(&t).is_empty());
    assert_eq!(t.ok::<InboxCountMethod>(Empty {}).pending, 0);
    let rejected = t.ok::<InboxList>(ListInbox {
        status: Some(InboxStatus::Rejected),
        page: PageRequest::default(),
    });
    assert_eq!(rejected.items.len(), 2);
    undo(&t);
    let mut back = pending(&t);
    back.sort_by_key(|i| i.id);
    assert_eq!(back, vec![a, b]);
    redo(&t);
    assert!(pending(&t).is_empty());
}

pub(super) fn proposals_are_validated_and_sourced_by_the_actor(f: Factory) {
    let t = TestApp::new(f());
    let empty = AddInboxItem {
        front: Some("   ".into()),
        ..AddInboxItem::default()
    };
    let e = t.err::<InboxAdd>(empty);
    assert_eq!(e.code, ErrorCode::ValidationInboxItem);
    assert_eq!(e.details["field"], "content");
    let long = AddInboxItem {
        front: Some("x".repeat(super::MAX_FIELD_CHARS + 1)),
        ..AddInboxItem::default()
    };
    assert_eq!(t.err::<InboxAdd>(long).details["field"], "field");
    let mut bad_attr = word("a", None, None);
    bad_attr.attributes.insert("Seviye".into(), json!({"x": 1}));
    assert_eq!(t.err::<InboxAdd>(bad_attr).details["field"], "attributes");
    let mut ghost = word("a", None, None);
    ghost.deck_id = Some(DeckId(uuid::Uuid::from_u128(9)));
    assert_eq!(t.err::<InboxAdd>(ghost).code, ErrorCode::NotFoundDeck);
    let mut too_many = word("a", None, None);
    too_many.tags = (0..=super::MAX_TAGS).map(|i| format!("t{i}")).collect();
    assert_eq!(t.err::<InboxAdd>(too_many).details["field"], "tags");

    // The app says where it got it; nobody else may.
    let mut clip = word("badger", None, None);
    clip.via = Some(InboxVia::Clipboard);
    assert_eq!(t.ok::<InboxAdd>(clip.clone()).source, InboxSource::Clipboard);
    let e = t
        .app
        .call::<InboxAdd>(&clip, &Actor::Mcp("claude".into()))
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::PermissionDenied, "MCP has no inbox tool");
    let cli = t
        .app
        .call::<InboxAdd>(&word("otter", None, None), &Actor::Cli)
        .unwrap();
    assert_eq!(
        cli.source,
        InboxSource::Other {
            actor: "cli".into(),
            id: None
        }
    );
    let e = t.app.call::<InboxAdd>(&clip, &Actor::Cli).unwrap_err();
    assert_eq!(e.details["field"], "via");
    // A stored item never keeps `via`, and only local API clients keep `client_ref`.
    let mut r = word("mole", None, None);
    r.client_ref = Some("abc".into());
    let stored = t.ok::<InboxAdd>(r);
    assert_eq!(stored.item.client_ref, None);
    assert_eq!(stored.item.via, None);
}

pub(super) fn deep_links_navigate_or_propose_never_write_notes(f: Factory) {
    let t = TestApp::new(f());
    let (_, verbs) = english(&t);
    let open = |url: String| t.app.call::<DeepLinksOpen>(&OpenDeepLink { url }, &Actor::User);
    let steps = || t.ok::<OplogList>(ListOplog::default()).items.len();
    let before = steps();

    let nav = open(format!("flashcard://deck/{verbs}")).unwrap();
    assert_eq!(
        nav,
        DeepLinkOutcome::Navigate {
            target: DeepLinkTarget::Deck { deck_id: verbs }
        }
    );
    let study = open(format!("flashcard://study?deck={verbs}&query=seviye%3E%3DB1")).unwrap();
    assert_eq!(
        study,
        DeepLinkOutcome::Navigate {
            target: DeepLinkTarget::Study {
                deck_id: Some(verbs),
                query: Some("seviye>=B1".into())
            }
        }
    );
    assert_eq!(steps(), before, "navigation writes nothing");
    let e = open(format!("flashcard://deck/{}", DeckId(uuid::Uuid::from_u128(5)))).unwrap_err();
    assert_eq!(e.code, ErrorCode::NotFoundDeck);
    let e = open("flashcard://study?query=seviye%3E%3D".into()).unwrap_err();
    assert_eq!(e.code, ErrorCode::ValidationQuery);
    for (url, reason) in [
        ("https://x/deck/1", "scheme"),
        ("flashcard://deck/not-an-id", "id"),
        ("flashcard://wipe", "target"),
        ("flashcard://import?url=https%3A%2F%2Fx", "unsupported"),
        ("flashcard://add?deck=x", "missing"),
    ] {
        let e = open(url.into()).unwrap_err();
        assert_eq!(e.code, ErrorCode::ValidationDeepLink, "{url}");
        assert_eq!(e.details["reason"], reason, "{url}");
    }

    // A writing link only proposes: an inbox item from `deep_link`, one undo step.
    let added = open(format!(
        "flashcard://add?deck={verbs}&front=porsuk&back=badger&tags=web&attr.Seviye=B1"
    ))
    .unwrap();
    let DeepLinkOutcome::Inbox { item } = added else {
        panic!("not an inbox outcome");
    };
    assert_eq!(item.source, InboxSource::DeepLink);
    assert_eq!(item.item.attributes["Seviye"], json!("B1"));
    assert_eq!(total(&t, ""), 0, "no note was written");
    t.ok::<InboxAccept>(AcceptInbox {
        items: vec![accept_one(item.id)],
        allow_duplicates: None,
    });
    assert_eq!(total(&t, "seviye:B1 tag:web"), 1);
    undo(&t);
    undo(&t);
    assert!(pending(&t).is_empty(), "undo takes the proposal back");

    t.ok::<ModulesSetEnabled>(SetModuleEnabled {
        id: "deep_links".into(),
        enabled: false,
    });
    let e = open(format!("flashcard://deck/{verbs}")).unwrap_err();
    assert_eq!(e.code, ErrorCode::ConflictModuleDisabled);
}

pub(super) fn clipboard_suggestions_are_opt_in_and_skip_secrets(f: Factory) {
    let t = TestApp::new(f());
    let check = |text: &str| {
        t.app.call::<ClipboardCheck>(&CheckClipboard { text: text.into() }, &Actor::User)
    };
    // Off by default: the shell does not even read the clipboard.
    assert!(!t.app.is_module_enabled("clipboard"));
    assert_eq!(
        check("porsuk").unwrap_err().code,
        ErrorCode::ConflictModuleDisabled
    );
    t.ok::<ModulesSetEnabled>(SetModuleEnabled {
        id: "clipboard".into(),
        enabled: true,
    });
    let v = check("  porsuk ").unwrap();
    assert!(v.suggest);
    assert_eq!(v.text, "porsuk");
    for (secret, reason) in [
        ("Tr0ub4dor&3", "secret"),
        ("sk-ant-api03-xyz", "secret"),
        ("4111 1111 1111 1111", "secret"),
        ("https://example.com", "not_text"),
        ("x", "too_short"),
    ] {
        let v = check(secret).unwrap();
        assert!(!v.suggest, "{secret}");
        assert_eq!(v.text, "");
        assert_eq!(v.reason.as_deref(), Some(reason), "{secret}");
    }
    // The limits are settings.
    t.ok::<SettingsSet>(fc_api::v1::settings::SetSetting {
        scope: fc_core::settings::SettingScope::App,
        key: super::super::clipboard::MAX_CHARS.into(),
        value: json!(10),
    });
    assert_eq!(check("a longer sentence").unwrap().reason.as_deref(), Some("too_long"));
}

pub(super) fn inbox_module_can_be_switched_off(f: Factory) {
    let t = TestApp::new(f());
    let a = t.ok::<InboxAdd>(word("badger", None, None));
    t.ok::<ModulesSetEnabled>(SetModuleEnabled {
        id: "inbox".into(),
        enabled: false,
    });
    assert_eq!(
        t.err::<InboxAdd>(word("otter", None, None)).code,
        ErrorCode::ConflictModuleDisabled
    );
    assert_eq!(
        t.err::<InboxList>(ListInbox::default()).code,
        ErrorCode::ConflictModuleDisabled
    );
    // Its menu entry goes away with it.
    let modules = t.ok::<ModulesList>(Empty {});
    let inbox = modules.modules.iter().find(|m| m.id == "inbox").unwrap();
    assert!(!inbox.enabled);
    t.ok::<ModulesSetEnabled>(SetModuleEnabled {
        id: "inbox".into(),
        enabled: true,
    });
    assert_eq!(pending(&t), vec![a]);
}
