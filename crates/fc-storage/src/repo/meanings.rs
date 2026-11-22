//! Meaning in context and accepted answers (v1.4 W2, migration `1450_meaning_in_context`):
//! a note's meaning in its sentence, stored apart from its fields, and the other answers the
//! offline written-answer check accepts. Storage keeps rows; what they mean is the app's.

use super::*;

/// One `note_meanings` row.
#[derive(Debug, Clone, PartialEq)]
pub struct NoteMeaningRecord {
    /// Note.
    pub note_id: NoteId,
    /// The note's meaning field.
    pub field_key: String,
    /// `suggested` or `applied`.
    pub status: String,
    /// The meaning.
    pub meaning: String,
    /// Phrase of the sentence it rests on.
    pub phrase: String,
    /// An expression / phrasal verb.
    pub expression: bool,
    /// Other correct translations.
    pub also_correct: Vec<String>,
    /// Meanings that do not fit the sentence.
    pub not_this: Vec<String>,
    /// Model confidence 0–1 (`None` = typed by the user).
    pub confidence: Option<f64>,
    /// The user changed the model's wording.
    pub edited: bool,
    /// The meaning field's value before the first apply.
    pub original: String,
    /// Model that answered.
    pub model: String,
    /// First stored.
    pub created_at: Timestamp,
    /// Last changed.
    pub updated_at: Timestamp,
}

/// One accepted answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptedAnswerRecord {
    /// Note.
    pub note_id: NoteId,
    /// Field key.
    pub field_key: String,
    /// The answer.
    pub answer: String,
    /// Order inside the note's field.
    pub position: i64,
}

/// Meaning reads.
pub trait MeaningRead {
    /// A note's stored meaning.
    fn get_note_meaning(&self, note: NoteId) -> StorageResult<Option<NoteMeaningRecord>>;
    /// A note's accepted answers, by field key then position.
    fn list_accepted_answers(&self, note: NoteId) -> StorageResult<Vec<AcceptedAnswerRecord>>;
}

/// Meaning writes.
pub trait MeaningWrite: MeaningRead {
    /// Inserts or replaces a note's meaning.
    fn put_note_meaning(&mut self, rec: &NoteMeaningRecord) -> StorageResult<()>;
    /// Removes a note's meaning (no row: nothing happens).
    fn delete_note_meaning(&mut self, note: NoteId) -> StorageResult<()>;
    /// Replaces every accepted answer of `note` with `rows`.
    fn set_accepted_answers(&mut self, note: NoteId, rows: &[AcceptedAnswerRecord]) -> StorageResult<()>;
}
