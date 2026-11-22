//! The word index (v1.4, docs/03 §5.2): the pure rules behind "which words does he know".
//!
//! A word is keyed by **(language, key)**: the language is a primary subtag (`en`, never
//! `en-GB`, see [`index_language`]) and the key is the lemma of a word or the lemmas of a
//! short expression joined by a space (`go`, `pick up`; [`crate::reading::headword_key`]).
//! Only **inflection** joins a key — the lemmatizer maps `went` to `go` — never derivation:
//! `actor` and `act` stay two words.
//!
//! The evidence of a word is kept apart ([`WordEvidence`]): the user's manual mark and what
//! its notes say (how many notes and cards, the longest review interval). Its status is
//! derived ([`WordEvidence::status`]): **a manual mark wins** (known / ignored), else a card
//! whose interval reached the threshold makes it *known by maturity*, else any card makes it
//! *learning*, else it is *new*.

use crate::card::CardState;
use crate::reading::{WordMark, WordStatus};
use crate::Timestamp;

/// Default of the "known by maturity" threshold (setting `words.known_interval_days`): the
/// same 21 days the statistics call mature and the Subtitle contract calls known.
pub const DEFAULT_KNOWN_INTERVAL_DAYS: u32 = crate::stats::MATURE_DAYS;

/// Index language of a BCP-47 tag: its primary subtag, lower case (`en-GB` → `en`). Empty
/// for a tag that does not start with 2–8 ASCII letters (unknown language).
pub fn index_language(tag: &str) -> String {
    let primary = tag.trim().split(['-', '_']).next().unwrap_or("");
    if (2..=8).contains(&primary.len()) && primary.chars().all(|c| c.is_ascii_alphabetic()) {
        primary.to_ascii_lowercase()
    } else {
        String::new()
    }
}

/// What one note says about its word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteWordFacts {
    /// Index language (`""` = unknown: reported, not indexed under a word).
    pub language: String,
    /// Word key.
    pub key: String,
    /// Live cards of the note.
    pub cards: u32,
    /// Longest interval (study days) among its review cards; `None` = no review card.
    pub max_interval_days: Option<u32>,
    /// Newest review of its cards.
    pub last_review: Option<Timestamp>,
    /// When the note was made.
    pub created_at: Timestamp,
}

/// Card count, longest review interval and newest review of a note's live cards, from each
/// card's `(state, interval in study days, last review)`.
pub fn card_facts(
    cards: &[(CardState, Option<u32>, Option<Timestamp>)],
) -> (u32, Option<u32>, Option<Timestamp>) {
    let count = u32::try_from(cards.len()).unwrap_or(u32::MAX);
    let max_interval = cards
        .iter()
        .filter(|(s, _, _)| *s == CardState::Review)
        .filter_map(|(_, i, _)| *i)
        .max();
    let last = cards.iter().filter_map(|(_, _, l)| *l).max();
    (count, max_interval, last)
}

/// The evidence of one word, each source apart.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WordEvidence {
    /// The user's manual mark and when it was set.
    pub mark: Option<(WordMark, Timestamp)>,
    /// Notes of the word.
    pub notes: u32,
    /// Live cards of those notes.
    pub cards: u32,
    /// Longest review interval among those cards (study days).
    pub max_interval_days: Option<u32>,
    /// Newest review among those cards.
    pub last_review: Option<Timestamp>,
    /// When its first note was made.
    pub first_seen: Option<Timestamp>,
}

impl WordEvidence {
    /// Folds a word's notes and mark. `None` when there is neither (nothing to index).
    pub fn collect<'a>(
        notes: impl IntoIterator<Item = &'a NoteWordFacts>,
        mark: Option<(WordMark, Timestamp)>,
    ) -> Option<Self> {
        let mut ev = Self {
            mark,
            notes: 0,
            cards: 0,
            max_interval_days: None,
            last_review: None,
            first_seen: None,
        };
        for n in notes {
            ev.notes = ev.notes.saturating_add(1);
            ev.cards = ev.cards.saturating_add(n.cards);
            ev.max_interval_days = ev.max_interval_days.max(n.max_interval_days);
            ev.last_review = ev.last_review.max(n.last_review);
            ev.first_seen = Some(ev.first_seen.map_or(n.created_at, |f| f.min(n.created_at)));
        }
        (ev.notes > 0 || ev.mark.is_some()).then_some(ev)
    }

    /// Known by maturity: a review card's interval reached `threshold_days`.
    pub fn known_by_maturity(&self, threshold_days: u32) -> bool {
        self.cards > 0 && self.max_interval_days.is_some_and(|d| d >= threshold_days)
    }

    /// Status from the cards alone (the mark ignored).
    pub fn derived_status(&self, threshold_days: u32) -> WordStatus {
        if self.known_by_maturity(threshold_days) {
            WordStatus::Known
        } else if self.cards > 0 {
            WordStatus::Learning
        } else {
            WordStatus::New
        }
    }

    /// The word's status: a manual mark wins over what the cards say.
    pub fn status(&self, threshold_days: u32) -> WordStatus {
        match self.mark {
            Some((WordMark::Known, _)) => WordStatus::Known,
            Some((WordMark::Ignored, _)) => WordStatus::Ignored,
            None => self.derived_status(threshold_days),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(cards: u32, interval: Option<u32>, created: i64) -> NoteWordFacts {
        NoteWordFacts {
            language: "en".into(),
            key: "go".into(),
            cards,
            max_interval_days: interval,
            last_review: interval.map(|_| Timestamp(created + 10)),
            created_at: Timestamp(created),
        }
    }

    #[test]
    fn language_is_the_primary_subtag() {
        assert_eq!(index_language("en-GB"), "en");
        assert_eq!(index_language(" TR "), "tr");
        assert_eq!(index_language("pt_BR"), "pt");
        assert_eq!(index_language(""), "");
        assert_eq!(index_language("1x"), "");
    }

    #[test]
    fn a_manual_mark_wins_over_the_cards() {
        let notes = [note(2, Some(40), 5)];
        let known = WordEvidence::collect(&notes, None).unwrap();
        assert_eq!(known.status(21), WordStatus::Known);
        assert!(known.known_by_maturity(21));
        assert_eq!(
            known.status(60),
            WordStatus::Learning,
            "threshold is a setting"
        );
        let ignored =
            WordEvidence::collect(&notes, Some((WordMark::Ignored, Timestamp(9)))).unwrap();
        assert_eq!(ignored.status(21), WordStatus::Ignored);
        assert_eq!(
            ignored.derived_status(21),
            WordStatus::Known,
            "evidence kept apart"
        );
        let marked = WordEvidence::collect(&[], Some((WordMark::Known, Timestamp(1)))).unwrap();
        assert_eq!(marked.status(21), WordStatus::Known);
        assert!(!marked.known_by_maturity(21));
    }

    #[test]
    fn folding_notes() {
        let notes = [note(1, None, 7), note(2, Some(3), 4), note(0, None, 9)];
        let ev = WordEvidence::collect(&notes, None).unwrap();
        assert_eq!((ev.notes, ev.cards, ev.max_interval_days), (3, 3, Some(3)));
        assert_eq!(ev.first_seen, Some(Timestamp(4)));
        assert_eq!(ev.last_review, Some(Timestamp(14)));
        assert_eq!(ev.status(21), WordStatus::Learning);
        assert_eq!(WordEvidence::collect(&[], None), None);
        let none = WordEvidence::collect(&[note(0, None, 1)], None).unwrap();
        assert_eq!(none.status(21), WordStatus::New);
    }

    #[test]
    fn card_facts_take_review_intervals_only() {
        let (n, max, last) = card_facts(&[
            (CardState::Learning, Some(90), Some(Timestamp(3))),
            (CardState::Review, Some(12), Some(Timestamp(2))),
            (CardState::New, None, None),
        ]);
        assert_eq!((n, max, last), (3, Some(12), Some(Timestamp(3))));
    }
}
