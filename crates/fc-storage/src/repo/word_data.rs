//! Word data (v1.4 B2, migration `1440_word_data`): the installed word-data pack of each
//! language — its rows (form → lemma, rank, band), word families and the pack's own record
//! (version, licence, sources). The rules (what a band is, how a pack becomes rows) are
//! `fc_core::word_data`'s; storage keeps and finds rows.

use super::*;

/// One `word_data_packs` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WordDataPackRecord {
    /// Language (primary subtag).
    pub language: String,
    /// Pack version.
    pub version: String,
    /// Display name.
    pub name: String,
    /// Licence of the pack.
    pub licence: String,
    /// Sources (JSON array of `fc_core::word_data::PackSource`).
    pub sources: serde_json::Value,
    /// How it was made.
    pub notes: String,
    /// `bundled` (shipped with the app) or `imported`.
    pub origin: String,
    /// Lemmas.
    pub words: u32,
    /// Rows (lemmas, forms and profile headwords).
    pub forms: u32,
    /// Family links.
    pub families: u32,
    /// Checksum of the pack's bytes.
    pub checksum: String,
    /// When it was installed.
    pub installed_at: Timestamp,
}

/// One `word_data` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WordDataRecord {
    /// Language.
    pub language: String,
    /// Form (a lemma is its own form).
    pub form: String,
    /// Its lemma.
    pub lemma: String,
    /// The lemma's frequency rank (1 = heard most).
    pub rank: Option<u32>,
    /// `A1`…`C2`.
    pub band: Option<String>,
    /// `profile` or `rank`.
    pub band_origin: Option<String>,
}

/// Order of [`WordDataRead::word_index_page_with_data`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum WordDataSort {
    /// By language and key (the index's own order).
    #[default]
    Key,
    /// Most heard first (words without a rank last), then language and key.
    Rank,
    /// Easiest band first, then rank (words without a band last), then language and key.
    Band,
}

/// Filters and order over the word data, next to a [`WordIndexQuery`].
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WordDataQuery {
    /// Order.
    pub sort: WordDataSort,
    /// Only words with one of these bands (`A1`…; empty = any).
    pub bands: Vec<String>,
    /// Only words whose rank is at most this.
    pub max_rank: Option<u32>,
    /// Sort values of the cursor row: (band sort value, rank sort value), with
    /// [`NO_BAND`] / [`NO_RANK`] for a missing band / rank. Used with `WordIndexQuery::after`.
    pub after_sort: Option<(String, u32)>,
}

/// Sort value of a missing band (after `C2`).
pub const NO_BAND: &str = "Z";
/// Sort value of a missing rank (after every rank).
pub const NO_RANK: u32 = u32::MAX;

/// A word of the index with its word data (if the pack has its key).
pub type WordWithData = (WordIndexRecord, Option<WordDataRecord>);

/// Reads of the word data.
pub trait WordDataRead {
    /// Installed packs, by language.
    fn word_data_packs(&self) -> StorageResult<Vec<WordDataPackRecord>>;
    /// Rows of `forms` in `language` (missing forms are simply absent).
    fn word_data_get(&self, language: &str, forms: &[String])
        -> StorageResult<Vec<WordDataRecord>>;
    /// `(lemma, head)` family links of `lemmas` as members **or** heads in `language`.
    fn word_families_of(
        &self,
        language: &str,
        lemmas: &[String],
    ) -> StorageResult<Vec<(String, String)>>;
    /// A page of the word index joined with the word data on (language, key = form), with
    /// the index query's filters (language, status, after) and the data query's (bands,
    /// rank, order, cursor sort values).
    fn word_index_page_with_data(
        &self,
        q: &WordIndexQuery,
        d: &WordDataQuery,
    ) -> StorageResult<Vec<WordWithData>>;
    /// `(language, band or None, status, words)` of the index with `threshold_days`.
    fn word_index_band_counts(
        &self,
        threshold_days: u32,
    ) -> StorageResult<Vec<(String, Option<String>, String, u32)>>;
}

/// Writes of the word data.
pub trait WordDataWrite {
    /// Replaces a language's pack: its record, rows and families (one transaction, the
    /// caller's).
    fn replace_word_data(
        &mut self,
        pack: &WordDataPackRecord,
        rows: &[WordDataRecord],
        families: &[(String, String)],
    ) -> StorageResult<()>;
    /// Removes a language's pack.
    fn remove_word_data(&mut self, language: &str) -> StorageResult<()>;
}
