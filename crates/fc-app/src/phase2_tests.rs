//! Phase-2 integration (v0.2 study × v0.3 card types), end to end through the API on the
//! in-memory store, SQLite in memory and SQLite on a temp file:
//!
//! * a cloze note and a classic note in a deck that inherits a "Seviye" classification;
//! * study renders every card through its card type engine (sanitized HTML), skips cards
//!   the note no longer yields (`empty`), shows flags with Anki numbering;
//! * typed-answer card types are judged by the v0.2 answer checkers, choice cards by their
//!   engine;
//! * answer → undo → the card, the review log and the session are exactly as before; redo
//!   re-applies exactly;
//! * a trial session leaves the `cards` table unchanged;
//! * the deck preset's daily limits cap a normal session.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::testing::{MemStore, TempSqlite, TestApp};
use fc_api::v1::attributes::{AttributeAssignment, DefineAttribute, NewAttributeOption};
use fc_api::v1::cardtypes::{CardInteraction, SetCardFlag};
use fc_api::v1::common::Empty;
use fc_api::v1::decks::{CreateDeck, DeckDto};
use fc_api::v1::methods::*;
use fc_api::v1::notes::{CreateNote, NoteDto, NoteRef, UpdateNote};
use fc_api::v1::study::*;
use fc_api::ErrorCode;
use fc_core::attribute::{AttributeKind, AttributeValue};
use fc_core::card::{Card, Rating};
use fc_core::content::builtin;
use fc_core::preset::PresetConfig;
use fc_core::{AttributeDefId, AttributeOptionId, CardId, DeckId, NoteTypeId};
use fc_storage::repo::CardFilter;
use fc_storage::{SqliteStore, Store};
use std::collections::BTreeMap;
use std::sync::Arc;

type Factory = dyn Fn() -> Arc<dyn Store>;

struct World {
    t: TestApp,
    parent: DeckDto,
    child: DeckDto,
    seviye: AttributeDefId,
    b1: AttributeOptionId,
    cloze: NoteDto,
    classic: NoteDto,
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

fn note(
    t: &TestApp,
    nt: NoteTypeId,
    deck: DeckId,
    fields: &[(&str, &str)],
    level: Option<(AttributeDefId, AttributeOptionId)>,
) -> NoteDto {
    t.ok::<NotesCreate>(CreateNote {
        notetype_id: nt,
        deck_id: deck,
        fields: fields
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

fn world(f: &Factory) -> World {
    let t = TestApp::new(f());
    let parent = deck(&t, "Dil");
    let child = deck(&t, "Dil::İngilizce");
    let def = t.ok::<AttributesDefine>(DefineAttribute {
        scope_deck_id: Some(parent.id),
        key: None,
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
    let opt = |l: &str| def.options.iter().find(|o| o.label == l).unwrap().id;
    let cloze = note(
        &t,
        builtin::find("cloze").unwrap().id,
        child.id,
        &[("text", "{{c1::Ankara}} is the capital of {{c2::Türkiye}}")],
        Some((def.id, opt("B1"))),
    );
    assert_eq!(cloze.cards.len(), 2, "one card per cloze number");
    let classic = note(
        &t,
        fc_core::notetype::builtin::CLASSIC_ID,
        child.id,
        &[
            ("front", "dog"),
            ("back", "<b>köpek</b><script>alert(1)</script>"),
        ],
        Some((def.id, opt("A2"))),
    );
    World {
        b1: opt("B1"),
        seviye: def.id,
        t,
        parent,
        child,
        cloze,
        classic,
    }
}

fn all_cards(t: &TestApp) -> Vec<Card> {
    t.store
        .read()
        .unwrap()
        .list_cards(&CardFilter {
            include_deleted: true,
            ..CardFilter::default()
        })
        .unwrap()
}

fn card(t: &TestApp, id: CardId) -> Card {
    t.store.read().unwrap().get_card(id).unwrap().unwrap()
}

fn reviews(t: &TestApp, id: CardId) -> Vec<fc_storage::repo::ReviewLogRecord> {
    t.store.read().unwrap().review_logs_for_card(id).unwrap()
}

/// Scheduled-mode reviews of a card.
fn scheduled_reviews(t: &TestApp, id: CardId) -> usize {
    reviews(t, id).iter().filter(|r| r.mode == "normal").count()
}

fn start(t: &TestApp, kind: StudyKind, mode: StudyMode, deck: DeckId) -> SessionDto {
    start_with(t, kind, mode, deck, None)
}

fn start_with(
    t: &TestApp,
    kind: StudyKind,
    mode: StudyMode,
    deck: DeckId,
    filter: Option<StudyFilter>,
) -> SessionDto {
    t.ok::<StudyStart>(StartSession {
        kind,
        mode: Some(mode),
        deck_id: Some(deck),
        include_subdecks: None,
        filter,
        from_session: None,
        new_limit: None,
        order: None,
        tz_offset_mins: Some(0),
        modes: None,
    })
}

fn answer(t: &TestApp, s: &SessionDto, card: CardId, rating: Rating) -> SessionDto {
    t.ok::<StudyAnswer>(AnswerCard {
        session_id: s.id,
        card_id: card,
        rating,
        duration_ms: 1_000,
        reveal_ms: Some(500),
    })
}

fn check(
    t: &TestApp,
    s: &SessionDto,
    card: CardId,
    given: &str,
    choice: Option<u32>,
) -> AnswerCheckDto {
    t.ok::<StudyCheckAnswer>(CheckAnswer {
        session_id: s.id,
        card_id: card,
        given: given.into(),
        choice,
    })
}

/// Removes `c2` from the cloze note: its card stays (with history) but is empty.
fn empty_c2(w: &World) -> CardId {
    let c2 = w
        .cloze
        .cards
        .iter()
        .find(|c| c.template_key == "c2")
        .unwrap()
        .id;
    let fields = w.t.ok::<NotesGet>(NoteRef { id: w.cloze.id }).fields;
    let mut fields: BTreeMap<String, String> = fields;
    fields.insert(
        "text".into(),
        "{{c1::Ankara}} is the capital of Türkiye".into(),
    );
    w.t.ok::<NotesUpdate>(UpdateNote {
        id: w.cloze.id,
        deck_id: None,
        fields,
        tags: vec![],
        attributes: vec![],
    });
    assert!(
        w.t.ok::<CardsRender>(fc_api::v1::cardtypes::CardRef { id: c2 })
            .empty
    );
    c2
}

// ------------------------------------------------------------------ scenarios

/// Classified cloze + classic notes → study (engine rendering, empty cards skipped, flags,
/// classification filter) → answer → undo → exact → redo → exact.
pub fn study_renders_card_types_and_undo_is_exact(f: &Factory) {
    let w = world(f);
    let t = &w.t;
    let c2 = empty_c2(&w);
    let classic_card = w.classic.cards[0].id;
    // Flag 7 = purple (Anki numbering, card_flag_defs).
    t.ok::<CardsSetFlag>(SetCardFlag {
        ids: vec![classic_card],
        flag: 7,
        on: true,
    });
    let flags = t.ok::<FlagsList>(Empty {}).flags;
    let purple = flags.iter().find(|f| f.flag == 7).unwrap();
    assert_eq!(purple.color, "purple");
    assert_eq!(flags.iter().find(|f| f.flag == 5).unwrap().color, "pink");

    // Walk the parent deck in a trial session (cards untouched): every card is rendered by
    // its engine and the empty c2 card is never queued.
    let s = start(t, StudyKind::Normal, StudyMode::Trial, w.parent.id);
    assert_eq!(s.counts.new, 2, "c1 + classic; empty c2 skipped");
    let mut seen = Vec::new();
    let mut cur = s;
    while let Some(c) = cur.current.clone() {
        assert_ne!(c.card_id, c2, "an empty card is never shown");
        if c.card_id == classic_card {
            assert_eq!(c.sides.card_type, "classic");
            assert_eq!(c.flags, 1 << 6, "flag 7 = bit 6");
            assert!(
                c.sides.answer.contains("<b>köpek</b>"),
                "{}",
                c.sides.answer
            );
            assert!(
                !c.sides.answer.contains("<script"),
                "sanitized: {}",
                c.sides.answer
            );
        } else {
            assert_eq!(c.sides.card_type, "cloze");
            assert_eq!(c.flags, 0);
            assert!(
                c.sides.question.contains("class=\"cloze\""),
                "{}",
                c.sides.question
            );
            assert!(
                !c.sides.question.contains("Ankara"),
                "c1 hidden: {}",
                c.sides.question
            );
            assert!(c.sides.answer.contains("Ankara"));
        }
        if !seen.contains(&c.card_id) {
            seen.push(c.card_id);
        }
        cur = answer(t, &cur, c.card_id, Rating::Easy);
    }
    assert_eq!(seen.len(), 2);
    // Answering an empty card directly is refused.
    let s = start(t, StudyKind::Normal, StudyMode::Scheduled, w.parent.id);
    let e = t
        .err::<StudyAnswer>(AnswerCard {
            session_id: s.id,
            card_id: c2,
            rating: Rating::Good,
            duration_ms: 0,
            reveal_ms: None,
        })
        .code;
    assert_eq!(e, ErrorCode::ConflictNotCurrentCard);

    // Custom session filtered by Seviye = B1 (inherited from the parent deck): only c1.
    let s = start_with(
        t,
        StudyKind::Custom,
        StudyMode::Scheduled,
        w.parent.id,
        Some(StudyFilter {
            attributes: vec![AttributeCondition {
                attr_id: w.seviye,
                values: vec![w.b1.to_string()],
            }],
            states: None,
            due_only: None,
            limit: None,
            query: None,
            saved_query_id: None,
        }),
    );
    assert_eq!(s.counts.new, 1);
    let shown = s.current.clone().unwrap();
    let c1 = shown.card_id;
    assert_eq!(shown.sides.template_key, "c1");
    assert!(!shown.type_answer);
    // Hints come from the answer text of the engine's rendering.
    assert!(t
        .ok::<StudyHint>(HintRequest {
            session_id: s.id,
            card_id: c1,
        })
        .text
        .is_some());

    let before = card(t, c1);
    let before_all = all_cards(t);
    let after = answer(t, &s, c1, Rating::Good);
    let answered = card(t, c1);
    assert_ne!(answered, before);
    assert_eq!(scheduled_reviews(t, c1), 1);
    assert_eq!(after.answered, 1);

    t.ok::<OplogUndo>(Empty {});
    assert_eq!(card(t, c1), before, "undo: card exactly as before");
    assert_eq!(all_cards(t), before_all, "undo: no other card changed");
    assert_eq!(scheduled_reviews(t, c1), 0, "undo: review removed");
    let back = t.ok::<StudySession>(SessionRef { session_id: s.id });
    assert_eq!(back.current.unwrap().card_id, c1, "undo: card shown again");
    assert_eq!(back.answered, 0);

    t.ok::<OplogRedo>(Empty {});
    assert_eq!(card(t, c1), answered, "redo: exactly the answered state");
    assert_eq!(scheduled_reviews(t, c1), 1);
    assert_eq!(
        card(t, c2).scheduling,
        before_all.iter().find(|c| c.id == c2).unwrap().scheduling
    );
}

/// A trial session over every card type leaves `cards` unchanged and logs trial reviews.
pub fn trial_mode_leaves_cards_unchanged(f: &Factory) {
    let w = world(f);
    let t = &w.t;
    let typing = note(
        t,
        builtin::find("typing").unwrap().id,
        w.child.id,
        &[("front", "cat"), ("back", "kedi")],
        None,
    );
    let choice = note(
        t,
        builtin::find("multiple_choice").unwrap().id,
        w.child.id,
        &[
            ("question", "2 + 2"),
            ("answer", "4"),
            ("distractors", "3\n5"),
        ],
        None,
    );
    let table = all_cards(t);
    let s = start(t, StudyKind::Normal, StudyMode::Trial, w.parent.id);
    // c1 and c2 are siblings: one per note (sibling burying).
    assert_eq!(s.counts.new, 4, "one cloze, classic, typing, choice");
    let mut cur = s.clone();
    let mut answered = 0;
    let mut kinds = Vec::new();
    for _ in 0..50 {
        let Some(c) = cur.current.clone() else { break };
        kinds.push(c.sides.card_type.clone());
        if c.note_id == typing.id {
            assert!(c.type_answer, "typing card type asks for typing");
            // v0.2 checker on the engine's expected answer.
            let ok = check(t, &cur, c.card_id, "Kedi", None);
            assert_eq!(
                (ok.grade, ok.suggested_rating),
                (AnswerGrade::Correct, Rating::Good)
            );
            let close = check(t, &cur, c.card_id, "kedj", None);
            assert_eq!(close.grade, AnswerGrade::Almost);
        }
        if c.note_id == choice.id {
            assert!(!c.type_answer);
            let CardInteraction::Choice { options, correct } = &c.sides.interaction else {
                panic!("choice interaction");
            };
            assert_eq!(options.len(), 3);
            let right = check(t, &cur, c.card_id, "", Some(*correct));
            assert_eq!(
                (right.grade, right.expected.as_str()),
                (AnswerGrade::Correct, "4")
            );
            let wrong = check(t, &cur, c.card_id, "", Some((*correct + 1) % 3));
            assert_eq!(
                (wrong.grade, wrong.suggested_rating),
                (AnswerGrade::Wrong, Rating::Again)
            );
        }
        cur = answer(t, &cur, c.card_id, Rating::Easy);
        answered += 1;
    }
    assert_eq!(answered, 4, "Easy graduates each new card at once");
    for k in ["classic", "cloze", "typing", "multiple_choice"] {
        assert!(kinds.iter().any(|x| x == k), "{k} studied: {kinds:?}");
    }
    assert_eq!(all_cards(t), table, "trial mode never writes cards");
    let logged: Vec<_> = table.iter().flat_map(|c| reviews(t, c.id)).collect();
    assert!(!logged.is_empty() && logged.iter().all(|r| r.mode == "trial"));
}

/// The deck preset's `new_per_day` / `reviews_per_day` cap normal sessions, counting what
/// was already studied today; custom sessions are not capped.
pub fn preset_daily_limits_apply(f: &Factory) {
    let w = world(f);
    let t = &w.t;
    // A preset with one new card a day on the parent deck.
    let preset = t.ok::<PresetsCreate>(fc_api::v1::presets::CreatePreset {
        name: "Sıkı".into(),
        config: Some(PresetConfig {
            new_per_day: 1,
            reviews_per_day: 0,
            desired_retention: 0.85,
            ..PresetConfig::default()
        }),
    });
    t.ok::<PresetsAssign>(fc_api::v1::presets::AssignPreset {
        deck_ids: vec![w.parent.id],
        preset_id: Some(preset.id),
    });
    let s = start(t, StudyKind::Normal, StudyMode::Scheduled, w.parent.id);
    assert_eq!(s.counts.new, 1, "new_per_day = 1");
    let c = s.current.clone().unwrap();
    answer(t, &s, c.card_id, Rating::Easy);
    let s2 = start(t, StudyKind::Normal, StudyMode::Scheduled, w.parent.id);
    assert_eq!(s2.counts.new, 0, "today's new card already introduced");
    // Undo gives the day's allowance back.
    t.ok::<OplogUndo>(Empty {});
    let s3 = start(t, StudyKind::Normal, StudyMode::Scheduled, w.parent.id);
    assert_eq!(s3.counts.new, 1);
    // Custom sessions ignore daily limits (one cloze sibling + classic).
    let custom = start(t, StudyKind::Custom, StudyMode::Scheduled, w.parent.id);
    assert_eq!(custom.counts.new, 2);
}

// ------------------------------------------------------------------ runners

macro_rules! phase2_suite {
    ($factory:expr) => {
        phase2_suite!(@tests $factory;
            study_renders_card_types_and_undo_is_exact,
            trial_mode_leaves_cards_unchanged,
            preset_daily_limits_apply,
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
    phase2_suite!(|| -> Arc<dyn Store> { Arc::new(MemStore::new()) });
}

mod sqlite_memory {
    use super::*;
    phase2_suite!(|| -> Arc<dyn Store> { Arc::new(SqliteStore::open_in_memory().unwrap()) });
}

mod sqlite_file {
    use super::*;
    phase2_suite!(|| -> Arc<dyn Store> { Arc::new(TempSqlite::new()) });
}
