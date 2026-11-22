//! v1.4 quiz — the mode the app calls **"Test"** (docs/02 §6.1): one word at a time, up to
//! four meanings to pick from, the right one shown at once, then the next word; at the end the
//! score and the missed words ("study the missed ones again" starts a quiz of those).
//!
//! The quiz is fixed when it starts: every word gets its options (the right meaning and up to
//! three distractors) so the screen, the answer handler and the result all see the same ones.
//! Distractors are chosen by the caller from the **same deck and language and the same field
//! role** (the app passes other words' `meaning` fields — never examples or sources);
//! [`options`] only makes them distinct: never the right meaning again (by [`option_key`]),
//! never a duplicate, never empty. With fewer than three usable distractors a word gets fewer
//! options ([`QuizItem::few_options`]); a word with none cannot be asked (one option is not a
//! question) and is left out.
//!
//! Each word is asked once (no re-queueing: this is a check, not drilling — Learn drills).

use super::{option_key, shuffle};
use fc_core::rng::Rng;
use fc_core::CardId;
use serde::{Deserialize, Serialize};

/// Most options of a word.
pub const OPTIONS: usize = 4;

/// One word of a quiz.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuizItem {
    /// Card asked.
    pub card_id: CardId,
    /// The word (plain text of the `term` field; `None` when the note has no field roles).
    #[serde(default)]
    pub term: Option<String>,
    /// The card's question as plain text (missed list fallback when `term` is `None`).
    #[serde(default)]
    pub prompt: String,
    /// Options (plain text), fixed at start.
    pub options: Vec<String>,
    /// Index of the right one.
    pub correct: u32,
    /// The option picked (`None` = not answered yet).
    #[serde(default)]
    pub picked: Option<u32>,
}

impl QuizItem {
    /// Fewer than [`OPTIONS`] options (the deck has too few other meanings).
    pub fn few_options(&self) -> bool {
        self.options.len() < OPTIONS
    }

    /// Answered right.
    pub fn is_correct(&self) -> bool {
        self.picked == Some(self.correct)
    }

    /// The right meaning.
    pub fn expected(&self) -> &str {
        self.options
            .get(self.correct as usize)
            .map_or("", String::as_str)
    }

    /// The word as shown in the missed list.
    pub fn label(&self) -> &str {
        self.term.as_deref().unwrap_or(&self.prompt)
    }
}

/// Quiz state of a session.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct QuizState {
    /// Words, in quiz order.
    pub items: Vec<QuizItem>,
    /// Answers change the schedule (`quiz.affects_schedule` when the quiz started).
    #[serde(default)]
    pub affects_schedule: bool,
}

/// Counts for the progress bar and the result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct QuizCounts {
    /// Words.
    pub total: u32,
    /// Answered.
    pub answered: u32,
    /// Answered right.
    pub correct: u32,
}

impl QuizCounts {
    /// Answered wrong.
    pub const fn wrong(self) -> u32 {
        self.answered - self.correct
    }

    /// Right answers of the answered words, 0–100 (0 before the first answer).
    pub fn percent(self) -> u32 {
        if self.answered == 0 {
            0
        } else {
            (f64::from(self.correct) * 100.0 / f64::from(self.answered)).round() as u32
        }
    }
}

impl QuizState {
    /// The word of `card`.
    pub fn item(&self, card: CardId) -> Option<&QuizItem> {
        self.items.iter().find(|i| i.card_id == card)
    }

    /// Records the pick `choice` for `card`: `Some(right)`, or `None` when the card is not in
    /// the quiz, was answered already, or `choice` is not one of its options.
    pub fn record(&mut self, card: CardId, choice: u32) -> Option<bool> {
        let item = self
            .items
            .iter_mut()
            .find(|i| i.card_id == card && i.picked.is_none())?;
        if choice as usize >= item.options.len() {
            return None;
        }
        item.picked = Some(choice);
        Some(item.is_correct())
    }

    /// Progress counts.
    pub fn counts(&self) -> QuizCounts {
        let n = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
        QuizCounts {
            total: n(self.items.len()),
            answered: n(self.items.iter().filter(|i| i.picked.is_some()).count()),
            correct: n(self.items.iter().filter(|i| i.is_correct()).count()),
        }
    }

    /// Every word is answered.
    pub fn finished(&self) -> bool {
        self.items.iter().all(|i| i.picked.is_some())
    }

    /// Words answered wrong, in quiz order.
    pub fn missed(&self) -> impl Iterator<Item = &QuizItem> {
        self.items
            .iter()
            .filter(|i| i.picked.is_some() && !i.is_correct())
    }
}

/// Options for a word meaning `correct`: up to `OPTIONS - 1` distractors from `pool` (other
/// words' meanings, already restricted to the same deck, language and role by the caller),
/// skipping empty texts, the right meaning and duplicates (by [`option_key`]); shuffled.
/// Returns the options and the right index, or `None` when `pool` has no usable distractor.
pub fn options(correct: &str, pool: &[String], rng: &mut dyn Rng) -> Option<(Vec<String>, u32)> {
    super::choice_options(correct, pool, OPTIONS, rng)
}

/// The card's own options (choice card types) trimmed to [`OPTIONS`]: the right one always
/// stays; the others are taken in order. `None` with fewer than two distinct options.
pub fn own_options(own: &[String], correct: u32, rng: &mut dyn Rng) -> Option<(Vec<String>, u32)> {
    let right = own.get(correct as usize)?;
    let pool: Vec<String> = own
        .iter()
        .enumerate()
        .filter(|(i, _)| *i != correct as usize)
        .map(|(_, o)| o.clone())
        .collect();
    // Keep the card's order when it fits; otherwise trim like any other word.
    if own.len() <= OPTIONS {
        let mut keys: Vec<String> = own.iter().map(|o| option_key(o)).collect();
        keys.sort();
        keys.dedup();
        if keys.len() == own.len() && own.len() >= 2 && !keys.iter().any(String::is_empty) {
            return Some((own.to_vec(), correct));
        }
    }
    let mut picked = options(right, &pool, rng)?;
    // `options` shuffles; that is fine for trimmed lists.
    picked.0.truncate(OPTIONS);
    Some(picked)
}

/// Shuffles the quiz order.
pub fn shuffle_items(items: &mut [QuizItem], rng: &mut dyn Rng) {
    shuffle(items, rng);
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use fc_core::rng::SplitMix64;
    use std::collections::HashSet;
    use uuid::Uuid;

    fn id(n: u128) -> CardId {
        CardId(Uuid::from_u128(n))
    }

    fn strings(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn four_distinct_options_never_the_same_meaning_twice() {
        // The pool repeats the right meaning (another note with the same meaning), has
        // case/space duplicates and an empty field.
        let pool = strings(&[
            "gitmek",
            "Gelmek",
            "gelmek ",
            "",
            "koşmak",
            "yürümek",
            "Gitmek",
        ]);
        for seed in 0..100 {
            let (opts, i) = options("gitmek", &pool, &mut SplitMix64::new(seed)).unwrap();
            assert_eq!(opts.len(), OPTIONS);
            assert_eq!(opts[i as usize], "gitmek");
            let keys: HashSet<String> = opts.iter().map(|o| option_key(o)).collect();
            assert_eq!(keys.len(), OPTIONS, "{opts:?}");
            assert_eq!(opts.iter().filter(|o| option_key(o) == "gitmek").count(), 1);
        }
    }

    #[test]
    fn small_decks_get_fewer_options_and_say_so() {
        // Two other meanings: three options.
        let (opts, i) = options("a", &strings(&["b", "c"]), &mut SplitMix64::new(1)).unwrap();
        assert_eq!(opts.len(), 3);
        assert_eq!(opts[i as usize], "a");
        let item = QuizItem {
            card_id: id(1),
            term: Some("x".into()),
            prompt: String::new(),
            options: opts,
            correct: i,
            picked: None,
        };
        assert!(item.few_options());
        // One other meaning: two options.
        assert_eq!(
            options("a", &strings(&["b"]), &mut SplitMix64::new(1))
                .unwrap()
                .0
                .len(),
            2
        );
        // Nothing usable: no question.
        assert!(options("a", &strings(&["A", " a", ""]), &mut SplitMix64::new(1)).is_none());
        assert!(options("a", &[], &mut SplitMix64::new(1)).is_none());
    }

    #[test]
    fn own_options_keep_the_card_order_or_are_trimmed() {
        let own = strings(&["x", "y", "z"]);
        assert_eq!(
            own_options(&own, 2, &mut SplitMix64::new(1)),
            Some((own.clone(), 2))
        );
        let many = strings(&["a", "b", "c", "d", "e", "f"]);
        let (opts, i) = own_options(&many, 4, &mut SplitMix64::new(3)).unwrap();
        assert_eq!(opts.len(), OPTIONS);
        assert_eq!(opts[i as usize], "e");
        assert!(own_options(&strings(&["a", "A"]), 0, &mut SplitMix64::new(1)).is_none());
    }

    #[test]
    fn answers_once_score_and_missed_list() {
        let item = |n: u128, correct: u32| QuizItem {
            card_id: id(n),
            term: Some(format!("w{n}")),
            prompt: String::new(),
            options: strings(&["a", "b", "c", "d"]),
            correct,
            picked: None,
        };
        let mut q = QuizState {
            items: vec![item(1, 0), item(2, 1), item(3, 2)],
            affects_schedule: false,
        };
        assert_eq!(
            q.counts(),
            QuizCounts {
                total: 3,
                answered: 0,
                correct: 0
            }
        );
        assert_eq!(q.counts().percent(), 0);
        assert_eq!(q.record(id(1), 0), Some(true));
        // Answered once only; out-of-range picks and unknown cards are refused.
        assert_eq!(q.record(id(1), 1), None);
        assert_eq!(q.record(id(2), 4), None);
        assert_eq!(q.record(id(9), 0), None);
        assert_eq!(q.record(id(2), 3), Some(false));
        assert!(!q.finished());
        let c = q.counts();
        assert_eq!(
            (c.answered, c.correct, c.wrong(), c.percent()),
            (2, 1, 1, 50)
        );
        assert_eq!(
            q.missed().map(|i| i.card_id).collect::<Vec<_>>(),
            vec![id(2)]
        );
        assert_eq!(q.record(id(3), 2), Some(true));
        assert!(q.finished());
        assert_eq!(q.counts().percent(), 67);
        assert_eq!(q.item(id(2)).unwrap().expected(), "b");
        // Persisted form roundtrips.
        let json = serde_json::to_value(&q).unwrap();
        assert_eq!(serde_json::from_value::<QuizState>(json).unwrap(), q);
    }
}
