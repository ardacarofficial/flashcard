//! v1.1 screen queries: `decks.summaries` (Home / Library progress, chips, recent decks) and
//! `notes.terms` (the deck page's term list).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::testing::TestApp;
use fc_api::v1::attributes::{AttributeAssignment, DefineAttribute, NewAttributeOption};
use fc_api::v1::cardtypes::SetCardFlag;
use fc_api::v1::common::Empty;
use fc_api::v1::decks::{CreateDeck, DeckDto, DeckSummary};
use fc_api::v1::methods::*;
use fc_api::v1::notes::{CreateNote, ListNotes, NoteDto};
use fc_core::attribute::{AttributeKind, AttributeValue};
use fc_core::card::CardState;
use fc_core::paging::PageRequest;
use fc_core::stats::Mastery;
use fc_core::time::MILLIS_PER_DAY;
use fc_core::{AttributeDefId, AttributeOptionId, DeckId, Timestamp};

fn deck(t: &TestApp, path: &str) -> DeckDto {
    t.ok::<DecksCreate>(CreateDeck {
        path: path.into(),
        parent_id: None,
        description: None,
        icon: None,
        color: None,
    })
}

fn note(
    t: &TestApp,
    deck: DeckId,
    front: &str,
    back: &str,
    level: Option<(AttributeDefId, AttributeOptionId)>,
) -> NoteDto {
    t.ok::<NotesCreate>(CreateNote {
        notetype_id: fc_core::notetype::builtin::CLASSIC_ID,
        deck_id: deck,
        fields: [("front", front), ("back", back)]
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect(),
        tags: vec![],
        attributes: level
            .map(|(attr_id, o)| AttributeAssignment {
                attr_id,
                value: Some(AttributeValue::Choice(o)),
            })
            .into_iter()
            .collect(),
        allow_duplicate: Some(true),
    })
}

/// Puts the card of `n` into `state`, last reviewed at `last` with an interval of `days`.
fn schedule(t: &TestApp, n: &NoteDto, state: CardState, last: i64, days: i64) {
    let mut tx = t.store.write().unwrap();
    let mut card = tx.get_card(n.cards[0].id).unwrap().unwrap();
    card.scheduling.state = state;
    card.scheduling.last_review = Some(Timestamp(last));
    card.scheduling.due = Some(Timestamp(last + days * MILLIS_PER_DAY));
    tx.update_card(&card).unwrap();
    tx.commit().unwrap();
}

struct World {
    t: TestApp,
    lang: DeckDto,
    verbs: DeckDto,
    other: DeckDto,
    level: AttributeDefId,
    notes: Vec<NoteDto>,
}

fn world() -> World {
    let t = TestApp::mem();
    let lang = deck(&t, "Lang");
    let verbs = deck(&t, "Lang::Verbs");
    let other = deck(&t, "Other");
    let def = t.ok::<AttributesDefine>(DefineAttribute {
        scope_deck_id: None,
        key: None,
        name: "Seviye".into(),
        kind: AttributeKind::OrderedEnum,
        target: None,
        required: None,
        options: ["A1", "B1"]
            .iter()
            .map(|l| NewAttributeOption {
                label: (*l).into(),
                color: None,
            })
            .collect(),
    });
    let opt = |l: &str| {
        (
            def.id,
            def.options.iter().find(|o| o.label == l).unwrap().id,
        )
    };
    let notes = vec![
        note(&t, verbs.id, "go", "<b>gitmek</b>", Some(opt("B1"))),
        note(&t, verbs.id, "come", "gelmek", Some(opt("B1"))),
        note(&t, lang.id, "dog", "köpek", Some(opt("A1"))),
        note(&t, other.id, "x", "y", None),
    ];
    // go: mastered (studied at 1000), come: learning (studied at 5000), dog and x: new.
    schedule(&t, &notes[0], CardState::Review, 1_000, 30);
    schedule(&t, &notes[1], CardState::Review, 5_000, 3);
    World {
        t,
        lang,
        verbs,
        other,
        level: def.id,
        notes,
    }
}

fn summary_of(list: &[DeckSummary], id: DeckId) -> &DeckSummary {
    list.iter().find(|s| s.deck_id == id).unwrap()
}

#[test]
fn deck_summaries_count_mastery_chips_and_recent_decks() {
    let w = world();
    let s = w.t.ok::<DecksSummaries>(Empty {});
    // Tree order.
    let order: Vec<DeckId> = s.decks.iter().map(|d| d.deck_id).collect();
    assert_eq!(order, vec![w.lang.id, w.verbs.id, w.other.id]);

    let verbs = summary_of(&s.decks, w.verbs.id);
    assert_eq!((verbs.new, verbs.learning, verbs.mastered), (0, 1, 1));
    assert_eq!(verbs.mastered_pct, 50);
    assert_eq!(verbs.last_studied_at, Some(Timestamp(5_000)));
    assert_eq!(verbs.chips.len(), 1);
    assert_eq!(
        (verbs.chips[0].label.as_str(), verbs.chips[0].notes),
        ("B1", 2)
    );
    assert_eq!(verbs.chips[0].attr_id, w.level);

    // A parent counts its sub-decks; chips most notes first.
    let lang = summary_of(&s.decks, w.lang.id);
    assert_eq!((lang.new, lang.learning, lang.mastered), (1, 1, 1));
    assert_eq!(lang.mastered_pct, 33);
    let chips: Vec<(&str, u32)> = lang
        .chips
        .iter()
        .map(|c| (c.label.as_str(), c.notes))
        .collect();
    assert_eq!(chips, vec![("B1", 2), ("A1", 1)]);

    let other = summary_of(&s.decks, w.other.id);
    assert_eq!(
        (other.new, other.mastered_pct, other.last_studied_at),
        (1, 0, None)
    );
    assert!(other.chips.is_empty());

    // Recent: decks whose own cards were studied, most recent first (Lang has none of its own).
    assert_eq!(s.recent, vec![w.verbs.id]);
    // Today's collection counts equal the sum of the tree's top-level counts.
    let tree = w.t.ok::<DecksTree>(Empty {});
    let roots = tree.roots.iter().fold((0, 0, 0, 0), |a, r| {
        (
            a.0 + r.counts.new,
            a.1 + r.counts.learning,
            a.2 + r.counts.due,
            a.3 + r.counts.total,
        )
    });
    assert_eq!(
        (s.today.new, s.today.learning, s.today.due, s.today.total),
        roots
    );
    assert_eq!(s.today.total, 4);
}

#[test]
fn note_terms_give_both_sides_mastery_flags_and_chips() {
    let w = world();
    let req = |deck: DeckId| ListNotes {
        deck_id: Some(deck),
        include_subdecks: None,
        page: PageRequest::default(),
    };
    let page = w.t.ok::<NotesTerms>(req(w.lang.id));
    assert_eq!(page.items.len(), 3, "sub-decks included");
    let go = page
        .items
        .iter()
        .find(|r| r.note_id == w.notes[0].id)
        .unwrap();
    assert_eq!(go.front.key, "front");
    assert_eq!(go.front.text, "go");
    let back = go.back.as_ref().unwrap();
    assert_eq!(
        (back.value.as_str(), back.text.as_str()),
        ("<b>gitmek</b>", "gitmek")
    );
    assert_eq!(go.mastery, Mastery::Mastered);
    assert_eq!(go.card_ids, vec![w.notes[0].cards[0].id]);
    assert_eq!(go.flags, 0);
    assert_eq!(go.chips.len(), 1);
    assert_eq!(
        (go.chips[0].name.as_str(), go.chips[0].label.as_str()),
        ("Seviye", "B1")
    );

    w.t.ok::<CardsSetFlag>(SetCardFlag {
        ids: go.card_ids.clone(),
        flag: 2,
        on: true,
    });
    let page = w.t.ok::<NotesTerms>(req(w.verbs.id));
    assert_eq!(page.items.len(), 2);
    let go = page
        .items
        .iter()
        .find(|r| r.note_id == w.notes[0].id)
        .unwrap();
    assert_eq!(go.flags, 0b10);
    let come = page
        .items
        .iter()
        .find(|r| r.note_id == w.notes[1].id)
        .unwrap();
    assert_eq!(come.mastery, Mastery::Learning);

    let other = w.t.ok::<NotesTerms>(req(w.other.id));
    assert_eq!(other.items.len(), 1);
    assert_eq!(other.items[0].mastery, Mastery::New);
    assert!(other.items[0].chips.is_empty());
}
