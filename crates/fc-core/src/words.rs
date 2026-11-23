//! The word index (v1.4, docs/03 §4.2): the pure rules behind "which words does he know".
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

// ------------------------------------------------------------------ history (v1.4 F1)

/// A review as the known-words history reads it: the word it counts for (an index into the
/// caller's word list), the card (an index into the caller's card list), the study day it
/// was given on and whether it left the card *known by maturity* (state `review` with a
/// scheduled interval ≥ the threshold).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KnownEvent {
    /// Word (caller's index).
    pub word: u32,
    /// Card (caller's index).
    pub card: u32,
    /// Study day of the review.
    pub day: i64,
    /// The review left the card known by maturity.
    pub known: bool,
}

/// A word's current manual mark and the study day it was set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarkEvent {
    /// Word (caller's index).
    pub word: u32,
    /// The mark.
    pub mark: WordMark,
    /// Study day it was set.
    pub day: i64,
}

/// Words known at the end of each study day `from..=to` (`words.history`).
///
/// Definition (docs/03 §4.2): a word is known at the end of day *d* when its manual mark
/// set on or before *d* is `known`; when that mark is `ignored` it is not; otherwise when
/// one of its cards' latest review on or before *d* left that card known by maturity.
/// Only the **current** mark is stored (with its time), so before it was set the word counts
/// by its cards alone. `events` must be sorted by day (ties in any order: one day's last
/// review per card wins, as it does in the log's time order when the caller sorts by time).
pub fn known_history(
    words: usize,
    cards: usize,
    events: &[KnownEvent],
    marks: &[MarkEvent],
    from: i64,
    to: i64,
) -> Vec<u32> {
    let mut card_known = vec![false; cards];
    let mut known_cards = vec![0u32; words];
    let mut forced: Vec<Option<bool>> = vec![None; words];
    let mut marks: Vec<MarkEvent> = marks.to_vec();
    marks.sort_by_key(|m| m.day);
    let mut known: u32 = 0;
    let is_known = |kc: u32, f: Option<bool>| f.unwrap_or(kc > 0);
    let (mut e, mut m) = (0, 0);
    let mut out = Vec::with_capacity(usize::try_from((to - from + 1).max(0)).unwrap_or(0));
    for day in from..=to {
        while let Some(ev) = events.get(e).filter(|ev| ev.day <= day) {
            e += 1;
            let (w, c) = (ev.word as usize, ev.card as usize);
            if w >= words || c >= cards || card_known[c] == ev.known {
                continue;
            }
            let before = is_known(known_cards[w], forced[w]);
            card_known[c] = ev.known;
            if ev.known {
                known_cards[w] += 1;
            } else {
                known_cards[w] = known_cards[w].saturating_sub(1);
            }
            let after = is_known(known_cards[w], forced[w]);
            known = adjust(known, before, after);
        }
        while let Some(mk) = marks.get(m).filter(|mk| mk.day <= day) {
            m += 1;
            let w = mk.word as usize;
            if w >= words {
                continue;
            }
            let before = is_known(known_cards[w], forced[w]);
            forced[w] = Some(mk.mark == WordMark::Known);
            let after = is_known(known_cards[w], forced[w]);
            known = adjust(known, before, after);
        }
        out.push(known);
    }
    out
}

fn adjust(n: u32, before: bool, after: bool) -> u32 {
    match (before, after) {
        (false, true) => n.saturating_add(1),
        (true, false) => n.saturating_sub(1),
        _ => n,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(word: u32, card: u32, day: i64, known: bool) -> KnownEvent {
        KnownEvent {
            word,
            card,
            day,
            known,
        }
    }

    #[test]
    fn known_history_counts_cards_and_marks() {
        // Word 0: card 0 matures on day 2, lapses on day 4. Word 1: two cards, one matures
        // on day 1 (before the window). Word 2: marked known on day 3. Word 3: card mature
        // from day 0, marked ignored on day 5.
        let events = [
            ev(3, 3, 0, true),
            ev(1, 1, 1, true),
            ev(0, 0, 2, false),
            ev(0, 0, 2, true),
            ev(1, 2, 3, true),
            ev(0, 0, 4, false),
        ];
        let marks = [
            MarkEvent {
                word: 2,
                mark: WordMark::Known,
                day: 3,
            },
            MarkEvent {
                word: 3,
                mark: WordMark::Ignored,
                day: 5,
            },
        ];
        let h = known_history(4, 4, &events, &marks, 1, 6);
        assert_eq!(h, vec![2, 3, 4, 3, 2, 2]);
        assert!(known_history(0, 0, &[], &[], 3, 2).is_empty());
    }

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
