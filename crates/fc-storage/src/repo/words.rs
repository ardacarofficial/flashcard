//! The word index (v1.4, migration `1410_word_index`): per-note contributions, per-word
//! evidence, the dirty queues the triggers fill, and producer-known note languages.
//! The rules (keys, languages, status) are the app's (`fc_app::modules::words`,
//! `fc_core::words`); storage only keeps and finds rows.

use super::*;
use fc_core::card::CardState;

/// One `word_index_notes` row: what a note contributes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WordIndexNoteRecord {
    /// Note.
    pub note_id: NoteId,
    /// Index language (`""` = unknown).
    pub language: String,
    /// Word key.
    pub key: String,
    /// Live cards.
    pub cards: u32,
    /// Longest review interval (study days).
    pub max_interval_days: Option<u32>,
    /// Newest review.
    pub last_review: Option<Timestamp>,
    /// Note creation.
    pub created_at: Timestamp,
}

/// One `word_index` row: a word's evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WordIndexRecord {
    /// Index language.
    pub language: String,
    /// Word key.
    pub key: String,
    /// Manual mark (`known` / `ignored`).
    pub mark: Option<String>,
    /// When the mark was set.
    pub mark_at: Option<Timestamp>,
    /// Notes.
    pub notes: u32,
    /// Live cards of those notes.
    pub cards: u32,
    /// Longest review interval (study days).
    pub max_interval_days: Option<u32>,
    /// Newest review.
    pub last_review: Option<Timestamp>,
    /// First note's creation.
    pub first_seen: Option<Timestamp>,
}

/// A page of [`WordIndexRead::word_index_page`]. Status is derived on read as
/// `fc_core::words::WordEvidence::status` does it (mark, else interval ≥ `threshold_days` →
/// known, else any card → learning, else new).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct WordIndexQuery {
    /// Only this language.
    pub language: Option<String>,
    /// Only these statuses (`new`, `learning`, `known`, `ignored`; empty = all).
    pub statuses: Vec<String>,
    /// Known-by-maturity threshold.
    pub threshold_days: u32,
    /// Rows after this `(language, key)`.
    pub after: Option<(String, String)>,
    /// Most rows.
    pub limit: u32,
}

/// What the index reads of a note.
#[derive(Debug, Clone, PartialEq)]
pub struct WordNoteSource {
    /// Note.
    pub note_id: NoteId,
    /// Note type.
    pub notetype_id: NoteTypeId,
    /// Home deck.
    pub home_deck_id: DeckId,
    /// Field values (raw).
    pub fields: fc_core::note::FieldValues,
    /// Created.
    pub created_at: Timestamp,
    /// Language of a Subtitle capture of the note (any, if several).
    pub capture_lang: Option<String>,
    /// Language a producer recorded (`word_note_language`).
    pub known_lang: Option<String>,
}

/// A live card's scheduling facts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WordCardRow {
    /// Note.
    pub note_id: NoteId,
    /// Card.
    pub card_id: CardId,
    /// State.
    pub state: CardState,
    /// Due.
    pub due: Option<Timestamp>,
    /// Last review.
    pub last_review: Option<Timestamp>,
}

/// A review of a card of an indexed note (`words.history`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WordReviewEvent {
    /// Index language.
    pub language: String,
    /// Word key.
    pub key: String,
    /// Card.
    pub card_id: CardId,
    /// When.
    pub at: Timestamp,
    /// The review left the card in state `review` with a scheduled interval ≥ the threshold.
    pub known: bool,
}

/// The dirty queues: notes, then `(language, key)` pairs.
pub type WordIndexDirty = (Vec<NoteId>, Vec<(String, String)>);

/// Every word row and every note row.
pub type WordIndexDump = (Vec<WordIndexRecord>, Vec<WordIndexNoteRecord>);

/// Reads of the word index.
pub trait WordIndexRead {
    /// Rows of `keys` in `language` (missing keys are simply absent).
    fn word_index_get(&self, language: &str, keys: &[String]) -> StorageResult<Vec<WordIndexRecord>>;
    /// Rows by `(language, key)` with the query's filters.
    fn word_index_page(&self, q: &WordIndexQuery) -> StorageResult<Vec<WordIndexRecord>>;
    /// `(language, status, words)` with `threshold_days` (see [`WordIndexQuery`]).
    fn word_index_counts(&self, threshold_days: u32) -> StorageResult<Vec<(String, String, u32)>>;
    /// Notes with a term but no known language.
    fn word_index_unassigned(&self) -> StorageResult<u32>;
    /// Note rows of `keys` in `language`.
    fn word_index_notes_of(
        &self,
        language: &str,
        keys: &[String],
    ) -> StorageResult<Vec<WordIndexNoteRecord>>;
    /// Note rows of notes.
    fn word_index_notes_by_id(&self, notes: &[NoteId]) -> StorageResult<Vec<WordIndexNoteRecord>>;
    /// Every row of both tables, ordered (equality checks and diagnostics).
    fn word_index_dump(&self) -> StorageResult<WordIndexDump>;
    /// Live notes among `notes`, with what the index reads of them.
    fn word_note_sources(&self, notes: &[NoteId]) -> StorageResult<Vec<WordNoteSource>>;
    /// Live cards of `notes`.
    fn word_card_rows(&self, notes: &[NoteId]) -> StorageResult<Vec<WordCardRow>>;
    /// `word_marks` rows of `keys` in `language`.
    fn word_marks_of(&self, language: &str, keys: &[String]) -> StorageResult<Vec<WordMarkRecord>>;
    /// Notes and keys waiting in the dirty queues.
    fn word_index_dirty_count(&self) -> StorageResult<u32>;
    /// `(language, notes, words)` of every known language (not `""`), by language.
    fn word_index_languages(&self) -> StorageResult<Vec<(String, u32, u32)>>;
    /// Rows with a manual mark (of `language`, or all).
    fn word_index_marked(&self, language: Option<&str>) -> StorageResult<Vec<WordIndexRecord>>;
    /// Reviews of the live cards of indexed notes with a known language (of `language`, or
    /// all), in time order.
    fn word_review_events(
        &self,
        language: Option<&str>,
        threshold_days: u32,
    ) -> StorageResult<Vec<WordReviewEvent>>;
}

/// Writes of the word index.
pub trait WordIndexWrite {
    /// Takes (and empties) the dirty queues: notes, then `(language, key)` pairs.
    fn take_word_index_dirty(&mut self) -> StorageResult<WordIndexDirty>;
    /// Empties the index and queues every note and mark (a full rebuild on the next drain).
    fn reset_word_index(&mut self) -> StorageResult<()>;
    /// Inserts or replaces a note row.
    fn put_word_index_note(&mut self, r: &WordIndexNoteRecord) -> StorageResult<()>;
    /// Deletes a note row.
    fn delete_word_index_note(&mut self, note: NoteId) -> StorageResult<()>;
    /// Inserts or replaces a word row.
    fn put_word_index_entry(&mut self, r: &WordIndexRecord) -> StorageResult<()>;
    /// Deletes a word row.
    fn delete_word_index_entry(&mut self, language: &str, key: &str) -> StorageResult<()>;
    /// Records the language a producer knew for a note.
    fn set_word_note_language(&mut self, note: NoteId, language: &str) -> StorageResult<()>;
}
