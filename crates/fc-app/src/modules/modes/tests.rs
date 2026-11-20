//! Service tests of the v0.10 study modes, each on MemStore, SQLite in memory and SQLite on a
//! temp file (`modes_suite!` at the bottom). One test per acceptance criterion (docs/04 v0.10):
//!
//! * learn mode: learned only when correct at every stage, targets a classification →
//!   [`learn_mode_learns_cards_at_every_stage`];
//! * test generator: mixed types, printable key, score, classification target, never
//!   schedules, undoable → [`test_generator_mixes_types_scores_and_never_schedules`];
//! * confidence scale mapped onto FSRS ratings → [`confidence_levels_map_onto_fsrs_ratings`];
//! * timed mode on the backend clock → [`timed_mode_is_enforced_on_the_backend_clock`]; the
//!   countdown belongs to the card on screen → [`timed_countdown_belongs_to_the_card_on_screen`];
//! * learn mode: recognition is not recall →
//!   [`learn_choice_answers_do_not_rate_reviewed_cards`];
//! * own `review_log.mode` per mode, statistics separate them, daily limits →
//!   [`modes_log_their_own_review_mode_and_respect_daily_limits`];
//! * inline card creation during study → [`inline_cards_during_study_are_one_undo_step`];
//! * every mode is a switchable module → [`modes_are_switchable_modules`].
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::{App, AppDeps};
use fc_api::v1::attributes::{AttributeAssignment, AttributeDefDto, DefineAttributeFromTemplate};
use fc_api::v1::common::Empty;
use fc_api::v1::decks::{CreateDeck, DeckDto};
use fc_api::v1::methods::*;
use fc_api::v1::modes::*;
use fc_api::v1::modules::SetModuleEnabled;
use fc_api::v1::notes::{CreateNote, NoteDto};
use fc_api::v1::oplog::Actor;
use fc_api::v1::presets::{AssignPreset, CreatePreset};
use fc_api::v1::settings::SetSetting;
use fc_api::v1::stats::StatsRequest;
use fc_api::v1::study::*;
use fc_api::{ApiError, ApiMethod, ErrorCode};
use fc_core::attribute::AttributeValue::Choice;
use fc_core::card::{Card, CardState, Rating};
use fc_core::i18n::Locale;
use fc_core::notetype::builtin;
use fc_core::preset::PresetConfig;
use fc_core::rng::SplitMix64;
use fc_core::settings::SettingScope;
use fc_core::time::FixedClock;
use fc_core::{CardId, DeckId, SessionId, Timestamp, V7IdGen};
use fc_storage::repo::{CardFilter, ReviewLogRecord};
use fc_storage::Store;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

/// 2026-03-02T10:00Z.
const T0: i64 = 1_772_445_600_000;

type Factory = dyn Fn() -> Arc<dyn Store>;

struct T {
    app: App,
    store: Arc<dyn Store>,
    clock: Arc<FixedClock>,
    /// Card → its answer (the note's back).
    answers: std::cell::RefCell<HashMap<CardId, String>>,
}

impl T {
    fn new(store: Arc<dyn Store>, seed: u64) -> Self {
        let clock = Arc::new(FixedClock::new(Timestamp(T0)));
        let app = App::new(AppDeps {
            store: store.clone(),
            ids: Box::new(V7IdGen::new(clock.clone(), SplitMix64::new(seed))),
            clock: clock.clone(),
            events: Arc::new(crate::events::InMemoryEventBus::new()),
        })
        .unwrap();
        app.load_state().unwrap();
        Self {
            app,
            store,
            clock,
            answers: Default::default(),
        }
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

    fn template(&self, id: &str, deck: DeckId) -> AttributeDefDto {
        self.ok::<AttributesDefineFromTemplate>(DefineAttributeFromTemplate {
            template_id: id.into(),
            scope_deck_id: Some(deck),
            locale: Some(Locale::Tr),
        })
    }

    fn note(&self, deck: DeckId, front: &str, back: &str, attr: Option<(&AttributeDefDto, &str)>) -> CardId {
        let n: NoteDto = self.ok::<NotesCreate>(CreateNote {
            notetype_id: builtin::CLASSIC_ID,
            deck_id: deck,
            fields: BTreeMap::from([
                ("front".to_owned(), front.to_owned()),
                ("back".to_owned(), back.to_owned()),
            ]),
            tags: vec![],
            attributes: attr
                .map(|(def, label)| {
                    vec![AttributeAssignment {
                        attr_id: def.id,
                        value: Some(Choice(
                            def.options.iter().find(|o| o.label == label).unwrap().id,
                        )),
                    }]
                })
                .unwrap_or_default(),
            allow_duplicate: None,
        });
        let id = n.cards[0].id;
        self.answers.borrow_mut().insert(id, back.to_owned());
        id
    }

    /// The screen shows the session's current card: its countdown starts (timed mode).
    fn start_timer(&self, s: &SessionDto) -> SessionDto {
        self.ok::<ModesTimedStart>(StartTimer {
            session_id: s.id,
            card_id: s.current.as_ref().unwrap().card_id,
        })
    }

    fn answer_of(&self, card: CardId) -> String {
        self.answers.borrow()[&card].clone()
    }

    fn start(&self, kind: StudyKind, mode: StudyMode, deck: DeckId, query: Option<&str>, modes: Option<StartModes>) -> SessionDto {
        self.try_start(kind, mode, deck, query, modes).unwrap()
    }

    fn try_start(
        &self,
        kind: StudyKind,
        mode: StudyMode,
        deck: DeckId,
        query: Option<&str>,
        modes: Option<StartModes>,
    ) -> Result<SessionDto, ApiError> {
        self.app.call::<StudyStart>(
            &StartSession {
                kind,
                mode: Some(mode),
                deck_id: Some(deck),
                include_subdecks: None,
                filter: query.map(|q| StudyFilter {
                    query: Some(q.to_owned()),
                    ..StudyFilter::default()
                }),
                from_session: None,
                new_limit: None,
                order: Some(StudyOrder::Due),
                tz_offset_mins: Some(0),
                modes,
            },
            &Actor::User,
        )
    }

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

    fn reviews(&self, card: CardId) -> Vec<ReviewLogRecord> {
        self.store.read().unwrap().review_logs_for_card(card).unwrap()
    }

    fn all_reviews(&self) -> Vec<ReviewLogRecord> {
        let ids: Vec<CardId> = self.all_cards().iter().map(|c| c.id).collect();
        ids.iter().flat_map(|c| self.reviews(*c)).collect()
    }

    fn session(&self, id: SessionId) -> SessionDto {
        self.ok::<StudySession>(SessionRef { session_id: id })
    }

    fn end(&self, id: SessionId) {
        self.ok::<StudyEnd>(SessionRef { session_id: id });
    }

    fn undo(&self) {
        self.ok::<OplogUndo>(Empty {});
    }

    fn redo(&self) {
        self.ok::<OplogRedo>(Empty {});
    }

    fn set(&self, scope: SettingScope, key: &str, value: serde_json::Value) {
        self.ok::<SettingsSet>(SetSetting {
            scope,
            key: key.into(),
            value,
        });
    }

    fn module(&self, id: &str, enabled: bool) {
        self.ok::<ModulesSetEnabled>(SetModuleEnabled {
            id: id.into(),
            enabled,
        });
    }

    fn learn(&self, s: SessionId, card: CardId, choice: Option<u32>, given: Option<&str>) -> LearnAnswerResult {
        self.ok::<ModesLearnAnswer>(LearnAnswer {
            session_id: s,
            card_id: card,
            choice,
            given: given.map(str::to_owned),
            duration_ms: 1_000,
        })
    }

    fn reviews_total(&self, fresh: bool) -> u32 {
        self.ok::<StatsGetOverview>(StatsRequest {
            scope: Default::default(),
            tz_offset_mins: Some(0),
            days: None,
            forecast_days: None,
            fresh: Some(fresh),
        })
        .reviews
        .totals
        .reviews
    }
}

fn learn_of(s: &SessionDto) -> &LearnDto {
    s.modes.as_ref().unwrap().learn.as_ref().unwrap()
}

/// Answers the shown learn card right (`true`) or wrong at its current stage.
fn learn_step(t: &T, s: &SessionDto, right: bool) -> LearnAnswerResult {
    let card = s.current.as_ref().unwrap().card_id;
    let expected = t.answer_of(card);
    let l = learn_of(s);
    match l.stage.unwrap() {
        LearnStage::Choice => {
            let correct = l.options.iter().position(|o| *o == expected).unwrap() as u32;
            let pick = if right { correct } else { (correct + 1) % l.options.len() as u32 };
            t.learn(s.id, card, Some(pick), None)
        }
        LearnStage::Written => {
            let given = if right { expected } else { "yanlış cevap".to_owned() };
            t.learn(s.id, card, None, Some(&given))
        }
    }
}

/// Quizlet-style learn mode: a learn session targets a classification (`seviye:A2`); every
/// card is asked as multiple choice, then written; it is learned only after being correct at
/// both stages (a wrong written answer sends it back to multiple choice); answers schedule the
/// card through the engine (`review_log.mode = learn`) and undo is exact.
pub fn learn_mode_learns_cards_at_every_stage(f: &Factory) {
    let t = T::new(f(), 1);
    let en = t.deck("İngilizce");
    let level = t.template("cefr", en.id);
    let a2: Vec<CardId> = ["badger", "otter", "heron", "lynx"]
        .iter()
        .zip(["porsuk", "su samuru", "balıkçıl", "vaşak"])
        .map(|(f, b)| t.note(en.id, f, b, Some((&level, "A2"))))
        .collect();
    let b1 = t.note(en.id, "wolverine", "porsuk kurdu", Some((&level, "B1")));
    let mut s = t.start(StudyKind::Learn, StudyMode::Scheduled, en.id, Some("seviye:A2"), None);
    assert_eq!(s.kind, StudyKind::Learn);
    assert_eq!(learn_of(&s).progress.total, 4, "only the A2 cards");
    // `study.answer` does not answer learn cards: they are judged.
    let first = s.current.as_ref().unwrap().card_id;
    let e = t.err::<StudyAnswer>(AnswerCard {
        session_id: s.id,
        card_id: first,
        rating: Rating::Good,
        duration_ms: 1,
        reveal_ms: None,
    });
    assert_eq!(e.code, ErrorCode::ValidationSessionMode);
    // A multiple-choice question has 4 distinct options, one of them right.
    let l = learn_of(&s);
    assert_eq!(l.stage, Some(LearnStage::Choice));
    assert_eq!(l.options.len(), 4);
    assert_eq!(l.options.iter().filter(|o| **o == t.answer_of(first)).count(), 1);

    // Undo of a learn answer is exact: card, session (progress, queue) and review log.
    let cards_before = t.all_cards();
    let r = learn_step(&t, &s, false);
    assert_eq!((r.grade, r.stage, r.learned), (AnswerGrade::Wrong, LearnStage::Choice, false));
    assert_eq!(t.reviews(first).len(), 1);
    t.undo();
    assert_eq!(t.all_cards(), cards_before);
    assert!(t.reviews(first).is_empty());
    assert_eq!(t.session(s.id), s);
    t.redo();
    s = t.session(s.id);
    assert_eq!(t.reviews(first).len(), 1);

    // Card `first`: choice wrong (done) → choice right → written wrong (back to choice) →
    // choice right → written right → learned. Every other card: choice right → written right.
    let mut stages: HashMap<CardId, Vec<(LearnStage, AnswerGrade, bool)>> = HashMap::new();
    stages.entry(first).or_default().push((LearnStage::Choice, AnswerGrade::Wrong, false));
    let mut wrong_written_done = false;
    let mut guard = 0;
    while let Some(c) = s.current.clone() {
        guard += 1;
        assert!(guard < 50, "learn session does not end");
        assert!(a2.contains(&c.card_id), "only A2 cards are asked");
        let stage = learn_of(&s).stage.unwrap();
        let right = !(c.card_id == first && stage == LearnStage::Written && !wrong_written_done);
        if !right {
            wrong_written_done = true;
        }
        let r = learn_step(&t, &s, right);
        stages
            .entry(c.card_id)
            .or_default()
            .push((r.stage, r.grade, r.learned));
        s = r.session;
    }
    use AnswerGrade::{Correct, Wrong};
    use LearnStage::{Choice as C, Written as W};
    assert_eq!(
        stages[&first],
        vec![
            (C, Wrong, false),
            (C, Correct, false),
            (W, Wrong, false),
            (C, Correct, false),
            (W, Correct, true)
        ]
    );
    for c in &a2[..] {
        if *c != first {
            assert_eq!(stages[c], vec![(C, Correct, false), (W, Correct, true)]);
        }
        // Learned only after a right written answer that followed a right choice answer.
        let log = &stages[c];
        let learned_at = log.iter().position(|x| x.2).unwrap();
        assert_eq!(learned_at, log.len() - 1);
        assert_eq!(log[learned_at].0, W);
        assert_eq!(log[learned_at - 1], (C, Correct, false));
    }
    let p = learn_of(&s).progress;
    assert_eq!((p.total, p.learned, p.choice, p.written), (4, 4, 0, 0));
    // Every answer scheduled the card and was logged as `learn` with the judged rating.
    for c in &a2 {
        let logs = t.reviews(*c);
        assert_eq!(logs.len(), stages[c].len());
        assert!(logs.iter().all(|r| r.mode == "learn"));
        assert_eq!(logs.last().unwrap().rating, Rating::Good);
        assert_ne!(t.card(*c).scheduling.state, CardState::New);
    }
    assert_eq!(t.reviews(first)[0].rating, Rating::Again);
    assert_eq!(t.card(b1).scheduling.state, CardState::New, "B1 was not in the session");
}

/// Recognition is not recall (orchestrator decision on review #7): in a learn session, a
/// multiple-choice answer on a card already in review is practice only — schedule unchanged,
/// logged `trial_learn`, not counted by statistics; the written stage rates it (correct →
/// Good, wrong → Again). New cards are rated at both stages.
pub fn learn_choice_answers_do_not_rate_reviewed_cards(f: &Factory) {
    let t = T::new(f(), 9);
    let d = t.deck("Tanıma");
    let cards: Vec<CardId> = ["ayı", "kurt", "tilki", "geyik"]
        .iter()
        .zip(["bear", "wolf", "fox", "deer"])
        .map(|(f, b)| t.note(d.id, f, b, None))
        .collect();
    // Graduate every card to review ("Easy" on a new card).
    let mut s = t.start(StudyKind::Normal, StudyMode::Scheduled, d.id, None, None);
    while let Some(c) = s.current.clone() {
        s = t.ok::<StudyAnswer>(AnswerCard { session_id: s.id, card_id: c.card_id, rating: Rating::Easy, duration_ms: 1_000, reveal_ms: None });
    }
    t.end(s.id);
    assert!(cards.iter().all(|c| t.card(*c).scheduling.state == CardState::Review));
    let reviewed = t.all_cards();
    t.ok::<StatsRefreshCache>(Default::default());
    let counted = t.reviews_total(true);

    let s = t.start(StudyKind::Learn, StudyMode::Scheduled, d.id, None, None);
    let first = s.current.as_ref().unwrap().card_id;
    // Multiple choice, wrong then right: the card moves to the written stage, its schedule
    // does not move.
    let r = learn_step(&t, &s, false);
    assert_eq!((r.stage, r.grade), (LearnStage::Choice, AnswerGrade::Wrong));
    let mut s = r.session;
    while s.current.as_ref().unwrap().card_id != first || learn_of(&s).stage != Some(LearnStage::Choice) {
        let r = learn_step(&t, &s, true);
        s = r.session;
    }
    let r = learn_step(&t, &s, true);
    assert_eq!((r.stage, r.grade), (LearnStage::Choice, AnswerGrade::Correct));
    assert_eq!(t.all_cards(), reviewed, "recognition changed no schedule");
    let logs: Vec<_> = t.reviews(first).into_iter().skip(1).collect(); // after the "Easy"
    assert_eq!(logs.len(), 2);
    assert!(logs.iter().all(|l| l.mode == "trial_learn"));
    assert_eq!(logs[0].rating, Rating::Again);
    assert_eq!(logs[0].state_after, CardState::Review, "logged without a schedule change");
    t.ok::<StatsRefreshCache>(Default::default());
    assert_eq!(t.reviews_total(true), counted, "practice is not counted");
    assert_eq!(t.reviews_total(false), counted);
    let mut s = r.session;
    // The written stage rates the card: right → Good.
    while s.current.as_ref().unwrap().card_id != first {
        s = learn_step(&t, &s, true).session;
    }
    assert_eq!(learn_of(&s).stage, Some(LearnStage::Written));
    let r = learn_step(&t, &s, true);
    assert!(r.learned);
    let last = t.reviews(first).last().cloned().unwrap();
    assert_eq!((last.mode.as_str(), last.rating), ("learn", Rating::Good));
    assert_ne!(t.card(first).scheduling, reviewed.iter().find(|c| c.id == first).unwrap().scheduling);
    t.end(r.session.id);
    // A wrong written answer on a reviewed card is "Again" (it lapses).
    let s = t.start(StudyKind::Learn, StudyMode::Scheduled, d.id, None, None);
    let c = s.current.as_ref().unwrap().card_id;
    assert_eq!(t.card(c).scheduling.state, CardState::Review);
    let mut s = learn_step(&t, &s, true).session;
    while s.current.as_ref().unwrap().card_id != c {
        s = learn_step(&t, &s, true).session;
    }
    assert_eq!(learn_of(&s).stage, Some(LearnStage::Written));
    learn_step(&t, &s, false);
    let last = t.reviews(c).last().cloned().unwrap();
    assert_eq!((last.mode.as_str(), last.rating), ("learn", Rating::Again));
    assert_eq!(t.card(c).scheduling.state, CardState::Relearning);
}

fn exam_fixture(t: &T) -> (DeckDto, Vec<CardId>, Vec<CardId>) {
    let yks = t.deck("YKS");
    let exam = t.template("yks", yks.id);
    let tyt: Vec<CardId> = (1..=6)
        .map(|i| t.note(yks.id, &format!("TYT soru {i}"), &format!("tyt cevap {i}"), Some((&exam, "TYT"))))
        .collect();
    let ayt: Vec<CardId> = (1..=3)
        .map(|i| t.note(yks.id, &format!("AYT soru {i}"), &format!("ayt cevap {i}"), Some((&exam, "AYT"))))
        .collect();
    (yks, tyt, ayt)
}

/// Test generator: a TYT practice test (`sınav:TYT`) of mixed question types; the answer key
/// is printable on request and returned after submission; scoring counts right, typo-level,
/// wrong and blank answers; one `test` review per question; `cards` never changes; submission
/// is one undo step and cannot be repeated.
pub fn test_generator_mixes_types_scores_and_never_schedules(f: &Factory) {
    let t = T::new(f(), 2);
    let (yks, tyt, _ayt) = exam_fixture(&t);
    let cards_before = t.all_cards();
    let s = t.start(
        StudyKind::Test,
        StudyMode::Scheduled,
        yks.id,
        Some("sınav:TYT"),
        Some(StartModes {
            test: Some(TestOptions {
                questions: Some(6),
                kinds: None,
            }),
            ..StartModes::default()
        }),
    );
    let info = s.modes.as_ref().unwrap().test.unwrap();
    assert_eq!((info.questions, info.submitted), (6, false));
    assert!(s.current.is_none(), "a test is answered as a whole");
    let paper = t.ok::<ModesTestPaper>(TestPaperRequest {
        session_id: s.id,
        with_key: None,
    });
    assert_eq!(paper.questions.len(), 6);
    assert!(paper.questions.iter().all(|q| tyt.contains(&q.card_id) && q.key.is_none()));
    let kinds = |k| paper.questions.iter().filter(|q| q.kind == k).count();
    assert_eq!(
        (
            kinds(TestQuestionKind::TrueFalse),
            kinds(TestQuestionKind::MultipleChoice),
            kinds(TestQuestionKind::Written)
        ),
        (2, 2, 2),
        "mixed types"
    );
    // Printable answer sheet.
    let keyed = t.ok::<ModesTestPaper>(TestPaperRequest {
        session_id: s.id,
        with_key: Some(true),
    });
    assert!(keyed.questions.iter().all(|q| q.key.is_some()));
    // Answers: right except one wrong multiple choice, one typo, one blank.
    let mut answers = Vec::new();
    let mut wrong_done = false;
    let mut typo_done = false;
    for q in &keyed.questions {
        let key = q.key.clone().unwrap();
        let mut a = TestAnswerDto {
            index: q.index,
            ..TestAnswerDto::default()
        };
        match q.kind {
            TestQuestionKind::MultipleChoice if !wrong_done => {
                wrong_done = true;
                a.choice = Some((key.option.unwrap() + 1) % q.options.len() as u32);
            }
            TestQuestionKind::MultipleChoice => a.choice = key.option,
            TestQuestionKind::TrueFalse => a.truth = key.truth,
            TestQuestionKind::Written if !typo_done => {
                typo_done = true;
                a.text = Some(key.expected.replace("cevap", "cevab"));
            }
            TestQuestionKind::Written => continue, // blank
        }
        answers.push(a);
    }
    let graded = t.ok::<ModesTestSubmit>(SubmitTest {
        session_id: s.id,
        answers: answers.clone(),
        duration_ms: 60_000,
    });
    let r = graded.result.clone().unwrap();
    assert_eq!((r.total, r.correct, r.almost, r.wrong, r.blank), (6, 3, 1, 1, 1));
    assert_eq!(r.points, 3.5);
    assert_eq!(r.percent, 58);
    assert!(graded.questions.iter().all(|q| q.key.is_some()));
    assert_eq!(graded.answers.as_ref().unwrap().len(), 6);
    // One `test` review per question, ratings from the verdicts; the schedule is untouched.
    let logs: Vec<_> = t.all_reviews();
    assert_eq!(logs.len(), 6);
    assert!(logs.iter().all(|l| l.mode == "test" && l.scheduled_days == 0));
    assert_eq!(logs.iter().filter(|l| l.rating == Rating::Good).count(), 3);
    assert_eq!(logs.iter().filter(|l| l.rating == Rating::Hard).count(), 1);
    assert_eq!(t.all_cards(), cards_before, "a test never changes cards");
    assert_eq!(t.reviews_total(true), 0, "statistics do not count test answers");
    let e = t.err::<ModesTestSubmit>(SubmitTest {
        session_id: s.id,
        answers,
        duration_ms: 1,
    });
    assert_eq!(e.code, ErrorCode::ConflictTestSubmitted);
    // Undo: reviews gone, paper ungraded; redo: graded again.
    t.undo();
    assert!(t.all_reviews().is_empty());
    let p = t.ok::<ModesTestPaper>(TestPaperRequest {
        session_id: s.id,
        with_key: None,
    });
    assert!(p.result.is_none() && p.questions == paper.questions);
    t.redo();
    let p = t.ok::<ModesTestPaper>(TestPaperRequest {
        session_id: s.id,
        with_key: None,
    });
    assert_eq!(p.result, graded.result);
    assert_eq!(t.all_cards(), cards_before);
    // `modes.test.*` on another kind of session.
    t.end(s.id);
    let normal = t.start(StudyKind::Normal, StudyMode::Trial, yks.id, None, None);
    let e = t.err::<ModesTestPaper>(TestPaperRequest {
        session_id: normal.id,
        with_key: None,
    });
    assert_eq!(e.code, ErrorCode::ValidationSessionMode);
}

/// Confidence rating: the five levels map through the settings table onto the four FSRS
/// ratings; a level-n answer leaves the card exactly as a four-button answer with the mapped
/// rating would (same time, same preset), logged as `confidence`. The deck setting makes the
/// scale the default; the buttons show the interval of their rating.
pub fn confidence_levels_map_onto_fsrs_ratings(f: &Factory) {
    let t = T::new(f(), 3);
    t.module("planning", false); // load balancing would spread equal cards over days
    let conf = t.deck("Güven");
    let buttons = t.deck("Düğmeler");
    let a: Vec<CardId> = (1..=5).map(|i| t.note(conf.id, &format!("c{i}"), &format!("a{i}"), None)).collect();
    let b: Vec<CardId> = (1..=5).map(|i| t.note(buttons.id, &format!("b{i}"), &format!("x{i}"), None)).collect();
    // Custom table: level 2 → hard.
    t.set(SettingScope::App, "confidence.map.2", serde_json::json!("hard"));
    let mapped = [Rating::Again, Rating::Hard, Rating::Hard, Rating::Good, Rating::Easy];
    // The deck setting chooses the scale.
    t.set(SettingScope::Deck(conf.id), "confidence.rating_scale", serde_json::json!("confidence"));
    let mut s = t.start(StudyKind::Normal, StudyMode::Scheduled, conf.id, None, None);
    let m = s.modes.clone().unwrap();
    assert_eq!(m.rating_scale, RatingScale::Confidence);
    let levels: Vec<_> = m.confidence.iter().map(|l| (l.level, l.rating)).collect();
    assert_eq!(levels, (1..=5).zip(mapped).collect::<Vec<_>>());
    let cur = s.current.clone().unwrap();
    for l in &m.confidence {
        let p = cur.intervals.iter().find(|p| p.rating == l.rating).unwrap();
        assert_eq!(l.interval_ms, p.interval_ms);
    }
    // A four-button answer is still possible; a confidence answer outside 1–5 is not.
    let e = t.err::<ModesConfidenceAnswer>(ConfidenceAnswer {
        session_id: s.id,
        card_id: cur.card_id,
        level: 6,
        duration_ms: 1,
        reveal_ms: None,
    });
    assert_eq!(e.code, ErrorCode::ValidationInvalidParams);
    let mut level_of = HashMap::new();
    for level in 1..=5u8 {
        let c = s.current.clone().unwrap().card_id;
        level_of.insert(c, level);
        s = t.ok::<ModesConfidenceAnswer>(ConfidenceAnswer {
            session_id: s.id,
            card_id: c,
            level,
            duration_ms: 2_000,
            reveal_ms: Some(1_000),
        });
    }
    t.end(s.id);
    // The same answers with the four buttons, at the same time, on twin cards.
    let mut s = t.start(StudyKind::Normal, StudyMode::Scheduled, buttons.id, None, None);
    assert!(s.modes.is_none(), "a plain session has no mode state");
    let mut level_of_b = HashMap::new();
    for level in 1..=5u8 {
        let c = s.current.clone().unwrap().card_id;
        level_of_b.insert(level, c);
        s = t.ok::<StudyAnswer>(AnswerCard {
            session_id: s.id,
            card_id: c,
            rating: mapped[level as usize - 1],
            duration_ms: 2_000,
            reveal_ms: Some(1_000),
        });
    }
    for c in &a {
        let level = level_of[c];
        let twin = level_of_b[&level];
        assert_eq!(t.card(*c).scheduling, t.card(twin).scheduling, "level {level}");
        let log = &t.reviews(*c)[0];
        assert_eq!((log.mode.as_str(), log.rating), ("confidence", mapped[level as usize - 1]));
        assert_eq!(t.reviews(twin)[0].mode, "normal");
    }
    assert_eq!(b.len(), 5);
    // Not a confidence session → refused.
    t.set(SettingScope::Deck(conf.id), "confidence.rating_scale", serde_json::json!("buttons"));
    let s = t.start(StudyKind::Custom, StudyMode::Trial, conf.id, None, None);
    let e = t.err::<ModesConfidenceAnswer>(ConfidenceAnswer {
        session_id: s.id,
        card_id: s.current.unwrap().card_id,
        level: 3,
        duration_ms: 1,
        reveal_ms: None,
    });
    assert_eq!(e.code, ErrorCode::ValidationSessionMode);
}

fn deadline(s: &SessionDto) -> Option<Timestamp> {
    s.modes.as_ref().unwrap().timer.unwrap().deadline
}

fn timer_not_started(e: &ApiError) {
    assert_eq!(e.code, ErrorCode::ValidationSessionMode);
    assert_eq!(e.details.get("reason").and_then(|v| v.as_str()), Some("timer_not_started"));
}

/// Timed mode: the limit runs on the backend clock from when the screen put the card up
/// (`modes.timed.start`); a late button answer is refused (`CONFLICT_TIME_UP`), an early
/// expiry too (`CONFLICT_TIMER_RUNNING`); expiry answers "Again" (`timed`) or skips the card
/// (no review, card unchanged); both are exactly undoable; a countdown is never restarted.
pub fn timed_mode_is_enforced_on_the_backend_clock(f: &Factory) {
    let t = T::new(f(), 4);
    let d = t.deck("Hızlı");
    for i in 1..=3 {
        t.note(d.id, &format!("q{i}"), &format!("a{i}"), None);
    }
    let timed = |on_timeout| StartModes {
        timed: Some(TimedOptions {
            seconds: 10,
            on_timeout,
        }),
        ..StartModes::default()
    };
    let s = t.start(StudyKind::Normal, StudyMode::Scheduled, d.id, None, Some(timed(TimeoutAction::Again)));
    assert_eq!(deadline(&s), None, "the countdown starts when the screen shows the card");
    let c1 = s.current.clone().unwrap().card_id;
    // A card whose countdown never started neither runs out nor takes an answer.
    t.advance(60_000);
    timer_not_started(&t.err::<ModesTimedExpire>(ExpireCard { session_id: s.id, card_id: c1 }));
    timer_not_started(&t.err::<StudyAnswer>(AnswerCard {
        session_id: s.id,
        card_id: c1,
        rating: Rating::Good,
        duration_ms: 1_000,
        reveal_ms: None,
    }));
    let s = t.start_timer(&s);
    let started = t.now();
    assert_eq!(deadline(&s), Some(started.plus_millis(10_000)));
    // Starting again while it runs buys no time.
    t.advance(2_000);
    assert_eq!(deadline(&t.start_timer(&s)), Some(started.plus_millis(10_000)));
    // Only the card on screen can be started.
    let other = t.all_cards().into_iter().map(|c| c.id).find(|id| *id != c1).unwrap();
    let e = t.err::<ModesTimedStart>(StartTimer { session_id: s.id, card_id: other });
    assert_eq!(e.code, ErrorCode::ConflictNotCurrentCard);
    // In time: an ordinary answer, logged as `timed`.
    t.advance(3_000);
    let s = t.ok::<StudyAnswer>(AnswerCard {
        session_id: s.id,
        card_id: c1,
        rating: Rating::Good,
        duration_ms: 5_000,
        reveal_ms: None,
    });
    assert_eq!(t.reviews(c1)[0].mode, "timed");
    let c2 = s.current.clone().unwrap().card_id;
    assert_eq!(deadline(&s), None, "the next card has not been shown yet");
    let s = t.start_timer(&s);
    assert_eq!(deadline(&s), Some(t.now().plus_millis(10_000)));
    // Too early to expire.
    t.advance(5_000);
    let e = t.err::<ModesTimedExpire>(ExpireCard { session_id: s.id, card_id: c2 });
    assert_eq!(e.code, ErrorCode::ConflictTimerRunning);
    // Too late to answer (the client says 1 s; the backend clock says 12 s).
    t.advance(7_000);
    let e = t.err::<StudyAnswer>(AnswerCard {
        session_id: s.id,
        card_id: c2,
        rating: Rating::Easy,
        duration_ms: 1_000,
        reveal_ms: None,
    });
    assert_eq!(e.code, ErrorCode::ConflictTimeUp);
    // ...and a run-out countdown cannot be restarted (any client: desktop, local API, plugins).
    let e = t.err::<ModesTimedStart>(StartTimer { session_id: s.id, card_id: c2 });
    assert_eq!(e.code, ErrorCode::ConflictTimeUp);
    let before = t.card(c2);
    let after = t.ok::<ModesTimedExpire>(ExpireCard { session_id: s.id, card_id: c2 });
    assert_eq!(after.modes.as_ref().unwrap().timed_out, 1);
    let log = &t.reviews(c2)[0];
    assert_eq!((log.mode.as_str(), log.rating, log.duration_ms), ("timed", Rating::Again, 10_000));
    assert_eq!(t.card(c2).scheduling.state, CardState::Learning);
    t.undo();
    assert_eq!(t.card(c2), before);
    assert!(t.reviews(c2).is_empty());
    // Undo puts the card back on screen with its countdown not started (full time again).
    let undone = t.session(s.id);
    assert_eq!(undone.current.as_ref().map(|c| c.card_id), Some(c2));
    assert_eq!((undone.answered, undone.modes.as_ref().unwrap().timed_out), (s.answered, 0));
    assert_eq!(deadline(&undone), None);
    t.redo();
    assert_eq!(t.reviews(c2).len(), 1);
    t.end(s.id);

    // Skip: the card leaves the session, no review, card unchanged; undo puts it back.
    let s = t.start(StudyKind::Normal, StudyMode::Scheduled, d.id, None, Some(timed(TimeoutAction::Skip)));
    let s = t.start_timer(&s);
    let c3 = s.current.clone().unwrap().card_id;
    let card_before = t.card(c3);
    let reviews_before = t.reviews(c3).len();
    t.advance(9_000); // within the grace period
    let after = t.ok::<ModesTimedExpire>(ExpireCard { session_id: s.id, card_id: c3 });
    assert_ne!(after.current.as_ref().map(|c| c.card_id), Some(c3));
    assert_eq!(after.modes.as_ref().unwrap().timed_out, 1);
    assert_eq!(t.card(c3), card_before);
    assert_eq!(t.reviews(c3).len(), reviews_before);
    t.undo();
    let undone = t.session(s.id);
    assert_eq!(undone.current.as_ref().unwrap().card_id, c3);
    assert_eq!(deadline(&undone), None);
    // Resume much later: the screen starts the shown card's countdown afresh (by now a
    // learning card may have come due before it).
    t.advance(60_000);
    let resumed = t.start_timer(&t.session(s.id));
    assert_eq!(deadline(&resumed), Some(t.now().plus_millis(10_000)));
    t.advance(3_000);
    t.ok::<StudyAnswer>(AnswerCard {
        session_id: s.id,
        card_id: resumed.current.unwrap().card_id,
        rating: Rating::Good,
        duration_ms: 3_000,
        reveal_ms: None,
    });
    // Options are validated: limits, kinds.
    let e = t
        .try_start(StudyKind::Normal, StudyMode::Scheduled, d.id, None, Some(StartModes {
            timed: Some(TimedOptions { seconds: 2, on_timeout: TimeoutAction::Again }),
            ..StartModes::default()
        }))
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::ValidationInvalidParams);
    let e = t
        .try_start(StudyKind::Learn, StudyMode::Scheduled, d.id, None, Some(timed(TimeoutAction::Again)))
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::ValidationSessionMode);
    let plain = t.start(StudyKind::Custom, StudyMode::Trial, d.id, None, None);
    let e = t.err::<ModesTimedStart>(StartTimer {
        session_id: plain.id,
        card_id: plain.current.unwrap().card_id,
    });
    assert_eq!(e.code, ErrorCode::ValidationSessionMode);
}

/// Review fix (v0.10): the countdown belongs to the card on screen. A card that comes on
/// screen without an answer — after the shown card is suspended or buried, or a
/// learning card coming back — has no countdown until the screen starts it, however long it
/// waited; an undone card starts afresh. So a card nobody saw is never answered "Again".
pub fn timed_countdown_belongs_to_the_card_on_screen(f: &Factory) {
    let t = T::new(f(), 8);
    let d = t.deck("Sayaç");
    for i in 1..=4 {
        t.note(d.id, &format!("s{i}"), &format!("c{i}"), None);
    }
    let s = t.start(StudyKind::Normal, StudyMode::Scheduled, d.id, None, Some(StartModes {
        timed: Some(TimedOptions { seconds: 10, on_timeout: TimeoutAction::Again }),
        ..StartModes::default()
    }));
    // Suspend, then bury the shown card: the next one waits, unstarted, for the screen.
    let mut removed = Vec::new();
    for bury in [false, true] {
        let shown = t.start_timer(&t.session(s.id));
        let c = shown.current.unwrap().card_id;
        t.advance(4_000);
        if bury {
            t.ok::<StudyBury>(BuryCards { card_ids: vec![c], buried: true, session_id: Some(s.id), tz_offset_mins: None });
        } else {
            t.ok::<StudySuspend>(SuspendCards { card_ids: vec![c], suspended: true, session_id: Some(s.id) });
        }
        removed.push(c);
        t.advance(30_000); // longer than the limit
        let next = t.session(s.id);
        let n = next.current.as_ref().unwrap().card_id;
        assert!(!removed.contains(&n));
        assert_eq!(deadline(&next), None, "the next card's countdown has not started");
        timer_not_started(&t.err::<ModesTimedExpire>(ExpireCard { session_id: s.id, card_id: n }));
    }
    // Undo (the bury): the card back on screen gets its full time, though its old countdown
    // ran out long ago.
    t.undo();
    let back = t.session(s.id);
    assert_eq!(back.current.as_ref().unwrap().card_id, removed[1]);
    assert_eq!(deadline(&back), None);
    let back = t.start_timer(&back);
    assert_eq!(deadline(&back), Some(t.now().plus_millis(10_000)));
    // A learning card coming back after "Again" starts a new countdown.
    let c = removed[1];
    t.advance(1_000);
    let mut s2 = t.ok::<StudyAnswer>(AnswerCard { session_id: s.id, card_id: c, rating: Rating::Again, duration_ms: 1_000, reveal_ms: None });
    let mut guard = 0;
    while s2.current.as_ref().map(|x| x.card_id) != Some(c) {
        guard += 1;
        assert!(guard < 20);
        match &s2.current {
            Some(other) => {
                let id = other.card_id;
                assert_eq!(deadline(&s2), None);
                t.start_timer(&s2);
                s2 = t.ok::<StudyAnswer>(AnswerCard { session_id: s.id, card_id: id, rating: Rating::Easy, duration_ms: 1_000, reveal_ms: None });
            }
            None => {
                t.advance(60_000);
                s2 = t.session(s.id);
            }
        }
    }
    t.advance(120_000);
    let again = t.session(s.id);
    assert_eq!(again.current.as_ref().unwrap().card_id, c);
    assert_eq!(deadline(&again), None, "not the countdown of its first showing");
    let again = t.start_timer(&again);
    assert_eq!(deadline(&again), Some(t.now().plus_millis(10_000)));
}

/// Every mode writes `review_log` with its own `mode`: schedule-changing `learn`,
/// `confidence`, `timed` (counted by statistics, cached = fresh) and practice `trial_learn`,
/// `trial_timed`, `test` (not counted, `cards` unchanged). Daily limits: hand-picked sessions
/// (learn, test) are not limited, but every schedule-changing answer counts against the
/// deck's `new_per_day` for later normal sessions (docs/10 §24).
pub fn modes_log_their_own_review_mode_and_respect_daily_limits(f: &Factory) {
    let t = T::new(f(), 5);
    let d = t.deck("Limitli");
    let preset = t.ok::<PresetsCreate>(CreatePreset {
        name: "Dört".into(),
        config: Some(PresetConfig {
            new_per_day: 4,
            ..PresetConfig::default()
        }),
    });
    t.ok::<PresetsAssign>(AssignPreset {
        deck_ids: vec![d.id],
        preset_id: Some(preset.id),
    });
    for i in 1..=12 {
        t.note(d.id, &format!("soru {i}"), &format!("cevap {i}"), None);
    }
    let new_today = |t: &T| {
        let s = t.start(StudyKind::Normal, StudyMode::Trial, d.id, None, None);
        t.end(s.id);
        s.counts.new
    };
    assert_eq!(new_today(&t), 4);
    // Learn takes the whole hand-picked set (not the daily 4); one learn answer introduces a card.
    let s = t.start(StudyKind::Learn, StudyMode::Scheduled, d.id, None, None);
    assert_eq!(learn_of(&s).progress.total, 12);
    let r = learn_step(&t, &s, true);
    t.end(r.session.id);
    assert_eq!(new_today(&t), 3, "a learn answer counts as a new card today");
    // A confidence answer.
    let s = t.start(StudyKind::Normal, StudyMode::Scheduled, d.id, None, Some(StartModes {
        rating_scale: Some(RatingScale::Confidence),
        ..StartModes::default()
    }));
    let c = s.current.unwrap().card_id;
    t.ok::<ModesConfidenceAnswer>(ConfidenceAnswer { session_id: s.id, card_id: c, level: 4, duration_ms: 1_000, reveal_ms: None });
    t.end(s.id);
    assert_eq!(new_today(&t), 2);
    // A timed answer.
    let timed = StartModes {
        timed: Some(TimedOptions { seconds: 30, on_timeout: TimeoutAction::Again }),
        ..StartModes::default()
    };
    let s = t.start_timer(&t.start(StudyKind::Normal, StudyMode::Scheduled, d.id, None, Some(timed.clone())));
    let c = s.current.unwrap().card_id;
    t.ok::<StudyAnswer>(AnswerCard { session_id: s.id, card_id: c, rating: Rating::Good, duration_ms: 1, reveal_ms: None });
    t.end(s.id);
    assert_eq!(new_today(&t), 1);
    let scheduled_cards = t.all_cards();
    // Practice: trial learn, trial timed, test — none counts, none changes cards.
    let s = t.start(StudyKind::Learn, StudyMode::Trial, d.id, None, None);
    let r = learn_step(&t, &s, true);
    t.end(r.session.id);
    let s = t.start_timer(&t.start(StudyKind::Custom, StudyMode::Trial, d.id, None, Some(timed)));
    let c = s.current.unwrap().card_id;
    t.ok::<StudyAnswer>(AnswerCard { session_id: s.id, card_id: c, rating: Rating::Good, duration_ms: 1, reveal_ms: None });
    t.end(s.id);
    let s = t.start(StudyKind::Test, StudyMode::Scheduled, d.id, None, Some(StartModes {
        test: Some(TestOptions { questions: Some(3), kinds: None }),
        ..StartModes::default()
    }));
    t.ok::<ModesTestSubmit>(SubmitTest { session_id: s.id, answers: vec![], duration_ms: 1_000 });
    t.end(s.id);
    assert_eq!(t.all_cards(), scheduled_cards, "practice never changes cards");
    assert_eq!(new_today(&t), 1, "practice does not count against limits");
    // Each mode is separable in the log.
    let mut modes: Vec<String> = t.all_reviews().into_iter().map(|r| r.mode).collect();
    modes.sort();
    assert_eq!(
        modes,
        vec!["confidence", "learn", "test", "test", "test", "timed", "trial_learn", "trial_timed"]
    );
    // Statistics count the schedule-changing ones (cache = recomputation).
    t.ok::<StatsRefreshCache>(Default::default());
    assert_eq!(t.reviews_total(true), 3);
    assert_eq!(t.reviews_total(false), 3);
}

/// Inline card creation during study: `question :: answer` lines become Classic notes (preview
/// marks duplicates, which are skipped), `<`/`&` stay text, the running session is untouched
/// and the whole creation is one undo step.
pub fn inline_cards_during_study_are_one_undo_step(f: &Factory) {
    let t = T::new(f(), 6);
    let d = t.deck("Okuma");
    t.note(d.id, "porsuk", "badger", None);
    t.note(d.id, "vaşak", "lynx", None);
    let s = t.start(StudyKind::Normal, StudyMode::Scheduled, d.id, None, None);
    let text = "Notlar\n- porsuk :: badger\n- su samuru :: otter\n- a < b & c :: küçüktür\n{{c1::Ankara}} başkent";
    let preview = t.ok::<ModesInlinePreview>(InlinePreviewRequest {
        text: text.into(),
        deck_id: d.id,
        notetype_id: None,
    });
    let rows: Vec<_> = preview.cards.iter().map(|c| (c.line, c.front.as_str(), c.duplicate)).collect();
    assert_eq!(rows, vec![(2, "porsuk", true), (3, "su samuru", false), (4, "a < b & c", false)]);
    let live = |t: &T| t.all_cards().iter().filter(|c| c.deleted_at.is_none()).count();
    let notes_before = live(&t);
    let created = t.ok::<ModesInlineCreate>(CreateInlineCards {
        text: text.into(),
        deck_id: d.id,
        notetype_id: None,
        tags: vec!["satır_içi".into()],
    });
    assert_eq!((created.note_ids.len(), created.duplicates), (2, 1));
    let note = t.ok::<NotesGet>(fc_api::v1::notes::NoteRef { id: created.note_ids[1] });
    assert_eq!(note.fields["front"], "a &lt; b &amp; c");
    assert_eq!(note.tags.iter().map(|t| t.as_str()).collect::<Vec<_>>(), vec!["satır_içi"]);
    // The session did not change.
    assert_eq!(t.session(s.id), s);
    assert_eq!(live(&t), notes_before + 2);
    t.undo();
    assert_eq!(live(&t), notes_before, "one undo removes every created note");
    t.redo();
    assert_eq!(live(&t), notes_before + 2);
    let e = t.err::<ModesInlineCreate>(CreateInlineCards {
        text: "düz metin".into(),
        deck_id: d.id,
        notetype_id: None,
        tags: vec![],
    });
    assert_eq!(e.code, ErrorCode::ValidationInlineCards);
}

/// Each mode is a module: switched off, its session kind/option and methods are refused; a
/// disabled confidence module's deck setting is ignored; switched on again, all works.
pub fn modes_are_switchable_modules(f: &Factory) {
    let t = T::new(f(), 7);
    let d = t.deck("Modüller");
    for i in 1..=3 {
        t.note(d.id, &format!("q{i}"), &format!("a{i}"), None);
    }
    let disabled = |e: ApiError, module: &str| {
        assert_eq!(e.code, ErrorCode::ConflictModuleDisabled);
        assert_eq!(e.details["module"], module);
    };
    for m in ["learn", "practice_test", "confidence", "timed", "focus", "inline_cards"] {
        t.module(m, false);
    }
    disabled(t.try_start(StudyKind::Learn, StudyMode::Scheduled, d.id, None, None).unwrap_err(), "learn");
    disabled(t.try_start(StudyKind::Test, StudyMode::Scheduled, d.id, None, None).unwrap_err(), "practice_test");
    let timed = StartModes {
        timed: Some(TimedOptions { seconds: 10, on_timeout: TimeoutAction::Skip }),
        ..StartModes::default()
    };
    disabled(t.try_start(StudyKind::Normal, StudyMode::Scheduled, d.id, None, Some(timed)).unwrap_err(), "timed");
    let conf = StartModes {
        rating_scale: Some(RatingScale::Confidence),
        ..StartModes::default()
    };
    disabled(t.try_start(StudyKind::Normal, StudyMode::Scheduled, d.id, None, Some(conf.clone())).unwrap_err(), "confidence");
    t.set(SettingScope::Deck(d.id), "confidence.rating_scale", serde_json::json!("confidence"));
    let s = t.start(StudyKind::Normal, StudyMode::Scheduled, d.id, None, None);
    assert!(s.modes.is_none(), "the setting of a disabled module is ignored");
    disabled(
        t.err::<ModesInlinePreview>(InlinePreviewRequest { text: "a :: b".into(), deck_id: d.id, notetype_id: None }),
        "inline_cards",
    );
    // On again.
    for m in ["learn", "practice_test", "confidence", "timed", "inline_cards"] {
        t.module(m, true);
    }
    let s = t.start(StudyKind::Normal, StudyMode::Scheduled, d.id, None, None);
    assert_eq!(s.modes.unwrap().rating_scale, RatingScale::Confidence);
    t.start(StudyKind::Learn, StudyMode::Trial, d.id, None, None);
    t.ok::<ModesInlinePreview>(InlinePreviewRequest { text: "a :: b".into(), deck_id: d.id, notetype_id: None });
    // Focus mode is presentation: its module only carries settings.
    let list = t.ok::<ModulesList>(Empty {});
    let focus = list.modules.iter().find(|m| m.id == "focus").unwrap();
    assert!(!focus.enabled && focus.methods.is_empty());
}

macro_rules! modes_suite {
    ($factory:expr) => {
        modes_suite!(@tests $factory;
            learn_mode_learns_cards_at_every_stage,
            test_generator_mixes_types_scores_and_never_schedules,
            confidence_levels_map_onto_fsrs_ratings,
            timed_mode_is_enforced_on_the_backend_clock,
            timed_countdown_belongs_to_the_card_on_screen,
            learn_choice_answers_do_not_rate_reviewed_cards,
            modes_log_their_own_review_mode_and_respect_daily_limits,
            inline_cards_during_study_are_one_undo_step,
            modes_are_switchable_modules,
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
    modes_suite!(|| -> Arc<dyn Store> { Arc::new(MemStore::new()) });
}

mod sqlite_memory {
    use fc_storage::{SqliteStore, Store};
    use std::sync::Arc;
    modes_suite!(|| -> Arc<dyn Store> { Arc::new(SqliteStore::open_in_memory().unwrap()) });
}

mod sqlite_file {
    use crate::testing::TempSqlite;
    use fc_storage::Store;
    use std::sync::Arc;
    modes_suite!(|| -> Arc<dyn Store> { Arc::new(TempSqlite::new()) });
}
