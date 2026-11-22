//! The word index (v1.4, docs/03 §5.2): "which words does he know", one answer for reading
//! mode, the Subtitle extension and scripts. A word is a (language, key) pair: the language
//! is a primary subtag (`en`) and the key a lemma or a short expression (`go`, `pick up`).
//! Its evidence is kept apart (manual mark; notes, cards and the longest interval) and its
//! status is derived: a manual mark wins, else a card whose interval reached
//! `words.known_interval_days` → known, else any card → learning, else new.

use fc_core::reading::{WordCounts, WordMark, WordStatus};
use fc_core::{CardId, NoteId, Timestamp};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// One word of the index with its evidence.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct WordEntryDto {
    /// Language (primary subtag).
    pub language: String,
    /// Lemma or expression.
    pub key: String,
    /// Derived status.
    pub status: WordStatus,
    /// Manual mark (wins over the cards).
    pub mark: Option<WordMark>,
    /// When the mark was set.
    pub mark_at: Option<Timestamp>,
    /// Notes whose term is this word.
    pub notes: u32,
    /// Live cards of those notes.
    pub cards: u32,
    /// Longest review interval of those cards (study days).
    pub max_interval_days: Option<u32>,
    /// A card's interval reached the threshold (whatever the mark says).
    pub known_by_maturity: bool,
    /// Newest review.
    pub last_review: Option<Timestamp>,
    /// When the first note was made.
    pub first_seen: Option<Timestamp>,
    /// The notes behind it (`words.lookup` only; empty in listings).
    pub note_ids: Vec<NoteId>,
    /// Their live cards (`words.lookup` only; empty in listings).
    pub card_ids: Vec<CardId>,
}

/// `words.lookup`: what the index knows about forms as they appear in a text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct LookupWords {
    /// BCP-47 language (empty = `reading.default_language`).
    #[serde(default)]
    pub language: String,
    /// Word forms or expressions (1–2000): `went`, `picked up`.
    pub forms: Vec<String>,
}

/// One looked-up form.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct WordFormResult {
    /// The form as given.
    pub form: String,
    /// Its key (`None` = no word in it, or longer than an expression).
    pub key: Option<String>,
    /// Status (`new` when the index does not know it).
    pub status: WordStatus,
    /// The index row, if any.
    pub word: Option<WordEntryDto>,
}

/// `words.lookup` result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct WordsLookupResult {
    /// Language (primary subtag).
    pub language: String,
    /// Lemmatizer used.
    pub lemmatizer: String,
    /// Known-by-maturity threshold in force.
    pub threshold_days: u32,
    /// One per form, in order.
    pub forms: Vec<WordFormResult>,
}

/// `words.list`: index rows by language and key.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ListWords {
    /// Only this language.
    #[serde(default)]
    pub language: Option<String>,
    /// Only this status.
    #[serde(default)]
    pub status: Option<WordStatus>,
    /// Page cursor (`next_cursor` of the previous page).
    #[serde(default)]
    pub cursor: Option<String>,
    /// Page size (1–1000, default 200).
    #[serde(default)]
    pub limit: Option<u32>,
}

/// A page of words.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct WordList {
    /// Words.
    pub words: Vec<WordEntryDto>,
    /// Cursor of the next page (`None` = last page).
    pub next_cursor: Option<String>,
}

/// Word counts of one language.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct LanguageWordCounts {
    /// Language.
    pub language: String,
    /// Words per status.
    pub counts: WordCounts,
}

/// `words.stats` result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct WordStats {
    /// Known-by-maturity threshold in force.
    pub threshold_days: u32,
    /// Per language.
    pub languages: Vec<LanguageWordCounts>,
    /// Notes with a term whose language is not known (no capture language and no
    /// `words.language` for their deck): not indexed under any word.
    pub unassigned_notes: u32,
}

/// `words.index.rebuild_now` result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct WordIndexRebuilt {
    /// Words in the index.
    pub words: u32,
    /// Notes indexed (with or without a language).
    pub notes: u32,
}
