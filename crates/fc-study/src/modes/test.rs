//! Test generator (docs/02 §6.1, Quizlet style; **"Exam"** in the app since v1.4): a paper of
//! mixed question types from a set of cards — multiple choice, written, matching (v1.4),
//! true/false — and its scoring. The paper is fixed when
//! the test starts (questions, options and answer key); answers are graded on submission.
//!
//! Distractors (wrong options, false statements) are other cards' answers from the same set.
//! A question that needs a distractor falls back to a written question when there is none.
//! A **matching** question (v1.4) pairs up to four words with their meanings: the asked card
//! and up to three other cards with distinct words and meanings; it is right only when every
//! word is matched right. True/false is off by default ([`DEFAULT_KINDS`]): guessing gets it
//! right half the time.

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
    /// v1.4: match up to [`MATCH_PAIRS`] words with their meanings.
    Matching,
}

/// Every question type, in the order the exam offers them.
pub const ALL_KINDS: [QuestionKind; 4] = [
    QuestionKind::MultipleChoice,
    QuestionKind::Written,
    QuestionKind::Matching,
    QuestionKind::TrueFalse,
];

/// Types of a paper when none are asked for (v1.4: true/false only on request).
pub const DEFAULT_KINDS: [QuestionKind; 3] = [
    QuestionKind::MultipleChoice,
    QuestionKind::Written,
    QuestionKind::Matching,
];

/// Most words of a matching question.
pub const MATCH_PAIRS: usize = 4;

/// What a question is made from: one card.
#[derive(Debug, Clone, PartialEq)]
pub struct TestSource {
    /// Card.
    pub card_id: CardId,
    /// Question side (sanitized HTML).
    pub prompt_html: String,
    /// The word asked, as plain text (matching questions list it).
    pub term: String,
    /// What the word means, whatever the card's direction (matching questions pair `term`
    /// with it; v1.3 roles: the `meaning` field).
    pub definition: String,
    /// Expected answer (plain text).
    pub answer: String,
    /// The card's own options and the correct index (choice card types).
    pub own_options: Option<(Vec<String>, u32)>,
    /// v1.4 accepted answers of `answer` and of `definition` (the offline written check takes
    /// them): never offered as a wrong option or a false statement, never another word's
    /// meaning on the same matching board.
    pub accepted: Vec<String>,
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
    /// Matching: for each word the index of its meaning in the options.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pairs: Option<Vec<u32>>,
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
    /// Matching: the words (options are their meanings).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub terms: Vec<String>,
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
    /// Matching: the option picked for each word (`None` = left blank).
    Pairs(Vec<Option<u32>>),
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
    /// v1.3: answers saved while the paper is open (paper order; empty = none saved), so an
    /// unfinished test can be continued.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub draft: Vec<Response>,
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
        DEFAULT_KINDS.to_vec()
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
        .map(|(i, s)| question(s, kinds[i % kinds.len()], &pool, sources, rng))
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
        terms: Vec::new(),
        key: AnswerKey {
            option: None,
            truth: None,
            expected: s.answer.clone(),
            pairs: None,
        },
    }
}

/// A matching question on `s` and up to three other sources with distinct words and
/// meanings; `None` when no other source qualifies.
fn matching(s: &TestSource, sources: &[TestSource], rng: &mut dyn Rng) -> Option<Question> {
    use super::option_key;
    let mut words = vec![option_key(&s.term)];
    let mut meanings = vec![option_key(&s.definition)];
    // Accepted answers of the words on the board: no other word may bring one as its meaning.
    let mut accepted: Vec<String> = s.accepted.iter().map(|a| option_key(a)).collect();
    if words[0].is_empty() || meanings[0].is_empty() {
        return None;
    }
    let mut others: Vec<&TestSource> = sources.iter().filter(|o| o.card_id != s.card_id).collect();
    shuffle(&mut others, rng);
    let mut set = vec![s];
    for o in others {
        if set.len() == MATCH_PAIRS {
            break;
        }
        let (w, m) = (option_key(&o.term), option_key(&o.definition));
        if w.is_empty() || m.is_empty() || words.contains(&w) || meanings.contains(&m) {
            continue;
        }
        // Neither its meaning is accepted for a word on the board, nor a board meaning for it.
        let theirs: Vec<String> = o.accepted.iter().map(|a| option_key(a)).collect();
        if accepted.contains(&m) || meanings.iter().any(|x| theirs.contains(x)) {
            continue;
        }
        words.push(w);
        meanings.push(m);
        accepted.extend(theirs);
        set.push(o);
    }
    if set.len() < 2 {
        return None;
    }
    let mut options: Vec<String> = set.iter().map(|x| x.definition.clone()).collect();
    shuffle(&mut options, rng);
    let pairs: Vec<u32> = set
        .iter()
        .map(|x| {
            options
                .iter()
                .position(|o| *o == x.definition)
                .and_then(|i| u32::try_from(i).ok())
                .unwrap_or(0)
        })
        .collect();
    Some(Question {
        card_id: s.card_id,
        kind: QuestionKind::Matching,
        prompt_html: s.prompt_html.clone(),
        terms: set.iter().map(|x| x.term.clone()).collect(),
        options,
        statement: None,
        key: AnswerKey {
            option: None,
            truth: None,
            expected: s.definition.clone(),
            pairs: Some(pairs),
        },
    })
}

/// Words matched right in a matching answer (`None` for other questions).
pub fn matched(q: &Question, r: &Response) -> Option<u32> {
    let key = q.key.pairs.as_ref()?;
    let n = match r {
        Response::Pairs(given) => key
            .iter()
            .zip(given)
            .filter(|(k, g)| **g == Some(**k))
            .count(),
        _ => 0,
    };
    u32::try_from(n).ok()
}

fn question(
    s: &TestSource,
    kind: QuestionKind,
    pool: &[String],
    sources: &[TestSource],
    rng: &mut dyn Rng,
) -> Question {
    match kind {
        QuestionKind::Written => written(s),
        QuestionKind::Matching => matching(s, sources, rng).unwrap_or_else(|| written(s)),
        QuestionKind::MultipleChoice => {
            let options = match &s.own_options {
                Some((opts, i)) if opts.len() >= 2 => Some((opts.clone(), *i)),
                _ => choice_options(
                    &s.answer,
                    &super::without_accepted(pool, &s.accepted),
                    CHOICE_OPTIONS,
                    rng,
                ),
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
                        pairs: None,
                    },
                    options,
                    statement: None,
                    terms: Vec::new(),
                },
                None => written(s),
            }
        }
        QuestionKind::TrueFalse => {
            // A wrong statement needs a distractor; without one the question is written.
            let pool = super::without_accepted(pool, &s.accepted);
            let Some((distractors, _)) = choice_options(&s.answer, &pool, 2, rng) else {
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
                terms: Vec::new(),
                key: AnswerKey {
                    option: None,
                    truth: Some(truth),
                    expected: s.answer.clone(),
                    pairs: None,
                },
            }
        }
    }
}

/// Grades `responses` (paper order; missing = blank). Written answers go to `check_text`
/// (question, given) — the configured answer checker, with the card's accepted answers.
pub fn grade(
    questions: &[Question],
    responses: &[Response],
    check_text: impl Fn(&Question, &str) -> Grade,
) -> Graded {
    let mut verdicts = Vec::with_capacity(questions.len());
    let mut given = Vec::with_capacity(questions.len());
    for (i, q) in questions.iter().enumerate() {
        let r = responses.get(i).cloned().unwrap_or(Response::Blank);
        let v = match (&r, q.kind) {
            (Response::Blank, _) => Verdict::Blank,
            (Response::Text(t), _) if t.trim().is_empty() => Verdict::Blank,
            (Response::Pairs(p), _) if p.iter().all(Option::is_none) => Verdict::Blank,
            (Response::Pairs(p), QuestionKind::Matching) => {
                let all = q.key.pairs.as_ref().is_some_and(|k| {
                    k.len() == p.len() && k.iter().zip(p).all(|(k, g)| *g == Some(*k))
                });
                if all {
                    Verdict::Correct
                } else {
                    Verdict::Wrong
                }
            }
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
            (Response::Text(t), QuestionKind::Written) => match check_text(q, t) {
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
                term: format!("q{i}"),
                definition: format!("cevap {i}"),
                answer: format!("cevap {i}"),
                own_options: None,
                accepted: Vec::new(),
            })
            .collect()
    }

    fn checker(q: &Question, g: &str) -> Grade {
        check(
            CheckerKind::Multi,
            &q.key.expected,
            g,
            &CheckOptions::default(),
        )
        .grade
    }

    /// v1.4: an accepted answer of the right one is never a wrong option, a false statement
    /// or another word's meaning on a matching board.
    #[test]
    fn accepted_answers_are_never_distractors() {
        // Card 1 means "vazgeçmek" and also accepts "bırakmak"; card 2 means "bırakmak".
        let mut src = sources(6);
        src[0].answer = "vazgeçmek".into();
        src[0].definition = "vazgeçmek".into();
        src[0].accepted = vec!["bırakmak".into()];
        src[1].answer = "bırakmak".into();
        src[1].definition = "bırakmak".into();
        let bad = |t: &str| super::super::option_key(t) == "bırakmak";
        for seed in 0..40 {
            let paper = generate(&src, &ALL_KINDS, 6, &mut SplitMix64::new(seed));
            for q in paper.iter().filter(|q| q.card_id == src[0].card_id) {
                match q.kind {
                    QuestionKind::MultipleChoice => {
                        assert!(!q.options.iter().any(|o| bad(o)), "{q:?}")
                    }
                    QuestionKind::TrueFalse => {
                        assert!(!q.statement.as_deref().is_some_and(bad), "{q:?}")
                    }
                    _ => {}
                }
            }
            for q in paper.iter().filter(|q| q.kind == QuestionKind::Matching) {
                let has = |t: &str| q.terms.iter().any(|x| x == t);
                assert!(
                    !(has("q1") && has("q2")),
                    "vazgeçmek and bırakmak on one board: {q:?}"
                );
            }
        }
    }

    #[test]
    fn paper_mixes_types_groups_them_and_keys_are_consistent() {
        // v1.4: every type asked for explicitly (the default leaves true/false out).
        let src = sources(12);
        let mut rng = SplitMix64::new(3);
        let paper = generate(&src, &ALL_KINDS, 12, &mut rng);
        assert_eq!(paper.len(), 12);
        let count = |k| paper.iter().filter(|q| q.kind == k).count();
        assert_eq!(
            (
                count(QuestionKind::TrueFalse),
                count(QuestionKind::MultipleChoice),
                count(QuestionKind::Written),
                count(QuestionKind::Matching)
            ),
            (3, 3, 3, 3)
        );
        // Grouped in ALL_KINDS order; one question per card.
        let kinds: Vec<_> = paper.iter().map(|q| q.kind).collect();
        let mut sorted = kinds.clone();
        sorted.sort_by_key(|k| ALL_KINDS.iter().position(|x| x == k));
        assert_eq!(kinds, sorted);
        let cards: std::collections::HashSet<_> = paper.iter().map(|q| q.card_id).collect();
        assert_eq!(cards.len(), 12);
        for q in &paper {
            let src_of = |id| src.iter().find(|s| s.card_id == id).unwrap();
            let s = src_of(q.card_id);
            assert_eq!(q.key.expected, s.answer);
            match q.kind {
                QuestionKind::MultipleChoice => {
                    assert_eq!(q.options.len(), CHOICE_OPTIONS);
                    assert_eq!(q.options[q.key.option.unwrap() as usize], s.answer);
                }
                QuestionKind::TrueFalse => {
                    let truth = q.key.truth.unwrap();
                    assert_eq!(q.statement.as_deref() == Some(s.answer.as_str()), truth);
                }
                QuestionKind::Written => assert!(q.options.is_empty()),
                QuestionKind::Matching => {
                    // Four words, the asked one first; the key pairs each with its meaning.
                    assert_eq!(q.terms.len(), MATCH_PAIRS);
                    assert_eq!(q.terms[0], s.term);
                    assert_eq!(q.options.len(), MATCH_PAIRS);
                    let pairs = q.key.pairs.as_ref().unwrap();
                    for (t, p) in q.terms.iter().zip(pairs) {
                        let owner = src.iter().find(|x| &x.term == t).unwrap();
                        assert_eq!(q.options[*p as usize], owner.answer);
                    }
                    let distinct: std::collections::HashSet<_> = q.options.iter().collect();
                    assert_eq!(distinct.len(), MATCH_PAIRS);
                }
            }
        }
        // Deterministic per seed.
        assert_eq!(
            generate(&src, &ALL_KINDS, 12, &mut SplitMix64::new(3)),
            paper
        );
    }

    /// A paper saved by v1.3 (no `terms`, no `pairs`; kinds `true_false`…) keeps loading and
    /// grading: the stored kind id (`test`) and shape did not change, so no migration.
    #[test]
    fn v1_3_saved_papers_still_load_and_grade() {
        let old = serde_json::json!({
            "questions": [
                { "card_id": "00000000-0000-0000-0000-000000000001", "kind": "true_false",
                  "prompt_html": "<p>q</p>", "options": [], "statement": "a",
                  "key": { "option": null, "truth": true, "expected": "a" } },
                { "card_id": "00000000-0000-0000-0000-000000000002", "kind": "multiple_choice",
                  "prompt_html": "<p>r</p>", "options": ["x", "b"], "statement": null,
                  "key": { "option": 1, "truth": null, "expected": "b" } }
            ],
            "draft": [{ "truth": true }, "blank"]
        });
        let state: TestState = serde_json::from_value(old).unwrap();
        assert_eq!(state.questions.len(), 2);
        assert!(state
            .questions
            .iter()
            .all(|q| q.terms.is_empty() && q.key.pairs.is_none()));
        assert_eq!(state.draft, vec![Response::Truth(true), Response::Blank]);
        let g = grade(
            &state.questions,
            &[Response::Truth(true), Response::Choice(1)],
            checker,
        );
        assert_eq!(g.percent, 100);
        // And it is written back without the new fields.
        let back = serde_json::to_value(&state.questions[0]).unwrap();
        assert!(back.get("terms").is_none() && back["key"].get("pairs").is_none());
    }

    #[test]
    fn default_types_leave_true_false_out() {
        let src = sources(9);
        let paper = generate(&src, &[], 9, &mut SplitMix64::new(5));
        let count = |k| paper.iter().filter(|q| q.kind == k).count();
        assert_eq!(count(QuestionKind::TrueFalse), 0);
        assert_eq!(
            (
                count(QuestionKind::MultipleChoice),
                count(QuestionKind::Written),
                count(QuestionKind::Matching)
            ),
            (3, 3, 3)
        );
        assert_eq!(DEFAULT_KINDS.len() + 1, ALL_KINDS.len());
    }

    #[test]
    fn matching_needs_two_distinct_words_and_scores_all_or_nothing() {
        // One card: no partner, written instead.
        let one = generate(
            &sources(1),
            &[QuestionKind::Matching],
            1,
            &mut SplitMix64::new(1),
        );
        assert_eq!(one[0].kind, QuestionKind::Written);
        // Three cards, two sharing a meaning: the pair is never on one board.
        let mut src = sources(3);
        src[2].answer = src[1].answer.clone();
        src[2].definition = src[1].definition.clone();
        for seed in 0..20 {
            let paper = generate(
                &src,
                &[QuestionKind::Matching],
                3,
                &mut SplitMix64::new(seed),
            );
            for q in &paper {
                assert_eq!(q.kind, QuestionKind::Matching);
                assert_eq!(q.terms.len(), 2, "{q:?}");
            }
        }
        let paper = generate(
            &sources(4),
            &[QuestionKind::Matching],
            1,
            &mut SplitMix64::new(2),
        );
        let q = &paper[0];
        let key: Vec<Option<u32>> = q.key.pairs.clone().unwrap().into_iter().map(Some).collect();
        let g = grade(&paper, &[Response::Pairs(key.clone())], checker);
        assert_eq!(g.verdicts[0], Verdict::Correct);
        assert_eq!(matched(q, &Response::Pairs(key.clone())), Some(4));
        // One word wrong: the question is wrong, three matched.
        let mut one_off = key.clone();
        one_off.swap(0, 1);
        let r = Response::Pairs(one_off);
        assert_eq!(
            grade(&paper, std::slice::from_ref(&r), checker).verdicts[0],
            Verdict::Wrong
        );
        assert_eq!(matched(q, &r), Some(2));
        // Nothing picked: blank.
        let blank = Response::Pairs(vec![None; 4]);
        assert_eq!(grade(&paper, &[blank], checker).verdicts[0], Verdict::Blank);
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
        let paper = generate(
            &src,
            &[
                QuestionKind::TrueFalse,
                QuestionKind::MultipleChoice,
                QuestionKind::Written,
            ],
            6,
            &mut SplitMix64::new(11),
        );
        // All right.
        let right: Vec<Response> = paper
            .iter()
            .map(|q| match q.kind {
                QuestionKind::MultipleChoice => Response::Choice(q.key.option.unwrap()),
                QuestionKind::TrueFalse => Response::Truth(q.key.truth.unwrap()),
                QuestionKind::Written => Response::Text(q.key.expected.clone()),
                QuestionKind::Matching => unreachable!(),
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
                (QuestionKind::Matching, _) => unreachable!(),
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
