//! v1.4 (Z1): typed answers are kept with their reviews (`typed_answers`) from the three typed
//! paths — Cards type-in (`study.answer` with `typed_answer`), Learn's written stage and the
//! Exam's written questions — bounded per card, undone with the answer, switchable
//! (`study.keep_typed_answers`), and shown as a leech's evidence. Every check reads the
//! persisted rows back (the store, `stats.leech_repairs.plan`).

use super::*;
use crate::modules::study::typed::KEEP_PER_CARD;
use fc_api::v1::stats::LeechCardRef;
use fc_storage::repo::TypedAnswerRecord;

impl T {
    fn typed(&self, card: CardId) -> Vec<TypedAnswerRecord> {
        self.store.read().unwrap().typed_answers_of_card(card).unwrap()
    }

    fn type_in(&self, s: SessionId, card: CardId, rating: Rating, typed: &str) -> SessionDto {
        self.ok::<StudyAnswer>(AnswerCard {
            session_id: s,
            card_id: card,
            rating,
            duration_ms: 1_000,
            reveal_ms: Some(500),
            typed_answer: Some(typed.to_owned()),
        })
    }

    fn plan(&self, card: CardId) -> fc_api::v1::stats::LeechRepairPlan {
        self.ok::<StatsLeechesRepairPlan>(LeechCardRef { card_id: card })
    }
}

/// Cards type-in, Learn's written stage and the Exam's written questions each keep the text
/// with their review; the leech plan shows it; undo removes it and redo brings it back.
pub fn typed_answers_are_kept_for_cards_learn_and_exam(f: &Factory) {
    let t = T::new(f(), 7);

    // Cards type-in.
    let d = t.deck("Dil");
    t.note(d.id, "dog", "köpek", None);
    let s = t.start(StudyKind::Normal, StudyMode::Scheduled, d.id, None, None);
    let c = s.current.as_ref().unwrap().card_id;
    t.type_in(s.id, c, Rating::Again, "  kopek ");
    let review = t.reviews(c).pop().unwrap();
    let rows = t.typed(c);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].typed, "kopek", "kept trimmed");
    assert_eq!(rows[0].review_log_id, Some(review.id));
    assert_eq!(rows[0].mode, "normal");
    assert_eq!(rows[0].correct, None, "rated by the user");
    let plan = t.plan(c);
    assert!(plan.typed_answers_recorded);
    assert_eq!(plan.failed_reviews.len(), 1);
    assert_eq!(plan.failed_reviews[0].typed_answer.as_deref(), Some("kopek"));
    // Undo takes the typed answer with the review; redo brings both back.
    t.undo();
    assert!(t.typed(c).is_empty());
    assert!(t.reviews(c).is_empty());
    t.redo();
    assert_eq!(t.typed(c), rows);
    t.end(s.id);

    // Learn's written stage (a lone card has no distractors: it is asked written).
    let l = t.deck("Öğren");
    let lc = t.note(l.id, "otter", "su samuru", None);
    let s = t.start(StudyKind::Learn, StudyMode::Scheduled, l.id, None, None);
    assert_eq!(learn_of(&s).stage, Some(LearnStage::Written));
    let r = t.learn(s.id, lc, None, Some("yanlış cevap"));
    assert_eq!(r.grade, AnswerGrade::Wrong);
    let rows = t.typed(lc);
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].typed, "yanlış cevap");
    assert_eq!(rows[0].mode, "learn");
    assert_eq!(rows[0].correct, Some(false));
    assert_eq!(rows[0].review_log_id, Some(t.reviews(lc).pop().unwrap().id));
    t.undo();
    assert!(t.typed(lc).is_empty());
    t.end(s.id);

    // The Exam's written questions.
    let e = t.deck("Sınav");
    t.note(e.id, "fox", "tilki", None);
    t.note(e.id, "deer", "geyik", None);
    let s = t.start(
        StudyKind::Test,
        StudyMode::Scheduled,
        e.id,
        None,
        Some(StartModes {
            test: Some(TestOptions {
                questions: Some(2),
                kinds: Some(vec![TestQuestionKind::Written]),
            }),
            ..StartModes::default()
        }),
    );
    let paper = t.ok::<ModesTestPaper>(TestPaperRequest {
        session_id: s.id,
        with_key: Some(true),
    });
    assert_eq!(paper.questions.len(), 2);
    let right = &paper.questions[0];
    let wrong = &paper.questions[1];
    t.ok::<ModesTestSubmit>(SubmitTest {
        session_id: s.id,
        answers: vec![
            TestAnswerDto {
                index: right.index,
                text: Some(right.key.clone().unwrap().expected),
                ..TestAnswerDto::default()
            },
            TestAnswerDto {
                index: wrong.index,
                text: Some("bilmiyorum".into()),
                ..TestAnswerDto::default()
            },
        ],
        duration_ms: 10_000,
    });
    let r = t.typed(right.card_id);
    let w = t.typed(wrong.card_id);
    assert_eq!((r.len(), w.len()), (1, 1));
    assert_eq!(r[0].typed, right.key.clone().unwrap().expected);
    assert_eq!((r[0].mode.as_str(), r[0].correct), ("test", Some(true)));
    assert_eq!((w[0].typed.as_str(), w[0].correct), ("bilmiyorum", Some(false)));
    assert_eq!(w[0].review_log_id, Some(t.reviews(wrong.card_id).pop().unwrap().id));
    assert_eq!(
        t.plan(wrong.card_id).failed_reviews[0].typed_answer.as_deref(),
        Some("bilmiyorum")
    );
    t.undo();
    assert!(t.typed(right.card_id).is_empty() && t.typed(wrong.card_id).is_empty());
    t.redo();
    assert_eq!(t.typed(wrong.card_id), w);
}

/// The newest 20 per card are kept (undo brings the pruned one back); with
/// `study.keep_typed_answers` off nothing new is kept, while rows kept before still show.
pub fn typed_answers_are_capped_undone_and_switchable(f: &Factory) {
    let t = T::new(f(), 8);
    let d = t.deck("Dil");
    t.note(d.id, "dog", "köpek", None);
    t.note(d.id, "cat", "kedi", None);
    let s = t.start(StudyKind::Normal, StudyMode::Scheduled, d.id, None, None);
    let c = s.current.as_ref().unwrap().card_id;
    {
        let mut tx = t.store.write().unwrap();
        for i in 0..KEEP_PER_CARD {
            tx.insert_typed_answer(&TypedAnswerRecord {
                id: 0,
                card_id: c,
                review_log_id: None,
                mode: "normal".into(),
                typed: format!("eski {i}"),
                correct: None,
                created_at: Timestamp(T0 - 100_000 + i as i64),
            })
            .unwrap();
        }
        tx.commit().unwrap();
    }
    let before = t.typed(c);
    assert_eq!(before.len(), KEEP_PER_CARD);
    let s = t.type_in(s.id, c, Rating::Again, "yeni");
    let after = t.typed(c);
    assert_eq!(after.len(), KEEP_PER_CARD, "bounded per card");
    assert_eq!(after[0].typed, "yeni", "newest first");
    assert!(!after.iter().any(|r| r.typed == "eski 0"), "the oldest is pruned");
    t.undo();
    assert_eq!(t.typed(c), before, "undo: the answer's row goes, the pruned one is back");
    t.redo();
    assert_eq!(t.typed(c), after);

    // Off: nothing new is kept; what was kept stays and still shows.
    t.set(SettingScope::App, "study.keep_typed_answers", serde_json::json!(false));
    let other = s.current.as_ref().unwrap().card_id;
    assert_ne!(other, c);
    t.type_in(s.id, other, Rating::Again, "hiç");
    assert!(t.typed(other).is_empty());
    let plan = t.plan(other);
    assert!(!plan.typed_answers_recorded);
    assert_eq!(plan.failed_reviews[0].typed_answer, None);
    let plan = t.plan(c);
    assert!(!plan.typed_answers_recorded);
    assert_eq!(plan.failed_reviews[0].typed_answer.as_deref(), Some("yeni"));
    assert_eq!(t.typed(c), after);
}
