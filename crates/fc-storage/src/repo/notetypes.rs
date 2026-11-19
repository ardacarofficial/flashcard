//! Note types.

use super::*;

/// Note type reads.
pub trait NoteTypeRead {
    /// Note type by id.
    fn get_notetype(&self, id: NoteTypeId) -> StorageResult<Option<NoteType>>;
    /// All note types.
    fn list_notetypes(&self, include_deleted: bool) -> StorageResult<Vec<NoteType>>;
}

/// Note type writes.
pub trait NoteTypeWrite: NoteTypeRead {
    /// Inserts or replaces (fields/templates stored as JSON).
    fn upsert_notetype(&mut self, nt: &NoteType) -> StorageResult<()>;
}
