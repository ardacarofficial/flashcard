//! Phase-2 integration, round 3 (+ v0.4 card management), end to end through the API on the
//! in-memory store, SQLite in memory and SQLite on a temp file:
//!
//! * deck presets (`presets.*`) drive study through `presets::effective`: inheritance from the
//!   parent deck, daily limits, learning steps / maximum interval change the next queue and
//!   intervals, undo puts the defaults back;
//! * custom flag names (v0.3 `card_flag_defs`) work in queries (`flag:<name>`), next to the
//!   built-in TR/EN colors; flag *n* = bit *n − 1*;
//! * custom study sessions from a query (`seviye:A2 is:due`) and from a saved query (a
//!   "filtered deck", canonical so renamed options keep working);
//! * `bulk.change_notetype` keeps card history where the template maps, one undo step;
//! * the search index holds folded plain text (cloze / HTML / links stripped), also for
//!   imported notes;
//! * the palette lists every track's commands; the shortcut editor covers study keys.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::testing::{MemStore, TempSqlite, TestApp};
use fc_api::v1::attributes::{
    AttributeAssignment, DefineAttribute, NewAttributeOption, RenameAttributeOption,
};
use fc_api::v1::browse::{SetShortcut, ShortcutContext};
use fc_api::v1::bulk::ChangeNoteType;
use fc_api::v1::cardtypes::{SetCardFlag, UpdateFlag};
use fc_api::v1::common::Empty;
use fc_api::v1::decks::{CreateDeck, DeckDto};
use fc_api::v1::methods::*;
use fc_api::v1::notes::{CreateNote, NoteDto};
use fc_api::v1::presets::{AssignPreset, CreatePreset, UpdatePreset};
use fc_api::v1::search::{CreateSavedQuery, QueryText, SearchCards};
use fc_api::v1::study::*;
use fc_api::ErrorCode;
use fc_core::attribute::{AttributeKind, AttributeValue};
use fc_core::card::{Card, CardState, Rating};
use fc_core::content::builtin;
use fc_core::preset::PresetConfig;
use fc_core::time::MILLIS_PER_DAY;
use fc_core::{AttributeDefId, AttributeOptionId, CardId, DeckId, NoteId, NoteTypeId};
use fc_storage::repo::CardFilter;
use fc_storage::{SqliteStore, Store};
use std::collections::BTreeMap;
use std::sync::Arc;

type Factory = dyn Fn() -> Arc<dyn Store>;

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
    attr: Option<(AttributeDefId, AttributeOptionId)>,
) -> NoteDto {
    t.ok::<NotesCreate>(CreateNote {
        notetype_id: nt,
        deck_id: deck,
        fields: fields
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect(),
        tags: vec![],
        attributes: attr
            .map(|(attr_id, o)| AttributeAssignment {
                attr_id,
                value: Some(AttributeValue::Choice(o)),
            })
            .into_iter()
            .collect(),
        allow_duplicate: Some(true),
    })
}

fn classic(t: &TestApp, deck: DeckId, front: &str, back: &str) -> NoteDto {
    note(
        t,
        fc_core::notetype::builtin::CLASSIC_ID,
        deck,
        &[("front", front), ("back", back)],
        None,
    )
}

fn card(t: &TestApp, id: CardId) -> Card {
    t.store.read().unwrap().get_card(id).unwrap().unwrap()
}

fn cards_of(t: &TestApp, note: NoteId) -> Vec<Card> {
    let mut v = t
        .store
        .read()
        .unwrap()
        .list_cards(&CardFilter {
            deck_ids: None,
            note_ids: Some(vec![note]),
            include_deleted: true,
        })
        .unwrap();
    v.sort_by_key(|c| c.id);
    v
}

fn note_row(t: &TestApp, id: NoteId) -> fc_core::note::Note {
    t.store.read().unwrap().get_note(id).unwrap().unwrap()
}

fn start(
    t: &TestApp,
    kind: StudyKind,
    deck: Option<DeckId>,
    filter: Option<StudyFilter>,
) -> SessionDto {
    t.ok::<StudyStart>(StartSession {
        kind,
        mode: Some(StudyMode::Scheduled),
        deck_id: deck,
        include_subdecks: None,
        filter,
        from_session: None,
        new_limit: None,
        order: None,
        tz_offset_mins: Some(0),
        modes: None,
        scope: None,
    })
}

fn by_query(query: &str) -> StudyFilter {
    StudyFilter {
        query: Some(query.into()),
        ..StudyFilter::default()
    }
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

fn hits(t: &TestApp, query: &str) -> Vec<CardId> {
    t.ok::<SearchIdsMethod>(QueryText {
        query: query.into(),
    })
    .card_ids
}

fn create_preset(t: &TestApp, name: &str, config: PresetConfig) -> fc_api::v1::presets::PresetDto {
    t.ok::<PresetsCreate>(CreatePreset {
        name: name.into(),
        config: Some(config),
    })
}

/// Puts a card in review, due a day ago (test setup, as if studied earlier).
fn make_due(t: &TestApp, id: CardId) {
    let mut tx = t.store.write().unwrap();
    let mut c = tx.get_card(id).unwrap().unwrap();
    let now = now(t).0;
    c.scheduling.state = CardState::Review;
    c.scheduling.due = Some(fc_core::Timestamp(now - MILLIS_PER_DAY));
    c.scheduling.last_review = Some(fc_core::Timestamp(now - 4 * MILLIS_PER_DAY));
    c.scheduling.stability = Some(3.0);
    c.scheduling.difficulty = Some(5.0);
    c.scheduling.reps = 2;
    tx.update_card(&c).unwrap();
    tx.commit().unwrap();
}

fn now(t: &TestApp) -> fc_core::Timestamp {
    t.app.now()
}

// ------------------------------------------------------------------ 1. presets

/// Changing a deck's preset (here: inherited from the parent) changes the next queue (daily
/// new limit) and the intervals (learning steps, maximum interval); undo restores defaults.
pub fn deck_preset_drives_queue_and_intervals(f: &Factory) {
    let t = TestApp::new(f());
    let parent = deck(&t, "Dil");
    let child = deck(&t, "Dil::İngilizce");
    for i in 0..5 {
        classic(&t, child.id, &format!("word{i}"), &format!("kelime{i}"));
    }
    // Defaults: 20 new a day, steps [1, 10], 100-year cap.
    let s = start(&t, StudyKind::Normal, Some(child.id), None);
    assert_eq!(s.counts.new, 5);
    let first = s.current.clone().unwrap().card_id;
    answer(&t, &s, first, Rating::Good);
    assert_eq!(
        card(&t, first).scheduling.state,
        CardState::Learning,
        "default steps"
    );
    let s = start(&t, StudyKind::Normal, Some(child.id), None);
    let easy = s.current.clone().unwrap().card_id;
    answer(&t, &s, easy, Rating::Easy);
    let c = card(&t, easy);
    let days = |c: &Card| (c.scheduling.due.unwrap().0 - now(&t).0) as f64 / MILLIS_PER_DAY as f64;
    assert!(
        days(&c) > 2.0,
        "default Easy interval is days: {}",
        days(&c)
    );

    // A preset on the parent: the child inherits it (`presets::effective`).
    let strict = create_preset(
        &t,
        "Sıkı",
        PresetConfig {
            new_per_day: 3,
            learning_steps_mins: vec![],
            maximum_interval_days: 1,
            ..PresetConfig::default()
        },
    );
    t.ok::<PresetsAssign>(AssignPreset {
        deck_ids: vec![parent.id],
        preset_id: Some(strict.id),
    });
    let eff = t.ok::<PresetsForDeck>(fc_api::v1::presets::PresetForDeck { deck_id: child.id });
    assert_eq!(eff.assigned_on, Some(parent.id));
    // Two new cards were introduced today: 3 − 2 = 1 left.
    let s = start(&t, StudyKind::Normal, Some(child.id), None);
    assert_eq!(s.counts.new, 1, "preset new_per_day minus today's");
    let next = s.current.clone().unwrap().card_id;
    answer(&t, &s, next, Rating::Good);
    let c = card(&t, next);
    assert_eq!(
        c.scheduling.state,
        CardState::Review,
        "no learning steps: straight to review"
    );
    assert!(
        days(&c) <= 1.01,
        "maximum_interval_days caps the interval: {}",
        days(&c)
    );

    // Editing the preset applies to the next session at once.
    t.ok::<PresetsUpdate>(UpdatePreset {
        id: strict.id,
        name: strict.name.clone(),
        config: PresetConfig {
            new_per_day: 10,
            ..strict.config.clone()
        },
    });
    assert_eq!(
        start(&t, StudyKind::Normal, Some(child.id), None)
            .counts
            .new,
        2
    );
    // Undo the edit, the answer and the assignment: back to the defaults.
    t.ok::<OplogUndo>(Empty {});
    assert_eq!(
        start(&t, StudyKind::Normal, Some(child.id), None)
            .counts
            .new,
        0
    );
    t.ok::<OplogUndo>(Empty {});
    assert_eq!(card(&t, next).scheduling.state, CardState::New);
    t.ok::<OplogUndo>(Empty {});
    let eff = t.ok::<PresetsForDeck>(fc_api::v1::presets::PresetForDeck { deck_id: child.id });
    assert_eq!((eff.preset_id, eff.config.new_per_day), (None, 20));
    assert_eq!(
        start(&t, StudyKind::Normal, Some(child.id), None)
            .counts
            .new,
        3
    );
}

// ------------------------------------------------------------------ 2. flags

/// Custom flag names from `card_flag_defs` resolve in queries; built-in TR/EN color names
/// keep working; flag n is bit n − 1 (Anki numbering).
pub fn custom_flag_names_in_queries(f: &Factory) {
    let t = TestApp::new(f());
    let d = deck(&t, "Bayrak");
    let a = classic(&t, d.id, "a", "1").cards[0].id;
    let b = classic(&t, d.id, "b", "2").cards[0].id;
    t.ok::<CardsSetFlag>(SetCardFlag {
        ids: vec![a],
        flag: 7,
        on: true,
    });
    t.ok::<CardsSetFlag>(SetCardFlag {
        ids: vec![b],
        flag: 1,
        on: true,
    });
    assert_eq!(card(&t, a).flags, 1 << 6, "flag 7 (purple) = bit 6");
    for q in ["flag:7", "flag:purple", "flag:mor", "flag:MOR"] {
        assert_eq!(hits(&t, q), vec![a], "{q}");
    }
    assert_eq!(hits(&t, "flag:red"), vec![b]);
    // Before naming: an unknown flag name is a positioned query error.
    let e = t.err::<SearchIdsMethod>(QueryText {
        query: "flag:önemli".into(),
    });
    assert_eq!(e.code, ErrorCode::ValidationQuery);
    t.ok::<FlagsUpdate>(UpdateFlag {
        flag: 7,
        name: Some("Önemli".into()),
        color: None,
    });
    t.ok::<FlagsUpdate>(UpdateFlag {
        flag: 1,
        name: Some("Çok zor".into()),
        color: None,
    });
    for q in [
        "flag:önemli",
        "flag:ONEMLI",
        "flag:Önemli",
        "-flag:0 flag:onemli",
    ] {
        assert_eq!(hits(&t, q), vec![a], "{q}");
    }
    for q in [
        "flag:çok_zor",
        "flag:\"çok zor\"",
        "flag:red",
        "flag:kırmızı",
    ] {
        assert_eq!(hits(&t, q), vec![b], "{q}");
    }
    // The browser's bulk flag writes the same bits.
    t.ok::<BulkSetFlag>(fc_api::v1::bulk::SetFlag {
        card_ids: vec![b],
        flag: 7,
        on: true,
    });
    assert_eq!(card(&t, b).flags, 1 | (1 << 6));
    assert_eq!(hits(&t, "flag:önemli").len(), 2);
    let e = t.err::<BulkSetFlag>(fc_api::v1::bulk::SetFlag {
        card_ids: vec![b],
        flag: 8,
        on: true,
    });
    assert_eq!(e.code, ErrorCode::ValidationFlag);
    // Undoing the rename makes the name unknown again.
    t.ok::<OplogUndo>(Empty {});
    t.ok::<OplogUndo>(Empty {});
    t.ok::<OplogUndo>(Empty {});
    assert_eq!(
        t.err::<SearchIdsMethod>(QueryText {
            query: "flag:çok_zor".into()
        })
        .code,
        ErrorCode::ValidationQuery
    );
}

// ------------------------------------------------------------------ 3. custom sessions

/// Custom study sessions from query text and from a saved query (filtered deck).
pub fn custom_sessions_from_queries(f: &Factory) {
    let t = TestApp::new(f());
    let d = deck(&t, "Dil");
    let level = t.ok::<AttributesDefine>(DefineAttribute {
        scope_deck_id: Some(d.id),
        key: None,
        name: "Seviye".into(),
        kind: AttributeKind::OrderedEnum,
        target: None,
        required: None,
        options: ["A1", "A2", "B1", "B2"]
            .iter()
            .map(|l| NewAttributeOption {
                label: (*l).into(),
                color: None,
            })
            .collect(),
    });
    let opt = |l: &str| level.options.iter().find(|o| o.label == l).unwrap().id;
    let nt = fc_core::notetype::builtin::CLASSIC_ID;
    let dog = note(
        &t,
        nt,
        d.id,
        &[("front", "dog"), ("back", "köpek")],
        Some((level.id, opt("A2"))),
    );
    let cat = note(
        &t,
        nt,
        d.id,
        &[("front", "cat"), ("back", "kedi")],
        Some((level.id, opt("A2"))),
    );
    let house = note(
        &t,
        nt,
        d.id,
        &[("front", "house"), ("back", "ev")],
        Some((level.id, opt("B1"))),
    );
    let (dog, cat, house) = (dog.cards[0].id, cat.cards[0].id, house.cards[0].id);
    make_due(&t, dog);

    // `seviye:A2 is:due` → only the due A2 card; the queue follows the query.
    let s = start(
        &t,
        StudyKind::Custom,
        None,
        Some(by_query("seviye:A2 is:due")),
    );
    assert_eq!((s.counts.review, s.counts.new), (1, 0));
    assert_eq!(s.current.clone().unwrap().card_id, dog);
    // Without `is:due` a custom session crams every A2 card.
    let all_a2 = start(
        &t,
        StudyKind::Custom,
        Some(d.id),
        Some(by_query("seviye:A2")),
    );
    assert_eq!(all_a2.counts.review + all_a2.counts.new, 2);
    // Answer the due card: the query no longer matches it.
    let s = start(
        &t,
        StudyKind::Custom,
        None,
        Some(by_query("seviye:A2 is:due")),
    );
    answer(&t, &s, dog, Rating::Good);
    assert!(card(&t, dog).scheduling.due.unwrap() > now(&t));
    // v1.3: an empty query result starts no session.
    let e = t.err::<StudyStart>(StartSession {
        kind: StudyKind::Custom,
        mode: Some(StudyMode::Scheduled),
        deck_id: None,
        include_subdecks: None,
        filter: Some(by_query("seviye:A2 is:due")),
        from_session: None,
        new_limit: None,
        order: None,
        tz_offset_mins: Some(0),
        modes: None,
        scope: None,
    });
    assert_eq!(e.code, ErrorCode::ConflictNothingToStudy);
    // Classification comparison + query text combined with the structured filter (AND).
    let b1 = start(&t, StudyKind::Custom, None, Some(by_query("seviye>=B1")));
    assert_eq!(b1.current.clone().unwrap().card_id, house);
    let filtered = StudyFilter {
        states: Some(vec![CardState::New]),
        ..by_query("seviye:A2")
    };
    let s = start(&t, StudyKind::Custom, None, Some(filtered));
    assert_eq!((s.counts.new, s.current.clone().unwrap().card_id), (1, cat));

    // A saved query is a "filtered deck": stored canonically, so renaming the option keeps it.
    let saved = t.ok::<SavedQueriesCreate>(CreateSavedQuery {
        name: "A2 kelimeler".into(),
        query: "seviye:A2 -is:due".into(),
        icon: None,
        position: None,
    });
    t.ok::<AttributesOptionRename>(RenameAttributeOption {
        attr_id: level.id,
        option_id: opt("A2"),
        label: "Temel".into(),
    });
    let s = start(
        &t,
        StudyKind::Custom,
        None,
        Some(StudyFilter {
            saved_query_id: Some(saved.id),
            ..StudyFilter::default()
        }),
    );
    let ids: Vec<CardId> = [s.current.clone().unwrap().card_id].into();
    assert_eq!(
        s.counts.new + s.counts.review + s.counts.learning,
        2,
        "dog (not due) + cat"
    );
    assert!(ids[0] == dog || ids[0] == cat);

    // Errors: positioned query errors, both forms at once, a query on a mistakes session.
    let e = t.err::<StudyStart>(StartSession {
        kind: StudyKind::Custom,
        mode: None,
        deck_id: None,
        include_subdecks: None,
        filter: Some(by_query("seviye:Z9")),
        from_session: None,
        new_limit: None,
        order: None,
        tz_offset_mins: None,
        modes: None,
        scope: None,
    });
    assert_eq!(e.code, ErrorCode::ValidationQuery);
    assert!(e.details["start"].is_number());
    let both = StudyFilter {
        saved_query_id: Some(saved.id),
        ..by_query("seviye:A2")
    };
    let e = t.err::<StudyStart>(StartSession {
        kind: StudyKind::Custom,
        mode: None,
        deck_id: None,
        include_subdecks: None,
        filter: Some(both),
        from_session: None,
        new_limit: None,
        order: None,
        tz_offset_mins: None,
        modes: None,
        scope: None,
    });
    assert_eq!(e.code, ErrorCode::ValidationInvalidParams);
}

// ------------------------------------------------------------------ 4. note type change

/// Classic + reverse → cloze: the forward card keeps its id and history as `c1`, the reverse
/// card is trashed, `c2` is generated; one undo step restores everything exactly, redo
/// re-applies exactly.
pub fn change_notetype_keeps_history_and_undoes(f: &Factory) {
    let t = TestApp::new(f());
    let d = deck(&t, "Değiştir");
    let reverse = builtin::find("classic_reverse").unwrap().id;
    let cloze = builtin::find("cloze").unwrap().id;
    let n = note(
        &t,
        reverse,
        d.id,
        &[
            ("front", "{{c1::Ankara}} and {{c2::İzmir}}"),
            ("back", "şehirler"),
        ],
        None,
    );
    assert_eq!(n.cards.len(), 2);
    let src = t.ok::<NotetypesGet>(fc_api::v1::notetypes::NoteTypeRef { id: reverse });
    let forward_key = src.templates[0].key.clone();
    let forward = cards_of(&t, n.id)
        .into_iter()
        .find(|c| c.template_key == forward_key)
        .unwrap();
    // Give the forward card history.
    let s = start(&t, StudyKind::Custom, Some(d.id), None);
    answer(&t, &s, s.current.clone().unwrap().card_id, Rating::Easy);
    let studied = s.current.clone().unwrap().card_id;
    let history = |id| t.store.read().unwrap().review_logs_for_card(id).unwrap();
    let logs_before = history(studied);
    assert_eq!(logs_before.len(), 1);
    let note_before = note_row(&t, n.id);
    let cards_before = cards_of(&t, n.id);

    // Mapping errors are validated before anything is written.
    let bad = |field_map: Option<BTreeMap<String, Option<String>>>, template_map| {
        t.err::<BulkChangeNotetype>(ChangeNoteType {
            note_ids: vec![n.id],
            notetype_id: cloze,
            field_map,
            template_map,
        })
        .code
    };
    let map = |pairs: &[(&str, Option<&str>)]| -> BTreeMap<String, Option<String>> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), v.map(str::to_owned)))
            .collect()
    };
    assert_eq!(
        bad(Some(map(&[("nope", Some("front"))])), None),
        ErrorCode::ValidationInvalidParams
    );
    assert_eq!(
        bad(Some(map(&[("text", Some("nope"))])), None),
        ErrorCode::ValidationInvalidParams
    );
    let other = classic(&t, d.id, "x", "y");
    let e = t.err::<BulkChangeNotetype>(ChangeNoteType {
        note_ids: vec![n.id, other.id],
        notetype_id: cloze,
        field_map: None,
        template_map: None,
    });
    assert_eq!(
        e.code,
        ErrorCode::ValidationInvalidParams,
        "mixed note types"
    );

    let r = t.ok::<BulkChangeNotetype>(ChangeNoteType {
        note_ids: vec![n.id],
        notetype_id: cloze,
        field_map: Some(map(&[("text", Some("front")), ("extra", Some("back"))])),
        template_map: Some(map(&[("cloze", Some(forward_key.as_str()))])),
    });
    assert_eq!(
        (r.notes, r.cards_kept, r.cards_trashed, r.cards_created),
        (1, 1, 1, 1)
    );
    let moved = note_row(&t, n.id);
    assert_eq!(moved.notetype_id, cloze);
    assert_eq!(
        moved.fields[&fc_core::notetype::FieldKey::new("text")],
        "{{c1::Ankara}} and {{c2::İzmir}}"
    );
    assert_eq!(
        moved.fields[&fc_core::notetype::FieldKey::new("extra")],
        "şehirler"
    );
    let after = cards_of(&t, n.id);
    let kept = after.iter().find(|c| c.id == forward.id).unwrap();
    assert_eq!(
        (kept.template_key.as_str(), kept.card_type.as_str()),
        ("c1", "cloze")
    );
    assert_eq!(
        kept.scheduling,
        forward_after_study(&cards_before, forward.id)
    );
    assert_eq!(history(studied), logs_before, "review history kept");
    let trashed: Vec<_> = after.iter().filter(|c| c.deleted_at.is_some()).collect();
    assert_eq!(trashed.len(), 1);
    assert_ne!(trashed[0].id, forward.id);
    let c2 = after.iter().find(|c| c.template_key == "c2").unwrap();
    assert!(c2.deleted_at.is_none() && c2.scheduling.state == CardState::New);
    // The moved note renders and studies as a cloze.
    let render = t.ok::<CardsRender>(fc_api::v1::cardtypes::CardRef { id: kept.id });
    assert!(!render.empty && render.sides.card_type == "cloze");
    assert!(
        render.sides.question.contains("[...]"),
        "{}",
        render.sides.question
    );
    let after_note = note_row(&t, n.id);
    let after_cards = cards_of(&t, n.id);

    // One undo step: note and the original cards exactly as before; the generated card is
    // trashed (cards are never deleted, like `cards.trash_generated`).
    t.ok::<OplogUndo>(Empty {});
    assert_eq!(note_row(&t, n.id), note_before);
    let undone = cards_of(&t, n.id);
    for c in &cards_before {
        assert_eq!(undone.iter().find(|u| u.id == c.id).unwrap(), c);
    }
    assert!(undone
        .iter()
        .find(|u| u.id == c2.id)
        .unwrap()
        .deleted_at
        .is_some());
    // Redo: exactly the changed state again.
    t.ok::<OplogRedo>(Empty {});
    assert_eq!(note_row(&t, n.id), after_note);
    let redone = cards_of(&t, n.id);
    for c in &after_cards {
        let r = redone.iter().find(|u| u.id == c.id).unwrap();
        assert_eq!(
            (&r.template_key, &r.card_type, r.deleted_at.is_some()),
            (&c.template_key, &c.card_type, c.deleted_at.is_some())
        );
        assert_eq!(r.scheduling, c.scheduling);
    }

    // Default mapping (same keys, then position): Classic → Classic + reverse keeps the card
    // and generates the reverse.
    let plain = classic(&t, d.id, "bir", "one");
    let r = t.ok::<BulkChangeNotetype>(ChangeNoteType {
        note_ids: vec![plain.id],
        notetype_id: reverse,
        field_map: None,
        template_map: None,
    });
    assert_eq!((r.cards_kept, r.cards_trashed, r.cards_created), (1, 0, 1));
    assert!(cards_of(&t, plain.id)
        .iter()
        .any(|c| c.id == plain.cards[0].id && c.deleted_at.is_none()));
}

fn forward_after_study(cards: &[Card], id: CardId) -> fc_core::card::SchedulingState {
    cards
        .iter()
        .find(|c| c.id == id)
        .unwrap()
        .scheduling
        .clone()
}

// ------------------------------------------------------------------ 5. search index

/// The index holds folded plain text: cloze answers (not `c1`), HTML-free, Turkish-folded,
/// links by label — for notes written by v0.3 editors and by v0.5 imports alike.
pub fn search_index_is_folded_plain_text(f: &Factory) {
    let t = TestApp::new(f());
    let d = deck(&t, "Ara");
    let cloze = builtin::find("cloze").unwrap().id;
    let n = note(
        &t,
        cloze,
        d.id,
        &[(
            "text",
            "{{c1::Ankara::şehir}} <b>başkent</b> [[Türkiye|ülke]]",
        )],
        None,
    );
    let ids = |q: &str| hits(&t, q).len();
    for q in [
        "ankara",
        "ANKARA",
        "baskent",
        "başkent",
        "\"ankara başkent ülke\"",
        "ulke",
    ] {
        assert_eq!(ids(q), n.cards.len(), "{q}");
    }
    for q in ["c1", "şehir", "turkiye"] {
        assert_eq!(ids(q), 0, "{q}");
    }
    let found = t.ok::<SearchCardsMethod>(SearchCards {
        query: "ankara".into(),
        sort: None,
        descending: None,
        notes_only: Some(true),
        offset: None,
        limit: None,
    });
    assert_eq!(found.total, 1);
    // Imported (v0.5 text import) notes go through the same path.
    let job = t.ok::<IoImportStart>(fc_api::v1::io::StartImport {
        source: fc_api::v1::io::ImportSource::Text {
            text: "<i>Kılıç</i>\tsword\nIşık\tlight".into(),
        },
        format: None,
        text_options: None,
        deck_id: Some(d.id),
        notetype_id: None,
        columns: None,
        skip_first_row: None,
        duplicates: None,
        create_missing_options: None,
        include_history: None,
    });
    t.app.run_pending_jobs().unwrap();
    let job = t.ok::<JobsGet>(fc_api::v1::jobs::JobRef { id: job.id });
    assert_eq!(
        job.status,
        fc_api::v1::jobs::JobStatus::Succeeded,
        "{:?}",
        job.error
    );
    for q in ["kilic", "KILIÇ", "isik", "ışık", "sword"] {
        assert_eq!(ids(q), 1, "{q}");
    }
    assert_eq!(ids("\"i kilic\""), 0, "HTML tag names are not indexed");
}

// ------------------------------------------------------------------ 6. palette, shortcuts

/// The palette lists the commands of all four tracks; the shortcut editor has the study keys
/// and catches conflicts with them.
pub fn palette_and_shortcuts_cover_every_track(f: &Factory) {
    let t = TestApp::new(f());
    let entries = t.ok::<PaletteListMethod>(Empty {}).entries;
    for m in [
        "study.start",
        "study.answer",
        "notetypes.update",
        "cards.render",
        "flags.update",
        "io.import.start",
        "io.backup.create",
        "presets.assign",
        "bulk.change_notetype",
        "search.cards",
    ] {
        assert!(entries.iter().any(|e| e.method == format!("v1.{m}")), "{m}");
    }
    let list = t.ok::<ShortcutsList>(Empty {});
    let study: BTreeMap<_, _> = list
        .items
        .iter()
        .filter(|i| i.context == ShortcutContext::Study)
        .map(|i| (i.action.as_str(), i.combo.clone()))
        .collect();
    for (a, c) in [
        ("study.reveal", "Space"),
        ("study.again", "1"),
        ("study.good", "3"),
        ("study.easy", "4"),
        ("study.hint", "H"),
        ("study.undo", "Z"),
        ("study.end", "Escape"),
    ] {
        assert_eq!(study.get(a).cloned().flatten().as_deref(), Some(c), "{a}");
    }
    assert!(list.conflicts.is_empty(), "{:?}", list.conflicts);
    // Rebinding a study key; a global combo on the same key conflicts.
    let l = t.ok::<ShortcutsSet>(SetShortcut {
        action: "study.good".into(),
        combo: Some("g".into()),
    });
    assert!(l
        .items
        .iter()
        .any(|i| i.action == "study.good" && i.combo.as_deref() == Some("G")));
    let e = t.err::<ShortcutsSet>(SetShortcut {
        action: "ui.search".into(),
        combo: Some("H".into()),
    });
    assert_eq!(e.code, ErrorCode::ConflictShortcut);
}

// ------------------------------------------------------------------ runners

macro_rules! suite {
    ($factory:expr) => {
        suite!(@tests $factory;
            deck_preset_drives_queue_and_intervals,
            custom_flag_names_in_queries,
            custom_sessions_from_queries,
            change_notetype_keeps_history_and_undoes,
            search_index_is_folded_plain_text,
            palette_and_shortcuts_cover_every_track,
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
    suite!(|| -> Arc<dyn Store> { Arc::new(MemStore::new()) });
}

mod sqlite_memory {
    use super::*;
    suite!(|| -> Arc<dyn Store> { Arc::new(SqliteStore::open_in_memory().unwrap()) });
}

mod sqlite_file {
    use super::*;
    suite!(|| -> Arc<dyn Store> { Arc::new(TempSqlite::new()) });
}
