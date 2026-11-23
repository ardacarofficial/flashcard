//! Leech repairs (v1.4 W3, migration `1470_leech_repairs`): which repair the user applied to a
//! leech card, so the next real reviews can be compared with the ones before. User history:
//! written through undoable commands (the app logs the inverse), kept in backups.

use super::*;

/// One `leech_repairs` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeechRepairRecord {
    /// Row id (assigned by storage; ignored on insert unless > 0).
    pub id: i64,
    /// The repaired card.
    pub card_id: CardId,
    /// The card's note.
    pub note_id: NoteId,
    /// `switch_example`, `add_accepted_answers`, `move_meaning` or `ai_rewrite`.
    pub kind: String,
    /// The repair as applied (JSON text).
    pub detail: String,
    /// The card's lapses when it was applied.
    pub lapses: u32,
    /// When it was applied.
    pub applied_at: Timestamp,
}

/// Leech repair reads.
pub trait LeechRepairRead {
    /// Repairs of `card_ids` (`None` = all), oldest first (`applied_at`, then `id`).
    fn list_leech_repairs(&self, card_ids: Option<&[CardId]>)
        -> StorageResult<Vec<LeechRepairRecord>>;

    /// One repair by id.
    fn get_leech_repair(&self, id: i64) -> StorageResult<Option<LeechRepairRecord>>;
}

/// Leech repair writes.
pub trait LeechRepairWrite: LeechRepairRead {
    /// Inserts `rec` and returns its id. `rec.id` is ignored unless it is > 0: then that id is
    /// used (restoring a row after an undo); an id already taken is a constraint error.
    fn insert_leech_repair(&mut self, rec: &LeechRepairRecord) -> StorageResult<i64>;

    /// Deletes a repair (no-op when missing).
    fn delete_leech_repair(&mut self, id: i64) -> StorageResult<()>;
}
