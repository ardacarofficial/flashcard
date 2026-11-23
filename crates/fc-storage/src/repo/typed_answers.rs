//! Typed answers (v1.4 Z1, migration `1421_typed_answers`): what the user typed for a written
//! answer (Cards type-in, Learn's written stage, the Exam's written questions), next to the
//! review it was rated with. User history: written through undoable commands (the app logs
//! the inverse and keeps the newest rows per card), kept in backups, never logged.

use super::*;
use fc_core::ReviewLogId;

/// One `typed_answers` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedAnswerRecord {
    /// Row id (assigned by storage; ignored on insert unless > 0).
    pub id: i64,
    /// The answered card.
    pub card_id: CardId,
    /// The review the answer was rated with (`None` = no review was logged).
    pub review_log_id: Option<ReviewLogId>,
    /// The review's mode (`normal`, `learn`, `test`, …).
    pub mode: String,
    /// What the user typed.
    pub typed: String,
    /// Judged right (`None` = the user rated it himself).
    pub correct: Option<bool>,
    /// When.
    pub created_at: Timestamp,
}

/// Typed answer reads.
pub trait TypedAnswerRead {
    /// Typed answers of a card, newest first (`created_at`, then `id`).
    fn typed_answers_of_card(&self, card_id: CardId) -> StorageResult<Vec<TypedAnswerRecord>>;

    /// Typed answers of these reviews (any order).
    fn typed_answers_of_reviews(
        &self,
        review_ids: &[ReviewLogId],
    ) -> StorageResult<Vec<TypedAnswerRecord>>;

    /// One typed answer by id.
    fn get_typed_answer(&self, id: i64) -> StorageResult<Option<TypedAnswerRecord>>;
}

/// Typed answer writes.
pub trait TypedAnswerWrite: TypedAnswerRead {
    /// Inserts `rec` and returns its id. `rec.id` is ignored unless it is > 0: then that id is
    /// used (restoring a row after an undo); an id already taken is a constraint error.
    fn insert_typed_answer(&mut self, rec: &TypedAnswerRecord) -> StorageResult<i64>;

    /// Deletes a typed answer (no-op when missing).
    fn delete_typed_answer(&mut self, id: i64) -> StorageResult<()>;
}
