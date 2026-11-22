//! v1.4 study modes the way the owner expects them (C1): **Test** (`quiz`: one word, four
//! meanings, the answer at once, missed words again) and **Exam** (`test`: the paper; types
//! selectable, true/false off by default, matching questions), and `study.options.modes`.
//! Each test runs on MemStore and SQLite in memory.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use crate::testing::{MemStore, TestApp};
use fc_api::v1::common::Empty;
use fc_api::v1::decks::{CreateDeck, DeckDto};
use fc_api::v1::methods::*;
use fc_api::v1::modes::*;
use fc_api::v1::modules::SetModuleEnabled;
use fc_api::v1::notes::{CreateNote, NoteDto};
use fc_api::v1::settings::SetSetting;
use fc_api::v1::study::*;
use fc_api::ErrorCode;
use fc_core::card::Card;
use fc_core::content::builtin;
use fc_core::settings::SettingScope;
use fc_core::{CardId, DeckId, SessionId};
use fc_storage::repo::CardFilter;
use fc_storage::{SqliteStore, Store};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

fn apps() -> Vec<TestApp> {
    vec![
        TestApp::new(Arc::new(MemStore::new())),
        TestApp::new(Arc::new(SqliteStore::open_in_memory().unwrap()) as Arc<dyn Store>),
    ]
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

/// A vocabulary note: word, meaning, an example sentence and a source.
fn word(t: &TestApp, deck: DeckId, w: &str, meaning: &str) -> NoteDto {
    t.ok::<NotesCreate>(CreateNote {
        notetype_id: builtin::find("vocabulary").unwrap().id,
        deck_id: deck,
        fields: BTreeMap::from([
            ("word".to_owned(), w.to_owned()),
            ("meaning".to_owned(), meaning.to_owned()),
            ("example".to_owned(), format!("EXAMPLE with {w}")),
        ]),
        tags: vec![],
        attributes: vec![],
        allow_duplicate: Some(true),
    })
}

fn start(t: &TestApp, kind: StudyKind, deck: DeckId, from: Option<SessionId>, modes: Option<StartModes>) -> Result<SessionDto, fc_api::ApiError> {
    t.app.call::<StudyStart>(
        &StartSession {
            kind,
            mode: None,
            deck_id: Some(deck),
            include_subdecks: None,
            filter: None,
            from_session: from,
            new_limit: None,
            order: Some(StudyOrder::Due),
            tz_offset_mins: Some(0),
            modes,
            scope: None,
        },
        &fc_api::v1::oplog::Actor::User,
    )
}

fn quiz_of(s: &SessionDto) -> QuizDto {
    s.modes.as_ref().unwrap().quiz.clone().unwrap()
}

fn pick(t: &TestApp, s: &SessionDto, choice: u32) -> QuizAnswerResult {
    t.ok::<ModesQuizAnswer>(QuizAnswer {
        session_id: s.id,
        card_id: s.current.as_ref().unwrap().card_id,
        choice,
        duration_ms: 900,
    })
}

fn cards(t: &TestApp) -> Vec<Card> {
    t.store.read().unwrap().list_cards(&CardFilter::default()).unwrap()
}

fn set(t: &TestApp, key: &str, value: serde_json::Value) {
    t.ok::<SettingsSet>(SetSetting {
        scope: SettingScope::App,
        key: key.into(),
        value,
    });
}

const WORDS: [(&str, &str); 6] = [
    ("go", "gitmek"),
    ("run", "koşmak"),
    ("eat", "yemek"),
    ("sleep", "uyumak"),
    ("read", "okumak"),
    ("write", "yazmak"),
];

/// Options are meanings (role `meaning`) of words in the same deck: never the example, never
/// another deck, never the right meaning twice, never another note of the same word.
#[test]
fn quiz_asks_the_term_with_meanings_of_the_same_deck() {
    for t in apps() {
        let en = deck(&t, "English");
        let other = deck(&t, "Other");
        let meaning_of: HashMap<String, String> = WORDS
            .iter()
            .map(|(w, m)| {
                word(&t, en.id, w, m);
                ((*w).to_owned(), (*m).to_owned())
            })
            .collect();
        // Another note of "go" (its other meaning must never be a wrong option for "go"),
        // a note whose meaning equals "go"'s (never a second right-looking option) and a
        // word in another deck.
        word(&t, en.id, "go", "ilerlemek");
        word(&t, en.id, "leave", "gitmek");
        word(&t, other.id, "zebra", "ZEBRA-OTHER-DECK");
        let in_deck: HashSet<&str> = WORDS
            .iter()
            .map(|(_, m)| *m)
            .chain(["ilerlemek", "gitmek"])
            .collect();
        let mut s = start(&t, StudyKind::Quiz, en.id, None, None).unwrap();
        assert_eq!(s.kind, StudyKind::Quiz);
        let q = quiz_of(&s);
        // One word per note (vocabulary notes have two cards).
        assert_eq!(q.progress.total, 8);
        assert!(!q.affects_schedule, "practice by default");
        let mut seen = 0;
        while let Some(card) = s.current.clone() {
            let q = quiz_of(&s);
            let term = q.term.clone().expect("the word is asked");
            assert_eq!(q.options.len(), 4, "{term}: {:?}", q.options);
            assert!(!q.few_options);
            let keys: HashSet<String> = q.options.iter().map(|o| o.to_lowercase()).collect();
            assert_eq!(keys.len(), 4, "distinct: {:?}", q.options);
            for o in &q.options {
                assert!(in_deck.contains(o.as_str()), "{o} is a meaning of this deck");
                assert!(!o.contains("EXAMPLE"), "never the example");
            }
            let r = pick(&t, &s, 0);
            assert!(q.options.contains(&r.expected));
            if term == "go" {
                // Both meanings of "go" are right for "go": never offered as a wrong one.
                let wrong: Vec<&String> = q.options.iter().filter(|o| **o != r.expected).collect();
                assert!(!wrong.iter().any(|o| *o == "gitmek" || *o == "ilerlemek"), "{:?}", q.options);
            } else if let Some(m) = meaning_of.get(&term) {
                assert_eq!(&r.expected, m);
                assert_eq!(q.options.iter().filter(|o| o == &m).count(), 1);
            }
            assert_ne!(s.current.as_ref().map(|c| c.card_id), r.session.current.as_ref().map(|c| c.card_id));
            let _ = card;
            s = r.session;
            seen += 1;
        }
        assert_eq!(seen, 8, "each word once");
    }
}

/// Fewer than four candidate meanings: fewer options, and the DTO says so; one meaning only
/// cannot be a quiz.
#[test]
fn quiz_degrades_on_small_decks() {
    for t in apps() {
        let one = deck(&t, "One");
        word(&t, one.id, "go", "gitmek");
        let e = start(&t, StudyKind::Quiz, one.id, None, None).unwrap_err();
        assert_eq!(e.code, ErrorCode::ValidationSessionMode);
        assert_eq!(e.details.get("reason").and_then(|v| v.as_str()), Some("too_few_words"));
        let two = deck(&t, "Two");
        word(&t, two.id, "go", "gitmek");
        word(&t, two.id, "run", "koşmak");
        let s = start(&t, StudyKind::Quiz, two.id, None, None).unwrap();
        let q = quiz_of(&s);
        assert_eq!((q.options.len(), q.few_options), (2, true));
        let three = deck(&t, "Three");
        for (w, m) in &WORDS[..3] {
            word(&t, three.id, w, m);
        }
        let s = start(&t, StudyKind::Quiz, three.id, None, None).unwrap();
        assert_eq!(quiz_of(&s).options.len(), 3);
        assert!(quiz_of(&s).few_options);
    }
}

/// The answer shows at once; the quiz is practice (cards untouched, `trial_quiz` reviews);
/// undo takes a pick back; the result reads back the score and the missed words; "study the
/// missed ones again" asks exactly those. With `quiz.affects_schedule` the answers rate.
#[test]
fn quiz_feedback_result_and_missed_restart() {
    for t in apps() {
        let en = deck(&t, "English");
        for (w, m) in &WORDS[..5] {
            word(&t, en.id, w, m);
        }
        let before = cards(&t);
        let mut s = start(&t, StudyKind::Quiz, en.id, None, None).unwrap();
        // Quiz words are not rated with buttons.
        let e = t.err::<StudyAnswer>(AnswerCard {
            session_id: s.id,
            card_id: s.current.as_ref().unwrap().card_id,
            rating: fc_core::card::Rating::Good,
            duration_ms: 1,
            reveal_ms: None,
        });
        assert_eq!(e.code, ErrorCode::ValidationSessionMode);
        // First word wrong (the verdict and the right option come back at once).
        let first = s.current.as_ref().unwrap().card_id;
        let q = quiz_of(&s);
        let r0 = pick(&t, &s, 0);
        let right = r0.correct_option;
        assert_eq!(r0.expected, q.options[right as usize]);
        let wrong = (right + 1) % 4;
        // Undo the pick and answer wrong for real.
        t.ok::<OplogUndo>(Empty {});
        s = t.ok::<StudySession>(SessionRef { session_id: s.id });
        assert_eq!(quiz_of(&s).progress.answered, 0);
        assert_eq!(s.current.as_ref().unwrap().card_id, first);
        let r = pick(&t, &s, wrong);
        assert!(!r.correct);
        assert_eq!((r.picked, r.correct_option), (wrong, right));
        assert_eq!(quiz_of(&r.session).progress.answered, 1);
        // A word is answered once.
        let e = t.err::<ModesQuizAnswer>(QuizAnswer {
            session_id: s.id,
            card_id: first,
            choice: right,
            duration_ms: 1,
        });
        assert_ne!(e.code, ErrorCode::Internal);
        s = r.session;
        // The rest right.
        while s.current.is_some() {
            let q = quiz_of(&s);
            let id = s.current.as_ref().unwrap().card_id;
            let expected = t.ok::<ModesQuizResult>(QuizRef { session_id: s.id }).total;
            assert_eq!(expected, 5);
            // Find the right option by answering through the API's verdict (no peeking): the
            // option equal to the meaning of this card's note.
            let meaning = WORDS
                .iter()
                .find(|(w, _)| Some(*w) == q.term.as_deref())
                .unwrap()
                .1;
            let i = q.options.iter().position(|o| o == meaning).unwrap() as u32;
            let r = pick(&t, &s, i);
            assert!(r.correct, "{id:?}");
            s = r.session;
        }
        // Persisted result.
        let res = t.ok::<ModesQuizResult>(QuizRef { session_id: s.id });
        assert_eq!((res.total, res.answered, res.correct, res.wrong, res.percent), (5, 5, 4, 1, 80));
        assert!(res.finished && !res.affects_schedule);
        assert_eq!(res.missed.len(), 1);
        assert_eq!(res.missed[0].card_id, first);
        assert_eq!(res.missed[0].picked.as_deref(), Some(q.options[wrong as usize].as_str()));
        // Practice: no card changed; every review is `trial_quiz`.
        assert_eq!(cards(&t), before);
        let reads = t.store.read().unwrap();
        let logs: Vec<_> = before
            .iter()
            .flat_map(|c| reads.review_logs_for_card(c.id).unwrap())
            .collect();
        drop(reads);
        assert_eq!(logs.len(), 5);
        assert!(logs.iter().all(|l| l.mode == "trial_quiz"), "{:?}", logs.iter().map(|l| &l.mode).collect::<Vec<_>>());
        t.ok::<StudyEnd>(SessionRef { session_id: s.id });
        // The result stays readable after the end; the missed words start again.
        assert_eq!(t.ok::<ModesQuizResult>(QuizRef { session_id: s.id }), res);
        let again = start(&t, StudyKind::Quiz, en.id, Some(s.id), None).unwrap();
        assert_eq!(quiz_of(&again).progress.total, 1);
        assert_eq!(again.current.as_ref().unwrap().card_id, first);
        // With nothing missed there is nothing to start.
        t.ok::<StudyEnd>(SessionRef { session_id: again.id });

        // `quiz.affects_schedule`: answers rate the card (`quiz` reviews), and the launcher
        // says so.
        set(&t, "quiz.affects_schedule", serde_json::Value::Bool(true));
        let s = start(&t, StudyKind::Quiz, en.id, None, None).unwrap();
        assert!(quiz_of(&s).affects_schedule);
        let id = s.current.as_ref().unwrap().card_id;
        let card_before: Card = t.store.read().unwrap().get_card(id).unwrap().unwrap();
        pick(&t, &s, 0);
        let card_after: Card = t.store.read().unwrap().get_card(id).unwrap().unwrap();
        assert_ne!(card_before.scheduling, card_after.scheduling);
        let last = t.store.read().unwrap().review_logs_for_card(id).unwrap();
        assert_eq!(last.last().unwrap().mode, "quiz");
        let opts = t.ok::<StudyOptionsGet>(StudyOptionsRequest {
            deck_id: Some(en.id),
            include_subdecks: None,
            tz_offset_mins: Some(0),
        });
        let quiz = opts.modes.iter().find(|m| m.kind == StudyKind::Quiz).unwrap();
        assert!(quiz.affects_schedule);
    }
}

/// `study.options.modes`: launcher order, name and description keys, the schedule flag, the
/// exam's question types (true/false offered, not selected) — and module switches.
#[test]
fn study_options_describe_every_mode() {
    for t in apps() {
        let en = deck(&t, "English");
        for (w, m) in WORDS {
            word(&t, en.id, w, m);
        }
        let get = || {
            t.ok::<StudyOptionsGet>(StudyOptionsRequest {
                deck_id: Some(en.id),
                include_subdecks: None,
                tz_offset_mins: Some(0),
            })
        };
        let o = get();
        let kinds: Vec<StudyKind> = o.modes.iter().map(|m| m.kind).collect();
        assert_eq!(
            kinds,
            [StudyKind::Normal, StudyKind::Quiz, StudyKind::Learn, StudyKind::Test, StudyKind::Match]
        );
        let flags: Vec<bool> = o.modes.iter().map(|m| m.affects_schedule).collect();
        assert_eq!(flags, [true, false, true, false, false]);
        let quiz = &o.modes[1];
        assert_eq!(quiz.name_key, "study.mode.quiz.name");
        assert_eq!(quiz.description_key, "study.mode.quiz.description");
        assert!(quiz.question_kinds.is_empty() && quiz.default_questions.is_none());
        let exam = &o.modes[3];
        assert_eq!(exam.name_key, "study.mode.test.name");
        assert_eq!(
            exam.question_kinds,
            [
                TestQuestionKind::MultipleChoice,
                TestQuestionKind::Written,
                TestQuestionKind::Matching,
                TestQuestionKind::TrueFalse
            ]
        );
        assert_eq!(
            exam.default_question_kinds,
            [TestQuestionKind::MultipleChoice, TestQuestionKind::Written, TestQuestionKind::Matching]
        );
        assert_eq!(exam.default_questions, Some(20));
        // Switching the quiz module off takes it off the list and refuses a start.
        t.ok::<ModulesSetEnabled>(SetModuleEnabled {
            id: "quiz".into(),
            enabled: false,
        });
        assert!(!get().modes.iter().any(|m| m.kind == StudyKind::Quiz));
        let e = start(&t, StudyKind::Quiz, en.id, None, None).unwrap_err();
        assert_eq!(e.code, ErrorCode::ConflictModuleDisabled);
    }
}

/// Exam (`test`): the default paper leaves true/false out and has a matching question;
/// a matching answer is graded all-or-nothing with the words matched reported; the types
/// asked for are the types given.
#[test]
fn exam_defaults_and_matching_questions() {
    for t in apps() {
        let en = deck(&t, "English");
        for (w, m) in WORDS {
            word(&t, en.id, w, m);
        }
        let s = start(
            &t,
            StudyKind::Test,
            en.id,
            None,
            Some(StartModes {
                timed: None,
                rating_scale: None,
                test: Some(TestOptions {
                    questions: Some(6),
                    kinds: None,
                }),
            }),
        )
        .unwrap();
        let paper = t.ok::<ModesTestPaper>(TestPaperRequest {
            session_id: s.id,
            with_key: Some(true),
        });
        let kinds: HashSet<TestQuestionKind> = paper.questions.iter().map(|q| q.kind).collect();
        assert!(!kinds.contains(&TestQuestionKind::TrueFalse), "{kinds:?}");
        assert!(kinds.contains(&TestQuestionKind::Matching), "{kinds:?}");
        let m = paper
            .questions
            .iter()
            .find(|q| q.kind == TestQuestionKind::Matching)
            .unwrap();
        assert_eq!(m.terms.len(), 4);
        assert_eq!(m.options.len(), 4);
        let key = m.key.as_ref().unwrap().pairs.clone().unwrap();
        for (term, p) in m.terms.iter().zip(&key) {
            let meaning = WORDS.iter().find(|(w, _)| w == term).unwrap().1;
            assert_eq!(m.options[*p as usize], meaning, "pairs by role: word → meaning");
        }
        // Answer the matching question with two words swapped; leave the rest blank.
        let mut given: Vec<Option<u32>> = key.iter().copied().map(Some).collect();
        given.swap(0, 1);
        let graded = t.ok::<ModesTestSubmit>(SubmitTest {
            session_id: s.id,
            answers: vec![TestAnswerDto {
                index: m.index,
                pairs: Some(given.clone()),
                ..TestAnswerDto::default()
            }],
            duration_ms: 10_000,
        });
        let result = graded.result.unwrap();
        let item = &result.items[m.index as usize];
        assert_eq!((item.verdict, item.matched), (TestVerdict::Wrong, Some(2)));
        assert_eq!(graded.answers.unwrap()[m.index as usize].pairs, Some(given));
        // Asked-for types are the types given (true/false still available).
        let s = start(
            &t,
            StudyKind::Test,
            en.id,
            None,
            Some(StartModes {
                timed: None,
                rating_scale: None,
                test: Some(TestOptions {
                    questions: Some(4),
                    kinds: Some(vec![TestQuestionKind::TrueFalse]),
                }),
            }),
        )
        .unwrap();
        let paper = t.ok::<ModesTestPaper>(TestPaperRequest {
            session_id: s.id,
            with_key: None,
        });
        assert!(paper.questions.iter().all(|q| q.kind == TestQuestionKind::TrueFalse));
        let _: Vec<CardId> = paper.questions.iter().map(|q| q.card_id).collect();
    }
}
