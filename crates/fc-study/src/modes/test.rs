//! Test generator (docs/02 §6.1, Quizlet style): a paper of mixed question types from a set
//! of cards — multiple choice, written, true/false — and its scoring. The paper is fixed when
//! the test starts (questions, options and answer key); answers are graded on submission.
//!
//! Distractors (wrong options, false statements) are other cards' answers from the same set.
//! A question that needs a distractor falls back to a written question when there is none.

use super::{choice_options, shuffle};
use crate::answer::Grade;
use fc_core::card::Rating;
use fc_core::rng::Rng;
use fc_core::CardId;
use serde::{Deserialize, Serialize};

/// Most questions on a paper.
pub const MAX_QUESTIONS: usize = 200;
/// Options of a multiple-choice question.
pub const CHOICE_OPTIONS: usize = 4;
/// Points of an "almost" written answer (typo-level difference).
pub const ALMOST_POINTS: f64 = 0.5;

/// Question type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuestionKind {
    /// Pick the answer among options.
    MultipleChoice,
    /// Type the answer.
    Written,
    /// Is the shown answer right?
    TrueFalse,
}

/// All question types, in paper order.
pub const ALL_KINDS: [QuestionKind; 3] = [
    QuestionKind::TrueFalse,
    QuestionKind::MultipleChoice,
    QuestionKind::Written,
];

/// What a question is made from: one card.
#[derive(Debug, Clone, PartialEq)]
pub struct TestSource {
    /// Card.
    pub card_id: CardId,
    /// Question side (sanitized HTML).
    pub prompt_html: String,
    /// Expected answer (plain text).
    pub answer: String,
    /// The card's own options and the correct index (choice card types).
    pub own_options: Option<(Vec<String>, u32)>,
}

/// Answer key of a question.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnswerKey {
    /// Multiple choice: the correct option.
    pub option: Option<u32>,
    /// True/false: whether the statement is right.
    pub truth: Option<bool>,
    /// The correct answer as text.
    pub expected: String,
}

/// One question of the paper.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Question {
    /// Card it asks.
    pub card_id: CardId,
    /// Type.
    pub kind: QuestionKind,
    /// Question side (sanitized HTML).
    pub prompt_html: String,
    /// Multiple choice: options (plain text).
    pub options: Vec<String>,
    /// True/false: the proposed answer.
    pub statement: Option<String>,
    /// Answer key.
    pub key: AnswerKey,
}

/// A learner's answer to one question.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Response {
    /// Picked option.
    Choice(u32),
    /// Typed text.
    Text(String),
    /// True or false.
    Truth(bool),
    /// Left blank.
    Blank,
}

/// Verdict on one answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// Right.
    Correct,
    /// Written, typo-level difference ([`ALMOST_POINTS`]).
    Almost,
    /// Wrong.
    Wrong,
    /// Not answered.
    Blank,
}

impl Verdict {
    /// Points (out of 1).
    pub const fn points(self) -> f64 {
        match self {
            Self::Correct => 1.0,
            Self::Almost => ALMOST_POINTS,
            Self::Wrong | Self::Blank => 0.0,
        }
    }

    /// Rating logged for the card (`review_log.mode = test`; the schedule never changes).
    pub const fn rating(self) -> Rating {
        match self {
            Self::Correct => Rating::Good,
            Self::Almost => Rating::Hard,
            Self::Wrong | Self::Blank => Rating::Again,
        }
    }
}

/// Graded paper.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Graded {
    /// Per question, in paper order.
    pub verdicts: Vec<Verdict>,
    /// The answers given, in paper order.
    pub responses: Vec<Response>,
    /// Points scored.
    pub points: f64,
    /// Score 0–100 (rounded).
    pub percent: u32,
}

/// Test state of a session.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TestState {
    /// The paper.
    pub questions: Vec<Question>,
    /// Result once submitted.
    #[serde(default)]
    pub graded: Option<Graded>,
}

/// Builds a paper of `count` questions (at most [`MAX_QUESTIONS`], at most one per source)
/// from `sources`, cycling through `kinds` so types are mixed; grouped by type in the order of
/// `kinds` (the way printed tests are laid out). Sources are drawn at random.
pub fn generate(
    sources: &[TestSource],
    kinds: &[QuestionKind],
    count: usize,
    rng: &mut dyn Rng,
) -> Vec<Question> {
    let kinds: Vec<QuestionKind> = if kinds.is_empty() {
        ALL_KINDS.to_vec()
    } else {
        let mut k = Vec::new();
        for kind in kinds {
            if !k.contains(kind) {
                k.push(*kind);
            }
        }
        k
    };
    let pool: Vec<String> = sources.iter().map(|s| s.answer.clone()).collect();
    let mut order: Vec<&TestSource> = sources.iter().collect();
    shuffle(&mut order, rng);
    order.truncate(count.min(MAX_QUESTIONS));
    let mut questions: Vec<Question> = order
        .iter()
        .enumerate()
        .map(|(i, s)| question(s, kinds[i % kinds.len()], &pool, rng))
        .collect();
    let rank = |k: QuestionKind| kinds.iter().position(|x| *x == k).unwrap_or(usize::MAX);
    questions.sort_by_key(|q| rank(q.kind));
    questions
}

fn written(s: &TestSource) -> Question {
    Question {
        card_id: s.card_id,
        kind: QuestionKind::Written,
        prompt_html: s.prompt_html.clone(),
        options: Vec::new(),
        statement: None,
        key: AnswerKey {
            option: None,
            truth: None,
            expected: s.answer.clone(),
        },
    }
}

fn question(s: &TestSource, kind: QuestionKind, pool: &[String], rng: &mut dyn Rng) -> Question {
    match kind {
        QuestionKind::Written => written(s),
        QuestionKind::MultipleChoice => {
            let options = match &s.own_options {
                Some((opts, i)) if opts.len() >= 2 => Some((opts.clone(), *i)),
                _ => choice_options(&s.answer, pool, CHOICE_OPTIONS, rng),
            };
            match options {
                Some((options, i)) => Question {
                    card_id: s.card_id,
                    kind,
                    prompt_html: s.prompt_html.clone(),
                    key: AnswerKey {
                        option: Some(i),
                        truth: None,
                        expected: options.get(i as usize).cloned().unwrap_or_default(),
                    },
                    options,
                    statement: None,
                },
                None => written(s),
            }
        }
        QuestionKind::TrueFalse => {
            // A wrong statement needs a distractor; without one the question is written.
            let Some((distractors, _)) = choice_options(&s.answer, pool, 2, rng) else {
                return written(s);
            };
            let truth = rng.below(2) == 0;
            let statement = if truth {
                s.answer.clone()
            } else {
                distractors
                    .into_iter()
                    .find(|d| super::option_key(d) != super::option_key(&s.answer))
                    .unwrap_or_default()
            };
            Question {
                card_id: s.card_id,
                kind,
                prompt_html: s.prompt_html.clone(),
                options: Vec::new(),
                statement: Some(statement),
                key: AnswerKey {
                    option: None,
                    truth: Some(truth),
                    expected: s.answer.clone(),
                },
            }
        }
    }
}

/// Grades `responses` (paper order; missing = blank). Written answers go to `check_text`
/// (expected, given) — the configured answer checker.
pub fn grade(
    questions: &[Question],
    responses: &[Response],
    check_text: impl Fn(&str, &str) -> Grade,
) -> Graded {
    let mut verdicts = Vec::with_capacity(questions.len());
    let mut given = Vec::with_capacity(questions.len());
    for (i, q) in questions.iter().enumerate() {
        let r = responses.get(i).cloned().unwrap_or(Response::Blank);
        let v = match (&r, q.kind) {
            (Response::Blank, _) => Verdict::Blank,
            (Response::Text(t), _) if t.trim().is_empty() => Verdict::Blank,
            (Response::Choice(c), QuestionKind::MultipleChoice) => {
                if q.key.option == Some(*c) {
                    Verdict::Correct
                } else {
                    Verdict::Wrong
                }
            }
            (Response::Truth(t), QuestionKind::TrueFalse) => {
                if q.key.truth == Some(*t) {
                    Verdict::Correct
                } else {
                    Verdict::Wrong
                }
            }
            (Response::Text(t), QuestionKind::Written) => match check_text(&q.key.expected, t) {
                Grade::Correct => Verdict::Correct,
                Grade::Almost => Verdict::Almost,
                Grade::Wrong => Verdict::Wrong,
            },
            // An answer of the wrong shape is wrong.
            _ => Verdict::Wrong,
        };
        verdicts.push(v);
        given.push(r);
    }
    let points: f64 = verdicts.iter().map(|v| v.points()).sum();
    let percent = if questions.is_empty() {
        0
    } else {
        (points * 100.0 / questions.len() as f64).round() as u32
    };
    Graded {
        verdicts,
        responses: given,
        points,
        percent,
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use crate::answer::{check, CheckOptions, CheckerKind};
    use fc_core::rng::SplitMix64;
    use uuid::Uuid;

    fn sources(n: u128) -> Vec<TestSource> {
        (1..=n)
            .map(|i| TestSource {
                card_id: CardId(Uuid::from_u128(i)),
                prompt_html: format!("<p>q{i}</p>"),
                answer: format!("cevap {i}"),
                own_options: None,
            })
            .collect()
    }

    fn checker(e: &str, g: &str) -> Grade {
        check(CheckerKind::Multi, e, g, &CheckOptions::default()).grade
    }

    #[test]
    fn paper_mixes_types_groups_them_and_keys_are_consistent() {
        let src = sources(12);
        let mut rng = SplitMix64::new(3);
        let paper = generate(&src, &[], 9, &mut rng);
        assert_eq!(paper.len(), 9);
        let count = |k| paper.iter().filter(|q| q.kind == k).count();
        assert_eq!(
            (
                count(QuestionKind::TrueFalse),
                count(QuestionKind::MultipleChoice),
                count(QuestionKind::Written)
            ),
            (3, 3, 3)
        );
        // Grouped in ALL_KINDS order; one question per card.
        let kinds: Vec<_> = paper.iter().map(|q| q.kind).collect();
        let mut sorted = kinds.clone();
        sorted.sort_by_key(|k| ALL_KINDS.iter().position(|x| x == k));
        assert_eq!(kinds, sorted);
        let cards: std::collections::HashSet<_> = paper.iter().map(|q| q.card_id).collect();
        assert_eq!(cards.len(), 9);
        for q in &paper {
            let src = src.iter().find(|s| s.card_id == q.card_id).unwrap();
            assert_eq!(q.key.expected, src.answer);
            match q.kind {
                QuestionKind::MultipleChoice => {
                    assert_eq!(q.options.len(), CHOICE_OPTIONS);
                    assert_eq!(q.options[q.key.option.unwrap() as usize], src.answer);
                }
                QuestionKind::TrueFalse => {
                    let truth = q.key.truth.unwrap();
                    assert_eq!(q.statement.as_deref() == Some(src.answer.as_str()), truth);
                }
                QuestionKind::Written => assert!(q.options.is_empty()),
            }
        }
        // Deterministic per seed.
        assert_eq!(generate(&src, &[], 9, &mut SplitMix64::new(3)), paper);
    }

    #[test]
    fn without_distractors_questions_fall_back_to_written() {
        let src = sources(1);
        let paper = generate(
            &src,
            &[QuestionKind::MultipleChoice, QuestionKind::TrueFalse],
            5,
            &mut SplitMix64::new(1),
        );
        assert_eq!(paper.len(), 1);
        assert_eq!(paper[0].kind, QuestionKind::Written);
        // A choice card brings its own options.
        let mut own = sources(1);
        own[0].own_options = Some((vec!["a".into(), "cevap 1".into()], 1));
        let paper = generate(
            &own,
            &[QuestionKind::MultipleChoice],
            1,
            &mut SplitMix64::new(1),
        );
        assert_eq!(paper[0].kind, QuestionKind::MultipleChoice);
        assert_eq!(paper[0].key.option, Some(1));
    }

    #[test]
    fn scoring() {
        let src = sources(6);
        let paper = generate(&src, &[], 6, &mut SplitMix64::new(11));
        // All right.
        let right: Vec<Response> = paper
            .iter()
            .map(|q| match q.kind {
                QuestionKind::MultipleChoice => Response::Choice(q.key.option.unwrap()),
                QuestionKind::TrueFalse => Response::Truth(q.key.truth.unwrap()),
                QuestionKind::Written => Response::Text(q.key.expected.clone()),
            })
            .collect();
        let g = grade(&paper, &right, checker);
        assert_eq!((g.percent, g.points), (100, 6.0));
        // Everything wrong or blank: a typo-level written answer earns half a point.
        let mixed: Vec<Response> = paper
            .iter()
            .enumerate()
            .map(|(i, q)| match (q.kind, i % 2) {
                (QuestionKind::MultipleChoice, _) => Response::Choice(q.key.option.unwrap() + 1),
                (QuestionKind::TrueFalse, _) => Response::Truth(!q.key.truth.unwrap()),
                (QuestionKind::Written, 0) => Response::Text("cevap 9x".into()),
                (QuestionKind::Written, _) => Response::Text(format!("{}x", q.key.expected)),
            })
            .collect();
        let g = grade(&paper, &mixed[..mixed.len() - 1], checker);
        assert_eq!(g.verdicts.len(), 6);
        assert_eq!(g.verdicts[5], Verdict::Blank);
        assert!(g.verdicts.iter().all(|v| *v != Verdict::Correct));
        assert_eq!(
            g.points,
            g.verdicts.iter().filter(|v| **v == Verdict::Almost).count() as f64 * ALMOST_POINTS
        );
        // Wrong-shaped answer is wrong; empty text is blank.
        let q = &paper[0];
        let bad = match q.kind {
            QuestionKind::Written => Response::Truth(true),
            _ => Response::Text("x".into()),
        };
        assert_eq!(
            grade(&paper[..1], &[bad], checker).verdicts[0],
            Verdict::Wrong
        );
        assert_eq!(
            grade(&paper[..1], &[Response::Text("  ".into())], checker).verdicts[0],
            Verdict::Blank
        );
        assert_eq!(Verdict::Almost.rating(), Rating::Hard);
        assert_eq!(grade(&[], &[], checker).percent, 0);
    }
}
