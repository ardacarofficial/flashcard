//! Reading mode (v0.10, migration `0900_reading`): reading items (texts, subtitles,
//! extracts; the incremental-reading queue), word marks and the vocabulary the word colours
//! are computed from.

use super::*;
use fc_core::card::CardState;
use fc_core::ReadingItemId;

/// One `reading_items` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReadingItemRecord {
    /// Id.
    pub id: ReadingItemId,
    /// `text`, `subtitle` or `extract`.
    pub kind: String,
    /// Extracts: the item it was taken from (`None` once that is purged).
    pub parent_id: Option<ReadingItemId>,
    /// Title.
    pub title: String,
    /// BCP-47 language.
    pub language: String,
    /// URL or file name.
    pub source: Option<String>,
    /// Plain text. **Empty in listings** ([`ReadingRead::list_reading_items`]).
    pub body: String,
    /// Body length in UTF-16 units (kept by the app).
    pub length: i64,
    /// Subtitle cues (JSON array). `None` in listings.
    pub cues: Option<String>,
    /// Number of cues (filled on read, ignored on write).
    pub cue_count: i64,
    /// Local video/audio next to a subtitle.
    pub media_path: Option<String>,
    /// Extracts: UTF-16 range in the parent body.
    pub extract_range: Option<(i64, i64)>,
    /// UTF-16 offset read up to.
    pub position: i64,
    /// In the reading queue.
    pub queued: bool,
    /// `high` / `normal` / `low`.
    pub priority: String,
    /// When it comes back (queued items).
    pub due: Option<Timestamp>,
    /// Last queue interval, days.
    pub interval_days: i64,
    /// Reading sessions done.
    pub read_count: i64,
    /// Last reading session.
    pub last_read: Option<Timestamp>,
    /// Created.
    pub created_at: Timestamp,
    /// Updated.
    pub updated_at: Timestamp,
    /// Trashed.
    pub deleted_at: Option<Timestamp>,
}

/// A user's mark on a word without cards.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WordMarkRecord {
    /// Primary language subtag (`en`).
    pub language: String,
    /// Lemma.
    pub lemma: String,
    /// `known` / `ignored`.
    pub status: String,
    /// When it was set.
    pub updated_at: Timestamp,
}

/// One live card of a live note, with the text its word is read from.
#[derive(Debug, Clone, PartialEq)]
pub struct VocabularyRow {
    /// Card.
    pub card_id: CardId,
    /// Note.
    pub note_id: NoteId,
    /// The note's `word` field if its note type has one, else its sort field (raw HTML).
    pub word: String,
    /// Learning state.
    pub state: CardState,
    /// Due.
    pub due: Option<Timestamp>,
    /// Last review.
    pub last_review: Option<Timestamp>,
}

/// Reading reads.
pub trait ReadingRead {
    /// Items by creation (newest first), bodies and cues not loaded.
    fn list_reading_items(&self, include_deleted: bool) -> StorageResult<Vec<ReadingItemRecord>>;
    /// One item with body and cues (also if trashed).
    fn get_reading_item(&self, id: ReadingItemId) -> StorageResult<Option<ReadingItemRecord>>;
    /// Marks of a language, by lemma.
    fn list_word_marks(&self, language: &str) -> StorageResult<Vec<WordMarkRecord>>;
    /// Every live card of a live note with its word text (creation order).
    fn vocabulary_rows(&self) -> StorageResult<Vec<VocabularyRow>>;
}

/// Reading writes.
pub trait ReadingWrite: ReadingRead {
    /// Inserts or replaces an item (all columns).
    fn upsert_reading_item(&mut self, item: &ReadingItemRecord) -> StorageResult<()>;
    /// Trash / restore. Returns rows changed.
    fn set_reading_items_deleted(
        &mut self,
        ids: &[ReadingItemId],
        deleted_at: Option<Timestamp>,
    ) -> StorageResult<u32>;
    /// Deletes items trashed before `before`; returns how many.
    fn purge_reading_items(&mut self, before: Timestamp) -> StorageResult<u32>;
    /// Sets (`Some(status)`) or clears (`None`) the mark of `(language, lemma)`.
    fn set_word_mark(
        &mut self,
        language: &str,
        lemma: &str,
        status: Option<&str>,
        now: Timestamp,
    ) -> StorageResult<()>;
}
