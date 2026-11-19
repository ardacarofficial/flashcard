//! Notes.

use super::*;

/// Filter for note listing.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NoteFilter {
    /// Notes with at least one live card in any of these decks (`None` = all notes). The app
    /// expands sub-decks.
    pub deck_ids: Option<Vec<DeckId>>,
    /// Include trashed notes.
    pub include_deleted: bool,
}

/// A note row in a listing.
#[derive(Debug, Clone, PartialEq)]
pub struct NoteListRow {
    /// The note (with tags).
    pub note: Note,
    /// Number of live cards.
    pub card_count: u32,
}

/// Note reads. Returned notes always carry their tags (from `note_tags`, in stored order).
pub trait NoteRead {
    /// Note by id.
    fn get_note(&self, id: NoteId) -> StorageResult<Option<Note>>;
    /// Paged listing, newest `updated_at` first.
    fn list_notes(
        &self,
        filter: &NoteFilter,
        page: &PageRequest,
    ) -> StorageResult<Page<NoteListRow>>;
    /// Live notes of `notetype` with this checksum (duplicate detection).
    fn find_notes_by_checksum(
        &self,
        notetype: NoteTypeId,
        checksum: &str,
    ) -> StorageResult<Vec<NoteId>>;
    /// Live notes carrying `tag` or any tag below it.
    fn note_ids_with_tag(&self, tag: &TagName) -> StorageResult<Vec<NoteId>>;
    /// `notes.home_deck_id` of a note (also if trashed); `None` for an unknown note.
    fn note_home_deck(&self, id: NoteId) -> StorageResult<Option<DeckId>>;
}

/// Note writes. Storage never creates tags here: every name in `note.tags` must already exist
/// (see [`TagWrite::ensure_tags`]), else `NotFound { entity: "tag" }`.
pub trait NoteWrite: NoteRead {
    /// Inserts a note (homed in `note.home_deck_id`), its `note_tags` (in `note.tags` order)
    /// and its `notes_fts` row with `search_text`.
    fn insert_note(&mut self, note: &Note, search_text: &str) -> StorageResult<()>;
    /// Replaces a note's columns (home deck included), tags and FTS row.
    fn update_note(&mut self, note: &Note, search_text: &str) -> StorageResult<()>;
    /// Sets `notes.home_deck_id`. `NotFound` for an unknown note.
    fn set_note_home_deck(&mut self, id: NoteId, home_deck: DeckId) -> StorageResult<()>;
    /// Trash/restore. Returns rows changed.
    fn set_notes_deleted(
        &mut self,
        ids: &[NoteId],
        deleted_at: Option<Timestamp>,
    ) -> StorageResult<u32>;
}
