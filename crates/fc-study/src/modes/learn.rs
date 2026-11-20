//! Quizlet-style learn mode (docs/02 §6.1): each card is asked first as **multiple choice**,
//! then **written**. A correct answer moves it up a stage; a card is **learned** once it was
//! correct at every stage. A wrong written answer sends it back to multiple choice; a wrong
//! multiple-choice answer keeps it there; an "almost" written answer (typo-level) keeps it at
//! the written stage. Cards that are not yet learned come back a few cards later
//! ([`REQUEUE_GAP`]).
//!
//! **Recognition is not recall** ([`updates_schedule`]): a multiple-choice answer on a card
//! already in review (or relearning) is practice only — it moves the card between stages but
//! leaves its FSRS schedule alone; only the written stage rates such a card (correct → Good,
//! almost → Hard, wrong → Again). New and learning cards are rated at both stages, and
//! graduate through the written one.
//!
//! Cards without a usable multiple-choice question (no distinct distractor in the session)
//! start at the written stage: "every stage" is every stage the card can be asked at.

use crate::answer::Grade;
use fc_core::card::{CardState, Rating};
use fc_core::CardId;
use serde::{Deserialize, Serialize};

/// A not-yet-learned card comes back after this many other cards (or at the end of the
/// queue when fewer are left).
pub const REQUEUE_GAP: usize = 3;

/// Most options of a multiple-choice question.
pub const CHOICE_OPTIONS: usize = 4;

/// Stage a card is asked at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LearnStage {
    /// Pick the answer among options.
    Choice,
    /// Type the answer.
    Written,
}

/// Progress of one card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LearnItem {
    /// Card.
    pub card_id: CardId,
    /// Its answer as text when the session started: the distractor pool of the other cards'
    /// multiple-choice questions.
    #[serde(default)]
    pub answer: String,
    /// Stage it is asked at next.
    pub stage: LearnStage,
    /// The card has no multiple-choice question (starts and stays at the written stage).
    #[serde(default)]
    pub written_only: bool,
    /// Correct at every stage.
    pub learned: bool,
    /// Wrong answers so far.
    pub mistakes: u32,
}

/// Learn progress of a session.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct LearnState {
    /// Cards, in session order.
    pub items: Vec<LearnItem>,
}

/// Counts for the progress bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LearnCounts {
    /// Cards in the session.
    pub total: u32,
    /// Learned.
    pub learned: u32,
    /// Waiting at the multiple-choice stage.
    pub choice: u32,
    /// Waiting at the written stage.
    pub written: u32,
}

/// Result of a learn answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LearnStep {
    /// Stage the card was asked at.
    pub stage: LearnStage,
    /// Stage it is asked at next.
    pub next: LearnStage,
    /// The answer completed the card.
    pub learned: bool,
}

impl LearnState {
    /// Every card `(id, answer text, written_only)` starts at multiple choice, or at the
    /// written stage when `written_only` says it has no multiple-choice question.
    pub fn new(cards: impl IntoIterator<Item = (CardId, String, bool)>) -> Self {
        Self {
            items: cards
                .into_iter()
                .map(|(card_id, answer, written_only)| LearnItem {
                    card_id,
                    answer,
                    stage: if written_only {
                        LearnStage::Written
                    } else {
                        LearnStage::Choice
                    },
                    written_only,
                    learned: false,
                    mistakes: 0,
                })
                .collect(),
        }
    }

    /// Progress of `card`.
    pub fn item(&self, card: CardId) -> Option<&LearnItem> {
        self.items.iter().find(|i| i.card_id == card)
    }

    /// Records an answer of `card` graded `grade` (multiple choice is only ever correct or
    /// wrong). `None` when the card is not in the session or already learned.
    pub fn record(&mut self, card: CardId, grade: Grade) -> Option<LearnStep> {
        let item = self
            .items
            .iter_mut()
            .find(|i| i.card_id == card && !i.learned)?;
        let stage = item.stage;
        match (stage, grade) {
            (LearnStage::Choice, Grade::Correct) => item.stage = LearnStage::Written,
            (LearnStage::Choice, Grade::Almost | Grade::Wrong) => item.mistakes += 1,
            (LearnStage::Written, Grade::Correct) => item.learned = true,
            (LearnStage::Written, Grade::Almost) => item.mistakes += 1,
            (LearnStage::Written, Grade::Wrong) => {
                item.mistakes += 1;
                if !item.written_only {
                    item.stage = LearnStage::Choice;
                }
            }
        }
        Some(LearnStep {
            stage,
            next: item.stage,
            learned: item.learned,
        })
    }

    /// Progress counts.
    pub fn counts(&self) -> LearnCounts {
        let mut c = LearnCounts {
            total: u32::try_from(self.items.len()).unwrap_or(u32::MAX),
            ..LearnCounts::default()
        };
        for i in &self.items {
            match (i.learned, i.stage) {
                (true, _) => c.learned += 1,
                (false, LearnStage::Choice) => c.choice += 1,
                (false, LearnStage::Written) => c.written += 1,
            }
        }
        c
    }

    /// Every card is learned.
    pub fn finished(&self) -> bool {
        self.items.iter().all(|i| i.learned)
    }
}

/// Rating a learn answer gives the scheduler: correct → Good, almost → Hard, wrong → Again
/// (the learner does not rate: the answer is judged).
pub const fn rating(grade: Grade) -> Rating {
    grade.suggested_rating()
}

/// Whether an answer at `stage` on a card in scheduling state `state` rates it (changes its
/// FSRS schedule). Multiple choice only tests recognition: on a card already in review or
/// relearning it is practice.
pub const fn updates_schedule(stage: LearnStage, state: CardState) -> bool {
    !matches!(
        (stage, state),
        (
            LearnStage::Choice,
            CardState::Review | CardState::Relearning
        )
    )
}

/// The distractor pool for `card`: the other cards' answers.
pub fn pool(state: &LearnState, card: CardId) -> Vec<String> {
    state
        .items
        .iter()
        .filter(|i| i.card_id != card)
        .map(|i| i.answer.clone())
        .collect()
}

/// Where a not-yet-learned card goes back into a queue of `remaining` other cards.
pub fn requeue_index(remaining: usize) -> usize {
    REQUEUE_GAP.min(remaining)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use uuid::Uuid;

    fn id(n: u128) -> CardId {
        CardId(Uuid::from_u128(n))
    }

    #[test]
    fn learned_only_after_correct_at_every_stage() {
        let mut s = LearnState::new([(id(1), "a".into(), false), (id(2), "b".into(), true)]);
        assert_eq!(
            s.counts(),
            LearnCounts {
                total: 2,
                learned: 0,
                choice: 1,
                written: 1
            }
        );
        // Card 1: choice wrong → stays; choice right → written; written wrong → back to
        // choice; choice right; written almost → stays written; written right → learned.
        let steps = [
            (Grade::Wrong, LearnStage::Choice, false),
            (Grade::Correct, LearnStage::Written, false),
            (Grade::Wrong, LearnStage::Choice, false),
            (Grade::Correct, LearnStage::Written, false),
            (Grade::Almost, LearnStage::Written, false),
            (Grade::Correct, LearnStage::Written, true),
        ];
        for (grade, next, learned) in steps {
            let step = s.record(id(1), grade).unwrap();
            assert_eq!((step.next, step.learned), (next, learned), "{grade:?}");
        }
        assert_eq!(s.item(id(1)).unwrap().mistakes, 3);
        // Learned cards take no more answers.
        assert!(s.record(id(1), Grade::Correct).is_none());
        assert!(!s.finished());
        // Card 2 has no choice question: written wrong stays written, right → learned.
        assert_eq!(
            s.record(id(2), Grade::Wrong).unwrap().next,
            LearnStage::Written
        );
        assert!(s.record(id(2), Grade::Correct).unwrap().learned);
        assert!(s.finished());
        assert!(s.record(id(3), Grade::Correct).is_none());
        // Persisted form roundtrips.
        let json = serde_json::to_value(&s).unwrap();
        assert_eq!(serde_json::from_value::<LearnState>(json).unwrap(), s);
    }

    #[test]
    fn ratings_and_requeue() {
        assert_eq!(rating(Grade::Correct), Rating::Good);
        assert_eq!(rating(Grade::Almost), Rating::Hard);
        assert_eq!(rating(Grade::Wrong), Rating::Again);
        assert_eq!(requeue_index(10), REQUEUE_GAP);
        assert_eq!(requeue_index(1), 1);
        assert_eq!(requeue_index(0), 0);
    }

    #[test]
    fn recognition_does_not_rate_reviewed_cards() {
        for state in [CardState::New, CardState::Learning] {
            assert!(updates_schedule(LearnStage::Choice, state));
            assert!(updates_schedule(LearnStage::Written, state));
        }
        for state in [CardState::Review, CardState::Relearning] {
            assert!(!updates_schedule(LearnStage::Choice, state));
            assert!(updates_schedule(LearnStage::Written, state));
        }
    }
}
