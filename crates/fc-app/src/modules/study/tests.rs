//! Service tests of the study module. Every scenario runs against the in-memory store, SQLite
//! in memory and SQLite on a temp file (`study_suite!` at the bottom).
//!
//! v0.2 acceptance criteria (docs/04):
//! * deterministic 365-day simulation, FSRS consistent → [`simulation_365_days`] (app level;
//!   the reference-formula comparison is in `fc-study`'s scheduler tests);
//! * rating → undo → card exactly as before → [`answer_undo_redo_is_exact`],
//!   [`review_lapse_undo_is_exact`], [`sibling_burying_is_undone_exactly`];
//! * trial mode never changes `cards` → [`trial_mode_never_changes_cards`].
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::events::{EventBus, InMemoryEventBus};
use crate::{App, AppDeps};
use fc_api::v1::attributes::DefineAttributeFromTemplate;
use fc_api::v1::common::Empty;
use fc_api::v1::decks::{CreateDeck, DeckDto};
use fc_api::v1::events::{DomainEvent, EventEnvelope};
use fc_api::v1::methods::*;
use fc_api::v1::modules::SetModuleEnabled;
use fc_api::v1::notes::{CreateNote, NoteDto};
use fc_api::v1::oplog::Actor;
use fc_api::v1::settings::SetSetting;
use fc_api::v1::study::*;
use fc_api::{ApiError, ApiMethod, ErrorCode};
use fc_core::attribute::AttributeValue::Choice;
use fc_core::card::{Card, CardState, Rating, SchedulingState};
use fc_core::notetype::{builtin, CardTemplate};
use fc_core::rng::{Rng, SplitMix64};
use fc_core::settings::SettingScope;
use fc_core::time::{FixedClock, MILLIS_PER_DAY};
use fc_core::{CardId, DeckId, NoteTypeId, SessionId, Timestamp, V7IdGen};
use fc_storage::repo::CardFilter;
use fc_storage::Store;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

/// 2026-01-01T10:00Z.
const T0: i64 = 1_767_225_600_000 + 10 * 3_600_000;

type Factory = dyn Fn() -> Arc<dyn Store>;

/// An app over `store` with a hand-driven clock.
struct T {
    app: App,
    store: Arc<dyn Store>,
    clock: Arc<FixedClock>,
    events: Arc<Mutex<Vec<EventEnvelope>>>,
}

impl T {
    fn new(store: Arc<dyn Store>, seed: u64) -> Self {
        let clock = Arc::new(FixedClock::new(Timestamp(T0)));
        let bus = Arc::new(InMemoryEventBus::new());
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = events.clone();
        bus.subscribe(Arc::new(move |e: &EventEnvelope| sink.lock().unwrap().push(e.clone())));
        let app = App::new(AppDeps {
            store: store.clone(),
            ids: Box::new(V7IdGen::new(clock.clone(), SplitMix64::new(seed))),
            clock: clock.clone(),
            events: bus,
        })
        .unwrap();
        app.load_state().unwrap();
        Self { app, store, clock, events }
    }

    fn ok<M: ApiMethod>(&self, req: M::Request) -> M::Response {
        match self.app.call::<M>(&req, &Actor::User) {
            Ok(r) => r,
            Err(e) => panic!("{} failed: {e:?}", M::NAME),
        }
    }

    fn err<M: ApiMethod>(&self, req: M::Request) -> ApiError {
        match self.app.call::<M>(&req, &Actor::User) {
            Ok(_) => panic!("{} unexpectedly succeeded", M::NAME),
            Err(e) => e,
        }
    }

    fn now(&self) -> Timestamp {
        fc_core::Clock::now(&*self.clock)
    }

    fn advance(&self, ms: i64) {
        self.clock.advance(ms);
    }

    fn deck(&self, path: &str) -> DeckDto {
        self.ok::<DecksCreate>(CreateDeck {
            path: path.into(),
            parent_id: None,
            description: None,
            icon: None,
            color: None,
        })
    }

    fn note_of(&self, notetype: NoteTypeId, deck: DeckId, front: &str, back: &str) -> NoteDto {
        self.ok::<NotesCreate>(CreateNote {
            notetype_id: notetype,
            deck_id: deck,
            fields: BTreeMap::from([
                ("front".to_owned(), front.to_owned()),
                ("back".to_owned(), back.to_owned()),
            ]),
            tags: vec![],
            attributes: vec![],
            allow_duplicate: None,
        })
    }

    fn note(&self, deck: DeckId, front: &str, back: &str) -> CardId {
        self.note_of(builtin::CLASSIC_ID, deck, front, back).cards[0].id
    }

    /// The full stored card (every column).
    fn card(&self, id: CardId) -> Card {
        self.store.read().unwrap().get_card(id).unwrap().unwrap()
    }

    fn all_cards(&self) -> Vec<Card> {
        self.store
            .read()
            .unwrap()
            .list_cards(&CardFilter {
                include_deleted: true,
                ..CardFilter::default()
            })
            .unwrap()
    }

    fn start(&self, kind: StudyKind, mode: StudyMode, deck: Option<DeckId>) -> SessionDto {
        self.ok::<StudyStart>(StartSession {
            kind,
            mode: Some(mode),
            deck_id: deck,
            include_subdecks: None,
            filter: None,
            from_session: None,
            new_limit: None,
            order: None,
            tz_offset_mins: Some(0),
            modes: None,
            scope: None,
        })
    }

    /// v1.3: `None` when there is nothing to study (`CONFLICT_NOTHING_TO_STUDY`: no session
    /// is created).
    fn try_start(&self, kind: StudyKind, mode: StudyMode, deck: Option<DeckId>) -> Option<SessionDto> {
        let req = StartSession {
            kind,
            mode: Some(mode),
            deck_id: deck,
            include_subdecks: None,
            filter: None,
            from_session: None,
            new_limit: None,
            order: None,
            tz_offset_mins: Some(0),
            modes: None,
            scope: None,
        };
        match self.app.call::<StudyStart>(&req, &Actor::User) {
            Ok(s) => Some(s),
            Err(e) if e.code == ErrorCode::ConflictNothingToStudy => None,
            Err(e) => panic!("study.start failed: {e:?}"),
        }
    }

    fn session(&self, id: SessionId) -> SessionDto {
        self.ok::<StudySession>(SessionRef { session_id: id })
    }

    fn answer(&self, s: SessionId, card: CardId, rating: Rating) -> SessionDto {
        self.ok::<StudyAnswer>(AnswerCard {
            session_id: s,
            card_id: card,
            rating,
            duration_ms: 1_000,
            reveal_ms: None,
            typed_answer: None,
        })
    }

    fn history(&self, card: CardId) -> CardHistory {
        self.ok::<StudyCardHistory>(CardHistoryRequest { card_id: card })
    }

    fn undo(&self) {
        self.ok::<OplogUndo>(Empty {});
    }

    fn redo(&self) {
        self.ok::<OplogRedo>(Empty {});
    }

    fn set(&self, key: &str, value: serde_json::Value) {
        self.ok::<SettingsSet>(SetSetting {
            scope: SettingScope::App,
            key: key.into(),
            value,
        });
    }
}

/// A note type with two cards per note (forward + reverse), inserted directly (the note type
/// editor arrives with v0.3).
fn two_way(t: &T) -> NoteTypeId {
    let mut nt = builtin::classic(Timestamp(1), "İki yön", "Ön", "Arka");
    nt.id = NoteTypeId(uuid::Uuid::from_u128(0x0190_0000_0000_7000_8000_0000_0000_0099));
    nt.builtin = None;
    nt.templates.push(CardTemplate {
        key: "reverse".into(),
        name: "2".into(),
        front: "{{back}}".into(),
        back: "{{FrontSide}}\n<hr class=\"fc-answer\">\n{{front}}".into(),
        card_type: "classic".into(),
        answer_field: None,
    });
    let mut tx = t.store.write().unwrap();
    tx.upsert_notetype(&nt).unwrap();
    tx.commit().unwrap();
    nt.id
}

// ------------------------------------------------------------------ acceptance

/// Rating → undo → the card is exactly as before (every column), its review is gone and the
/// session shows it again; redo re-applies exactly.
pub fn answer_undo_redo_is_exact(f: &Factory) {
    let t = T::new(f(), 1);
    let deck = t.deck("Dil");
    let card = t.note(deck.id, "dog", "köpek");
    let other = t.note(deck.id, "cat", "kedi");
    let before = t.card(card);
    let s = t.start(StudyKind::Normal, StudyMode::Scheduled, Some(deck.id));
    assert_eq!(s.counts.new, 2);
    let shown = s.current.clone().unwrap();
    assert_eq!(shown.card_id, card);
    // Rendered by the classic card type engine (sanitized HTML).
    assert_eq!(shown.sides.card_type, "classic");
    assert_eq!(fc_study::text::html_to_text(&shown.sides.question), "dog");
    assert_eq!(super::engine::answer_text(&shown.sides.answer), "köpek");
    assert_eq!(shown.intervals.len(), 4);
    assert_eq!(shown.intervals[0].interval_ms, 60_000, "again = first learning step");

    t.advance(5_000);
    let after = t.ok::<StudyAnswer>(AnswerCard {
        session_id: s.id,
        card_id: card,
        rating: Rating::Good,
        duration_ms: 9_000, // more than really passed: capped at 5 s
        reveal_ms: Some(2_000),
        typed_answer: None,
    });
    assert_eq!(after.answered, 1);
    assert_eq!(after.current.as_ref().unwrap().card_id, other);
    let answered = t.card(card);
    assert_eq!(answered.scheduling.state, CardState::Learning);
    assert_ne!(answered, before);
    let h = t.history(card);
    assert_eq!(h.reviews.len(), 1);
    let r = &h.reviews[0];
    assert_eq!((r.duration_ms, r.answer_duration_ms), (5_000, Some(2_000)));
    assert_eq!((r.rating, r.state_before, r.state_after), (Rating::Good, CardState::New, CardState::Learning));
    assert_eq!(r.mode, "normal");
    assert!(t
        .events
        .lock()
        .unwrap()
        .iter()
        .any(|e| matches!(&e.event, DomainEvent::StudyCardsChanged { card_ids } if card_ids == &vec![card])));

    t.undo();
    assert_eq!(t.card(card), before, "undo restores every column");
    assert!(t.history(card).reviews.is_empty());
    let back = t.session(s.id);
    assert_eq!(back.current.as_ref().unwrap().card_id, card, "card shown again");
    assert_eq!((back.answered, back.counts), (0, s.counts));

    t.redo();
    assert_eq!(t.card(card), answered);
    assert_eq!(t.history(card).reviews, h.reviews);
    assert_eq!(t.session(s.id).answered, 1);
    t.undo();
    assert_eq!(t.card(card), before);
}

/// A lapse on a review card (Again: lapses, stability, difficulty, reps, relearning step) is
/// undone exactly. The card is answered early on its due day, less than `interval × 24 h`
/// after the previous review: it is in the session and the elapsed days are calendar days.
pub fn review_lapse_undo_is_exact(f: &Factory) {
    let t = T::new(f(), 11);
    let deck = t.deck("Dil");
    let card = t.note(deck.id, "dog", "köpek");
    let s = t.start(StudyKind::Normal, StudyMode::Scheduled, Some(deck.id));
    t.answer(s.id, card, Rating::Easy); // 10:00Z → review
    t.ok::<StudyEnd>(SessionRef { session_id: s.id });
    let graduated = t.card(card);
    assert_eq!(graduated.scheduling.state, CardState::Review);
    let days = t.history(card).reviews[0].scheduled_days;
    assert!(days >= 2);
    // Due at the start (04:00Z) of the due study day.
    let due = graduated.scheduling.due.unwrap();
    assert_eq!(due, Timestamp(T0 - 6 * 3_600_000 + i64::from(days) * MILLIS_PER_DAY));

    // 05:00Z on the due day: 19 h short of `days × 24 h`.
    t.clock.set(due.plus_millis(3_600_000));
    let s = t.start(StudyKind::Normal, StudyMode::Scheduled, Some(deck.id));
    assert_eq!(s.counts.review, 1, "due today although not 24 h × interval later");
    let before = t.card(card);
    let after = t.answer(s.id, card, Rating::Again);
    let lapsed = t.card(card);
    assert_eq!(lapsed.scheduling.state, CardState::Relearning);
    assert_eq!(lapsed.scheduling.lapses, before.scheduling.lapses + 1);
    assert_eq!(lapsed.scheduling.reps, before.scheduling.reps + 1);
    assert_ne!(lapsed.scheduling.stability, before.scheduling.stability);
    assert_ne!(lapsed.scheduling.difficulty, before.scheduling.difficulty);
    let h = t.history(card);
    assert_eq!(h.reviews.len(), 2);
    assert_eq!(h.reviews[1].elapsed_days, days, "calendar days, not 24-hour spans");
    assert_eq!((h.reviews[1].state_before, h.reviews[1].state_after), (CardState::Review, CardState::Relearning));
    assert_eq!(after.counts.learning, 1);

    t.undo();
    assert_eq!(t.card(card), before, "undo restores every column");
    assert_eq!(t.history(card).reviews.len(), 1);
    let back = t.session(s.id);
    assert_eq!((back.answered, back.counts), (0, s.counts));
    assert_eq!(back.current.unwrap().card_id, card);

    t.redo();
    assert_eq!(t.card(card), lapsed);
    assert_eq!(t.history(card).reviews, h.reviews);
    t.undo();
    assert_eq!(t.card(card), before);
}

/// Undoing an answer of an earlier session restores that session's queue but does not reopen
/// it: only the newer session stays open.
pub fn undo_does_not_reopen_ended_session(f: &Factory) {
    let t = T::new(f(), 12);
    let deck = t.deck("D");
    let card = t.note(deck.id, "q0", "a");
    t.note(deck.id, "q1", "a");
    let a = t.start(StudyKind::Normal, StudyMode::Scheduled, Some(deck.id));
    let before = t.card(card);
    t.answer(a.id, card, Rating::Easy);
    let b = t.start(StudyKind::Normal, StudyMode::Scheduled, Some(deck.id));
    assert!(t.ok::<StudySummary>(SessionRef { session_id: a.id }).ended_at.is_some());
    t.undo();
    assert_eq!(t.card(card), before);
    assert!(
        t.ok::<StudySummary>(SessionRef { session_id: a.id }).ended_at.is_some(),
        "the ended session stays ended"
    );
    assert_eq!(t.ok::<StudyActive>(Empty {}).session.unwrap().id, b.id);
    assert_eq!(t.session(a.id).answered, 0, "its queue state is restored");
}

/// Answering one card buries its siblings until the day ends; undo un-buries them exactly.
pub fn sibling_burying_is_undone_exactly(f: &Factory) {
    let t = T::new(f(), 2);
    let nt = two_way(&t);
    let deck = t.deck("Dil");
    let note = t.note_of(nt, deck.id, "dog", "köpek");
    assert_eq!(note.cards.len(), 2);
    // Sibling burying is a deck option (v0.4 preset, read through `presets::effective`; on
    // by default).
    let preset = t.ok::<PresetsCreate>(fc_api::v1::presets::CreatePreset {
        name: "Göm".into(),
        config: None,
    });
    assert!(preset.config.bury_siblings);
    t.ok::<PresetsAssign>(fc_api::v1::presets::AssignPreset {
        deck_ids: vec![deck.id],
        preset_id: Some(preset.id),
    });
    let s = t.start(StudyKind::Normal, StudyMode::Scheduled, Some(deck.id));
    assert_eq!(s.counts.new, 1, "one card per note");
    let first = s.current.unwrap().card_id;
    let sibling = note.cards.iter().find(|c| c.id != first).unwrap().id;
    let sib_before = t.card(sibling);
    t.answer(s.id, first, Rating::Easy);
    let buried = t.card(sibling);
    let day_end = fc_study::day::day_end(t.now(), 0, 4);
    assert_eq!(buried.buried_until, Some(day_end));
    t.undo();
    assert_eq!(t.card(sibling), sib_before);
    // With burying off, both cards are queued.
    t.ok::<PresetsUpdate>(fc_api::v1::presets::UpdatePreset {
        id: preset.id,
        name: preset.name.clone(),
        config: fc_core::preset::PresetConfig {
            bury_siblings: false,
            ..preset.config.clone()
        },
    });
    let s = t.start(StudyKind::Normal, StudyMode::Scheduled, Some(deck.id));
    assert_eq!(s.counts.new, 2);
}

/// Trial sessions log reviews (`mode = "trial"`) and run the full step machine on a simulated
/// state, but never write `cards`.
pub fn trial_mode_never_changes_cards(f: &Factory) {
    let t = T::new(f(), 3);
    let deck = t.deck("Deneme");
    let cards: Vec<CardId> = (0..3).map(|i| t.note(deck.id, &format!("q{i}"), &format!("a{i}"))).collect();
    // Give one card a real schedule first so trial sees new and review cards.
    let s = t.start(StudyKind::Normal, StudyMode::Scheduled, Some(deck.id));
    t.answer(s.id, cards[0], Rating::Easy);
    t.ok::<StudyEnd>(SessionRef { session_id: s.id });
    t.advance(30 * MILLIS_PER_DAY);

    let table_before = t.all_cards();
    let trial = t.ok::<StudyStart>(StartSession {
        kind: StudyKind::Custom,
        mode: Some(StudyMode::Trial),
        deck_id: Some(deck.id),
        include_subdecks: None,
        filter: None,
        from_session: None,
        new_limit: None,
        order: None,
        tz_offset_mins: Some(0),
        modes: None,
        scope: None,
    });
    assert_eq!(trial.counts.new + trial.counts.review, 3);
    let ratings = [Rating::Again, Rating::Hard, Rating::Good, Rating::Again, Rating::Easy];
    let mut answered = 0;
    let mut cur = trial;
    for i in 0..60 {
        let Some(c) = cur.current.clone() else { break };
        // Hints and answer checks are allowed and do not touch cards either.
        t.ok::<StudyHint>(HintRequest { session_id: cur.id, card_id: c.card_id });
        t.ok::<StudyCheckAnswer>(CheckAnswer {
            session_id: cur.id,
            card_id: c.card_id,
            given: "a".into(),
            choice: None,
        });
        t.advance(15 * 60_000);
        cur = t.answer(cur.id, c.card_id, ratings[i % ratings.len()]);
        answered += 1;
    }
    assert!(cur.current.is_none() && answered >= 5, "{answered}");
    assert!(cur.mistakes >= 1);
    t.ok::<StudyEnd>(SessionRef { session_id: cur.id });
    assert_eq!(t.all_cards(), table_before, "cards table unchanged");
    let trial_reviews: usize = cards
        .iter()
        .map(|c| t.history(*c).reviews.iter().filter(|r| r.mode == "trial").count())
        .sum();
    assert_eq!(trial_reviews, answered);
    // Undoing a trial answer also leaves cards alone.
    let again = t.start(StudyKind::Custom, StudyMode::Trial, Some(deck.id));
    let c = again.current.unwrap().card_id;
    t.answer(again.id, c, Rating::Good);
    t.undo();
    assert_eq!(t.all_cards(), table_before);
}

/// One year of daily sessions on a deterministic clock. Every card's state can be replayed
/// from its review log through the scheduler (docs/03 §5) and two runs with the same seed
/// produce the same history.
pub fn simulation_365_days(f: &Factory) {
    fn run(store: Arc<dyn Store>) -> (Vec<Card>, Vec<Vec<ReviewLogDto>>) {
        let t = T::new(store, 77);
        // The replay below checks the pure scheduler; v0.6 planning (load balancing) would
        // move due days within their fuzz range (its own simulation is in fc-study).
        t.ok::<ModulesSetEnabled>(SetModuleEnabled {
            id: "planning".into(),
            enabled: false,
        });
        let deck = t.deck("Yıl");
        let cards: Vec<CardId> = (0..8).map(|i| t.note(deck.id, &format!("w{i}"), "x")).collect();
        let mut rng = SplitMix64::new(42);
        for day in 0..365i64 {
            t.clock.set(Timestamp(T0 + day * MILLIS_PER_DAY));
            // v1.3: a day with nothing due starts no session.
            let Some(mut s) = t.try_start(StudyKind::Normal, StudyMode::Scheduled, None) else {
                continue;
            };
            for _ in 0..200 {
                let Some(c) = s.current.clone() else { break };
                let rating = match rng.below(10) {
                    0 => Rating::Again,
                    1 => Rating::Hard,
                    9 => Rating::Easy,
                    _ => Rating::Good,
                };
                t.advance(8_000);
                s = t.answer(s.id, c.card_id, rating);
            }
            assert!(s.current.is_none());
            t.ok::<StudyEnd>(SessionRef { session_id: s.id });
        }
        let sched = fc_study::scheduler::builtin("fsrs").unwrap();
        let params = fc_study::scheduler::default_params();
        let mut histories = Vec::new();
        for c in &cards {
            let card = t.card(*c);
            let h = t.history(*c);
            assert!(h.reviews.len() >= 4, "card reviewed through the year");
            // Replay: the review log fully determines the stored schedule.
            let mut state = SchedulingState::NEW;
            for r in &h.reviews {
                assert_eq!(r.state_before, state.state);
                let out = sched.schedule(&state, r.rating, r.created_at, &params).unwrap();
                assert_eq!(out.state.state, r.state_after);
                assert_eq!(out.scheduled_days, r.scheduled_days);
                assert_eq!(out.state.stability, r.stability_after);
                state = out.state;
            }
            assert_eq!(state, card.scheduling);
            assert!(card.scheduling.due.unwrap() > Timestamp(T0 + 364 * MILLIS_PER_DAY));
            assert!(h.retrievability.unwrap() > 0.0);
            histories.push(h.reviews);
        }
        (t.all_cards(), histories)
    }
    let a = run(f());
    let b = run(f());
    assert_eq!(a, b, "deterministic");
}

// ------------------------------------------------------------------ engine behaviour

/// Again keeps the card in the session (learning step) and shows it again once due.
pub fn learning_cards_come_back_in_the_session(f: &Factory) {
    let t = T::new(f(), 4);
    let deck = t.deck("D");
    let card = t.note(deck.id, "a", "b");
    let s = t.start(StudyKind::Normal, StudyMode::Scheduled, None);
    let s = t.answer(s.id, card, Rating::Again);
    assert_eq!(s.counts.learning, 1);
    // Only card left: shown within the learn-ahead window.
    assert_eq!(s.current.as_ref().unwrap().card_id, card);
    assert_eq!(s.current.unwrap().state, CardState::Learning);
    let s = t.answer(s.id, card, Rating::Good); // → 10 min step
    let s = t.answer(s.id, card, Rating::Good); // graduates
    assert!(s.current.is_none());
    assert_eq!(s.counts, StudyCounts::default());
    assert_eq!((s.answered, s.correct, s.mistakes), (3, 2, 1));
    let summary = t.ok::<StudyEnd>(SessionRef { session_id: s.id });
    assert_eq!((summary.answered, summary.mistakes), (3, 1));
    assert!(summary.ended_at.is_some());
    assert_eq!(t.card(card).scheduling.state, CardState::Review);
}

/// The session survives an app restart: `study.active` resumes it where it was.
pub fn session_survives_restart(f: &Factory) {
    let store = f();
    let first = T::new(store.clone(), 5);
    let deck = first.deck("D");
    for i in 0..3 {
        first.note(deck.id, &format!("q{i}"), "a");
    }
    let s = first.start(StudyKind::Normal, StudyMode::Scheduled, Some(deck.id));
    let s = first.answer(s.id, s.current.clone().unwrap().card_id, Rating::Easy);
    drop(first);
    let second = T::new(store, 6);
    let resumed = second.ok::<StudyActive>(Empty {}).session.unwrap();
    assert_eq!(resumed, s);
    // Starting a new session closes the old one.
    let next = second.start(StudyKind::Normal, StudyMode::Scheduled, None);
    assert_eq!(second.ok::<StudyActive>(Empty {}).session.unwrap().id, next.id);
    assert!(second.ok::<StudySummary>(SessionRef { session_id: s.id }).ended_at.is_some());
}

/// "Repeat mistakes": a mistakes session holds exactly the cards rated Again.
pub fn mistakes_session_repeats_again_cards(f: &Factory) {
    let t = T::new(f(), 7);
    let deck = t.deck("D");
    let cards: Vec<CardId> = (0..4).map(|i| t.note(deck.id, &format!("q{i}"), "a")).collect();
    let mut s = t.start(StudyKind::Normal, StudyMode::Scheduled, Some(deck.id));
    let mut failed = Vec::new();
    while let Some(c) = s.current.clone() {
        let fail = c.card_id == cards[1] || c.card_id == cards[3];
        let rating = if fail && !failed.contains(&c.card_id) {
            failed.push(c.card_id);
            Rating::Again
        } else {
            Rating::Easy
        };
        s = t.answer(s.id, c.card_id, rating);
    }
    let summary = t.ok::<StudyEnd>(SessionRef { session_id: s.id });
    assert_eq!(summary.mistakes, 2);
    let m = t.ok::<StudyStart>(StartSession {
        kind: StudyKind::Mistakes,
        mode: Some(StudyMode::Trial),
        deck_id: None,
        include_subdecks: None,
        filter: None,
        from_session: Some(s.id),
        new_limit: None,
        order: None,
        tz_offset_mins: None,
        modes: None,
        scope: None,
    });
    let total = m.counts.new + m.counts.learning + m.counts.review;
    assert_eq!(total, 2);
    let mut seen = Vec::new();
    let mut m = m;
    while let Some(c) = m.current.clone() {
        seen.push(c.card_id);
        m = t.answer(m.id, c.card_id, Rating::Easy);
    }
    seen.sort();
    failed.sort();
    assert_eq!(seen, failed);
    let e = t.err::<StudyStart>(StartSession {
        kind: StudyKind::Mistakes,
        mode: None,
        deck_id: None,
        include_subdecks: None,
        filter: None,
        from_session: None,
        new_limit: None,
        order: None,
        tz_offset_mins: None,
        modes: None,
        scope: None,
    });
    assert_eq!(e.code, ErrorCode::ValidationInvalidParams);
}

/// Custom sessions: classification filter (Seviye = A2) and cramming not-yet-due cards.
pub fn custom_session_filters_by_classification(f: &Factory) {
    let t = T::new(f(), 8);
    let en = t.deck("İngilizce");
    let words = t.ok::<DecksCreate>(CreateDeck {
        path: "Kelimeler".into(),
        parent_id: Some(en.id),
        description: None,
        icon: None,
        color: None,
    });
    let level = t.ok::<AttributesDefineFromTemplate>(DefineAttributeFromTemplate {
        template_id: "cefr".into(),
        scope_deck_id: Some(en.id),
        locale: None,
    });
    let opt = |label: &str| level.options.iter().find(|o| o.label == label).unwrap().id;
    let mk = |front: &str, label: &str| -> CardId {
        t.ok::<NotesCreate>(CreateNote {
            notetype_id: builtin::CLASSIC_ID,
            deck_id: words.id,
            fields: BTreeMap::from([
                ("front".to_owned(), front.to_owned()),
                ("back".to_owned(), "x".to_owned()),
            ]),
            tags: vec![],
            attributes: vec![fc_api::v1::attributes::AttributeAssignment {
                attr_id: level.id,
                value: Some(Choice(opt(label))),
            }],
            allow_duplicate: None,
        })
        .cards[0]
            .id
    };
    let a1 = mk("one", "A1");
    let a2a = mk("two", "A2");
    let a2b = mk("three", "A2");
    let _b1 = mk("four", "B1");
    // Make a2b a review card due in 10 days.
    let s = t.start(StudyKind::Custom, StudyMode::Scheduled, None);
    let mut s = s;
    while let Some(c) = s.current.clone() {
        let r = if c.card_id == a2b { Rating::Easy } else { Rating::Again };
        if c.card_id != a2b && c.state != CardState::New {
            break;
        }
        s = t.answer(s.id, c.card_id, r);
    }
    t.ok::<StudyEnd>(SessionRef { session_id: s.id });
    assert_eq!(t.card(a2b).scheduling.state, CardState::Review);

    let filter = |due_only: Option<bool>| StudyFilter {
        attributes: vec![AttributeCondition {
            attr_id: level.id,
            values: vec![opt("A2").to_string()],
        }],
        states: None,
        due_only,
        limit: None,
        query: None,
        saved_query_id: None,
    };
    let start = |kind, filter| {
        t.ok::<StudyStart>(StartSession {
            kind,
            mode: Some(StudyMode::Trial),
            deck_id: Some(en.id),
            include_subdecks: Some(true),
            filter: Some(filter),
            from_session: None,
            new_limit: None,
            order: Some(StudyOrder::Due),
            tz_offset_mins: Some(0),
            modes: None,
            scope: None,
        })
    };
    let due = start(StudyKind::Normal, filter(None));
    assert_eq!(due.counts.review, 0, "normal sessions only take due cards");
    let cram = start(StudyKind::Custom, filter(None));
    assert_eq!(
        (cram.counts.new + cram.counts.learning, cram.counts.review),
        (1, 1),
        "A2 only, not-yet-due review included"
    );
    let mut ids = Vec::new();
    let mut c = cram;
    while let Some(cur) = c.current.clone() {
        ids.push(cur.card_id);
        c = t.answer(c.id, cur.card_id, Rating::Easy);
    }
    ids.sort();
    let mut want = vec![a2a, a2b];
    want.sort();
    assert_eq!(ids, want);
    assert!(!ids.contains(&a1));
}

/// Suspend/bury the current card during a session (it leaves the queue) and undo both.
pub fn suspend_and_bury_in_session_with_undo(f: &Factory) {
    let t = T::new(f(), 9);
    let deck = t.deck("D");
    let a = t.note(deck.id, "a", "1");
    let b = t.note(deck.id, "b", "2");
    let c = t.note(deck.id, "c", "3");
    let a_before = t.card(a);
    let s = t.start(StudyKind::Normal, StudyMode::Scheduled, Some(deck.id));
    assert_eq!(s.current.unwrap().card_id, a);
    let r = t.ok::<StudySuspend>(SuspendCards {
        card_ids: vec![a],
        suspended: true,
        session_id: Some(s.id),
    });
    assert_eq!(r.cards, 1);
    assert!(t.card(a).suspended);
    let now = t.session(s.id);
    assert_eq!(now.current.unwrap().card_id, b);
    assert_eq!(now.counts.new, 2);
    t.ok::<StudyBury>(BuryCards {
        card_ids: vec![b],
        buried: true,
        session_id: Some(s.id),
        tz_offset_mins: None,
    });
    assert_eq!(t.session(s.id).current.unwrap().card_id, c);
    assert!(t.card(b).buried_until.is_some());
    t.undo(); // bury
    assert!(t.card(b).buried_until.is_none());
    assert_eq!(t.session(s.id).current.unwrap().card_id, b);
    t.redo();
    // A new session skips both (and ends `s`).
    let other = t.start(StudyKind::Normal, StudyMode::Scheduled, Some(deck.id));
    assert_eq!(other.counts.new, 1);
    t.undo(); // bury: the queue of the ended session is restored, it stays ended
    assert!(t.card(b).buried_until.is_none());
    let back = t.session(s.id);
    assert!(back.ended_at.is_some() && back.current.is_none());
    assert_eq!(back.counts.new, 2);
    t.undo(); // suspend
    assert_eq!(t.card(a), a_before);
    assert_eq!(t.session(s.id).counts.new, 3);
    // Unchanged requests are no-ops (no undo step).
    let r = t.ok::<StudySuspend>(SuspendCards {
        card_ids: vec![a],
        suspended: false,
        session_id: None,
    });
    assert_eq!(r.cards, 0);
    let e = t.err::<StudySuspend>(SuspendCards {
        card_ids: vec![],
        suspended: true,
        session_id: None,
    });
    assert_eq!(e.code, ErrorCode::ValidationInvalidParams);
}

/// A note deleted mid-session drops out of the queue instead of breaking the session.
pub fn deleted_cards_leave_the_session(f: &Factory) {
    let t = T::new(f(), 15);
    let deck = t.deck("D");
    let first = t.note_of(builtin::CLASSIC_ID, deck.id, "a", "1");
    let second = t.note(deck.id, "b", "2");
    let s = t.start(StudyKind::Normal, StudyMode::Scheduled, None);
    assert_eq!(s.current.unwrap().card_id, first.cards[0].id);
    t.ok::<NotesDelete>(fc_api::v1::notes::NoteIds { ids: vec![first.id] });
    let now = t.session(s.id);
    assert_eq!(now.current.as_ref().unwrap().card_id, second);
    assert_eq!(now.counts.new, 1);
    let after = t.answer(s.id, second, Rating::Easy);
    assert!(after.current.is_none());
    assert_eq!(after.counts, StudyCounts::default());
}

/// Postpone moves today's due cards N days and is undone exactly.
pub fn postpone_moves_due_cards_and_undoes(f: &Factory) {
    let t = T::new(f(), 10);
    let deck = t.deck("D");
    let a = t.note(deck.id, "a", "1");
    let b = t.note(deck.id, "b", "2");
    let s = t.start(StudyKind::Normal, StudyMode::Scheduled, None);
    t.answer(s.id, a, Rating::Easy);
    t.answer(s.id, b, Rating::Easy);
    t.advance(40 * MILLIS_PER_DAY); // both overdue now
    let before: Vec<Card> = vec![t.card(a), t.card(b)];
    let r = t.ok::<StudyPostpone>(PostponeCards {
        deck_id: Some(deck.id),
        include_subdecks: None,
        days: 3,
        tz_offset_mins: Some(180),
    });
    assert_eq!(r.cards, 2);
    for c in [a, b] {
        assert_eq!(t.card(c).scheduling.due, Some(t.now().plus_days(3)));
    }
    // v1.3: nothing due today any more — no session starts (it used to open an empty one).
    assert!(t.try_start(StudyKind::Normal, StudyMode::Scheduled, None).is_none());
    t.undo();
    assert_eq!(vec![t.card(a), t.card(b)], before);
    for days in [0, 400] {
        let e = t.err::<StudyPostpone>(PostponeCards {
            deck_id: None,
            include_subdecks: None,
            days,
            tz_offset_mins: None,
        });
        assert_eq!(e.code, ErrorCode::ValidationInvalidParams);
    }
}

/// Hints reveal more step by step and are counted in the review; typed answers are graded.
pub fn hints_and_answer_checking(f: &Factory) {
    let t = T::new(f(), 11);
    let deck = t.deck("D");
    let card = t.note(deck.id, "dog", "<b>köpek</b>");
    let s = t.start(StudyKind::Normal, StudyMode::Scheduled, None);
    let hint = |_n: u32| t.ok::<StudyHint>(HintRequest { session_id: s.id, card_id: card });
    let h1 = hint(1);
    assert_eq!((h1.level, h1.text.as_deref()), (1, Some("k____")));
    let h2 = hint(2);
    assert_eq!((h2.level, h2.text.as_deref()), (2, Some("kö___")));
    assert_eq!(t.session(s.id).current.unwrap().hints_used, 2);
    let check = |given: &str| {
        t.ok::<StudyCheckAnswer>(CheckAnswer {
            session_id: s.id,
            card_id: card,
            given: given.into(),
            choice: None,
        })
    };
    let ok = check("Kopek");
    assert_eq!((ok.grade, ok.suggested_rating), (AnswerGrade::Correct, Rating::Good));
    assert_eq!(ok.expected, "köpek");
    assert!(!ok.auto_grade);
    let close = check("kopk");
    assert_eq!((close.grade, close.suggested_rating), (AnswerGrade::Almost, Rating::Hard));
    assert!(close.diff.iter().any(|d| d.kind == DiffKind::Missing));
    assert_eq!(check("kedi").grade, AnswerGrade::Wrong);
    t.set(super::ANSWER_CHECKER, serde_json::json!("exact"));
    assert_eq!(check("kopek").grade, AnswerGrade::Wrong);
    t.answer(s.id, card, Rating::Hard);
    assert_eq!(t.history(card).reviews[0].hints_used, 2);
    // Hint strategy is a setting; typing is a deck-level setting.
    t.set(super::TYPE_ANSWER, serde_json::json!(true));
    let s2 = t.start(StudyKind::Custom, StudyMode::Trial, None);
    assert!(s2.current.unwrap().type_answer);
}

/// SM-2 can replace FSRS through the `study.scheduler` setting.
pub fn sm2_scheduler_is_selectable(f: &Factory) {
    let t = T::new(f(), 12);
    let deck = t.deck("D");
    let card = t.note(deck.id, "a", "b");
    t.set(super::SCHEDULER, serde_json::json!("sm2"));
    let s = t.start(StudyKind::Normal, StudyMode::Scheduled, None);
    let easy = s.current.unwrap().intervals[3];
    assert_eq!(easy.scheduled_days, 4, "SM-2 easy graduation");
    t.answer(s.id, card, Rating::Easy);
    assert_eq!(t.card(card).scheduling.difficulty, Some(2.5), "ease factor");
}

/// Errors: unknown session, card not in the session, ended session, disabled module.
pub fn study_errors(f: &Factory) {
    let t = T::new(f(), 13);
    let deck = t.deck("D");
    let a = t.note(deck.id, "a", "1");
    let s = t.start(StudyKind::Normal, StudyMode::Scheduled, None);
    let outside = t.note(deck.id, "b", "2"); // added after the queue was built
    let e = t.err::<StudyAnswer>(AnswerCard {
        session_id: s.id,
        card_id: outside,
        rating: Rating::Good,
        duration_ms: 1,
        reveal_ms: None,
        typed_answer: None,
    });
    assert_eq!(e.code, ErrorCode::ConflictNotCurrentCard);
    let e = t.err::<StudySession>(SessionRef {
        session_id: SessionId(uuid::Uuid::from_u128(5)),
    });
    assert_eq!(e.code, ErrorCode::NotFoundSession);
    t.ok::<StudyEnd>(SessionRef { session_id: s.id });
    let e = t.err::<StudyAnswer>(AnswerCard {
        session_id: s.id,
        card_id: a,
        rating: Rating::Good,
        duration_ms: 1,
        reveal_ms: None,
        typed_answer: None,
    });
    assert_eq!(e.code, ErrorCode::ConflictSessionEnded);
    assert!(t.session(s.id).current.is_none());
    let e = t.err::<StudyStart>(StartSession {
        kind: StudyKind::Normal,
        mode: None,
        deck_id: None,
        include_subdecks: None,
        filter: None,
        from_session: None,
        new_limit: None,
        order: None,
        tz_offset_mins: Some(10_000),
        modes: None,
        scope: None,
    });
    assert_eq!(e.code, ErrorCode::ValidationInvalidParams);
    // Rule 7: the module can be switched off.
    t.ok::<ModulesSetEnabled>(SetModuleEnabled {
        id: "study".into(),
        enabled: false,
    });
    let e = t.err::<StudyActive>(Empty {});
    assert_eq!(e.code, ErrorCode::ConflictModuleDisabled);
}

/// Normal sessions take the configured number of new cards; history lists reviews.
pub fn new_limit_and_order(f: &Factory) {
    let t = T::new(f(), 14);
    let deck = t.deck("D");
    let cards: Vec<CardId> = (0..5).map(|i| t.note(deck.id, &format!("q{i}"), "a")).collect();
    t.set(super::NEW_PER_SESSION, serde_json::json!(2));
    let s = t.start(StudyKind::Normal, StudyMode::Scheduled, None);
    assert_eq!(s.counts.new, 2);
    assert_eq!(s.current.unwrap().card_id, cards[0], "added order");
    let s = t.ok::<StudyStart>(StartSession {
        kind: StudyKind::Normal,
        mode: None,
        deck_id: None,
        include_subdecks: None,
        filter: None,
        from_session: None,
        new_limit: Some(10),
        order: Some(StudyOrder::NewestFirst),
        tz_offset_mins: None,
        modes: None,
        scope: None,
    });
    assert_eq!(s.counts.new, 5);
    assert_eq!(s.current.unwrap().card_id, cards[4]);
}

macro_rules! study_suite {
    ($factory:expr) => {
        study_suite!(@tests $factory;
            answer_undo_redo_is_exact,
            sibling_burying_is_undone_exactly,
            review_lapse_undo_is_exact,
            undo_does_not_reopen_ended_session,
            trial_mode_never_changes_cards,
            simulation_365_days,
            learning_cards_come_back_in_the_session,
            session_survives_restart,
            mistakes_session_repeats_again_cards,
            custom_session_filters_by_classification,
            suspend_and_bury_in_session_with_undo,
            postpone_moves_due_cards_and_undoes,
            deleted_cards_leave_the_session,
            hints_and_answer_checking,
            sm2_scheduler_is_selectable,
            study_errors,
            new_limit_and_order,
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
    use crate::testing::MemStore;
    use fc_storage::Store;
    use std::sync::Arc;
    study_suite!(|| -> Arc<dyn Store> { Arc::new(MemStore::new()) });
}

mod sqlite_memory {
    use fc_storage::{SqliteStore, Store};
    use std::sync::Arc;
    study_suite!(|| -> Arc<dyn Store> { Arc::new(SqliteStore::open_in_memory().unwrap()) });
}

mod sqlite_file {
    use crate::testing::TempSqlite;
    use fc_storage::Store;
    use std::sync::Arc;
    study_suite!(|| -> Arc<dyn Store> { Arc::new(TempSqlite::new()) });
}

/// v1.3 study launcher (`study.options`, scoped starts, resume of every kind).
#[path = "launcher_tests.rs"]
mod launcher;

#[test]
fn v02_internal_methods() {
    assert_eq!(
        crate::tests::internal_methods_of(&crate::testing::TestApp::mem(), &["study"]),
        ["study.restore"]
    );
}

#[test]
fn answer_text_takes_the_part_after_the_rule() {
    use super::engine::answer_text;
    assert_eq!(answer_text("q\n<hr class=\"fc-answer\">\n<b>a</b>"), "a");
    assert_eq!(answer_text("q<hr>a"), "a");
    assert_eq!(answer_text("<p>only</p>"), "only");
}
