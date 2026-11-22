//! The word index (v1.4, docs/03 §4.2): "which words does he know", one answer for reading
//! mode, the Subtitle extension and scripts. A word is a (language, key) pair: the language
//! is a primary subtag (`en`) and the key a lemma or a short expression (`go`, `pick up`).
//! Its evidence is kept apart (manual mark; notes, cards and the longest interval) and its
//! status is derived: a manual mark wins, else a card whose interval reached
//! `words.known_interval_days` → known, else any card → learning, else new.

use crate::v1::io::ImportSource;
use fc_core::reading::{WordCounts, WordMark, WordStatus};
use fc_core::word_data::{BandOrigin, CefrBand, PackSource};
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
    /// Frequency rank and CEFR band (`None` = the word-data module is off or the pack does
    /// not know the word).
    pub level: Option<WordLevelDto>,
}

/// How often a word is heard and its estimated CEFR band, from the language's word-data
/// pack (v1.4, docs/03 §4.3; sources and licences: `words.data.sources`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct WordLevelDto {
    /// The pack's lemma the rank belongs to (`went` → `go`); `None` for an expression.
    pub lemma: Option<String>,
    /// Frequency rank of the lemma (1 = heard most); `None` for an expression or a profile
    /// headword the frequency list lacks.
    pub rank: Option<u32>,
    /// CEFR band.
    pub band: Option<CefrBand>,
    /// Where the band comes from: a CEFR profile list, the rank, or the expression rule.
    pub band_origin: Option<BandOrigin>,
    /// The band is an estimate (from the rank or the expression rule, not a profile list).
    pub estimated: bool,
}

/// A member of a word's family (derivation: `act`, `actor`, `action`). Its status is its
/// own: knowing one member never makes another known.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct FamilyMemberDto {
    /// Lemma.
    pub key: String,
    /// Its status in the index (`new` when the index does not know it).
    pub status: WordStatus,
    /// The family's head (the base word).
    pub head: bool,
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
    /// Frequency rank and CEFR band of the form (also for words the index does not have;
    /// `None` = module off or unknown to the pack).
    pub level: Option<WordLevelDto>,
    /// Other members of its word family (empty = none known or module off).
    pub family: Vec<FamilyMemberDto>,
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
    /// Order (default `key`). `rank` and `band` need the word-data module.
    #[serde(default)]
    pub sort: Option<WordSort>,
    /// Only words in these CEFR bands (word-data module).
    #[serde(default)]
    pub bands: Vec<CefrBand>,
    /// Only words whose frequency rank is at most this (word-data module): `5000` = among
    /// the 5000 lemmas heard most.
    #[serde(default)]
    pub max_rank: Option<u32>,
}

/// Order of `words.list`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum WordSort {
    /// By language and key.
    #[default]
    Key,
    /// Heard most first (words without a rank, such as expressions, last).
    Rank,
    /// Easiest CEFR band first, then by rank (words without a band last).
    Band,
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
    /// Words per status in each CEFR band (`band: None` = no band known; empty when the
    /// word-data module is off). Expressions count under their key's own pack row only.
    pub bands: Vec<BandWordCounts>,
}

/// Word counts of one CEFR band.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct BandWordCounts {
    /// Band (`None` = the pack does not know these words).
    pub band: Option<CefrBand>,
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

/// An installed word-data pack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct WordDataPackDto {
    /// Language (primary subtag).
    pub language: String,
    /// Pack version.
    pub version: String,
    /// Name.
    pub name: String,
    /// Licence of the pack.
    pub licence: String,
    /// Sources with their licences and the attribution they ask for.
    pub sources: Vec<PackSource>,
    /// How it was made.
    pub notes: String,
    /// `bundled` (shipped with the app) or `imported` (`words.data.import`).
    pub origin: String,
    /// Lemmas.
    pub words: u32,
    /// Rows (lemmas, inflected forms and profile headwords).
    pub forms: u32,
    /// Word-family links.
    pub families: u32,
    /// When it was installed.
    pub installed_at: Timestamp,
}

/// A pack shipped with the app.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct BundledPackDto {
    /// Language.
    pub language: String,
    /// Version.
    pub version: String,
}

/// `words.data.sources` result: what word data is installed and where it comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct WordDataSources {
    /// Installed packs, by language.
    pub packs: Vec<WordDataPackDto>,
    /// Packs shipped with this build.
    pub bundled: Vec<BundledPackDto>,
}

/// `words.data.import`: replaces a language's pack with one from a file or text (format:
/// `tools/word-data/README.md`). A shipped pack never overwrites an imported one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ImportWordData {
    /// The pack (JSON).
    pub source: ImportSource,
}

/// `words.data.reset`: puts the shipped pack of a language back (or removes the language's
/// pack when none is shipped).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ResetWordData {
    /// Language (BCP-47 or primary subtag).
    pub language: String,
}
