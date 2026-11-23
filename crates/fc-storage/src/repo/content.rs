//! Card content (v0.3): media rows and files, per-note media/link references, flag names,
//! note-type membership.

use super::*;

/// A media row (`media` table). The file lives at `media/<ab>/<hash>` next to the database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaRecord {
    /// SHA-256 hex of the content.
    pub hash: String,
    /// MIME type.
    pub mime: String,
    /// Size in bytes.
    pub size: u64,
    /// File name it was first added with.
    pub original_name: Option<String>,
    /// Number of notes referencing it (live or trashed). Computed on read, so it stays right
    /// when notes are purged (cascade); ignored on insert.
    pub ref_count: u32,
    /// First added.
    pub created_at: Timestamp,
}

/// A card flag (bit `flag - 1` of `cards.flags`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlagDef {
    /// 1–7.
    pub flag: u8,
    /// Custom name (`None` = the localized default).
    pub name: Option<String>,
    /// Color token.
    pub color: String,
    /// Last change.
    pub updated_at: Timestamp,
}

/// References a note's fields make, written together on every note write.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NoteRefs {
    /// The note's own link key (normalized sort field).
    pub key: String,
    /// `[[links]]`: `(target key, label)`.
    pub links: Vec<(String, String)>,
    /// Referenced media hashes (only ones with a `media` row).
    pub media: Vec<String>,
}

/// Content reads.
pub trait ContentRead {
    /// Media row by hash.
    fn get_media(&self, hash: &str) -> StorageResult<Option<MediaRecord>>;
    /// All media rows, newest first.
    fn list_media(&self) -> StorageResult<Vec<MediaRecord>>;
    /// The file content of `hash`, if stored.
    fn read_media_file(&self, hash: &str) -> StorageResult<Option<Vec<u8>>>;
    /// Media hashes referenced by a note (sorted).
    fn note_media(&self, note: NoteId) -> StorageResult<Vec<String>>;
    /// Live notes whose own key is one of `keys`: `(key, note)`, oldest note first per key.
    fn notes_by_link_key(&self, keys: &[String]) -> StorageResult<Vec<(String, NoteId)>>;
    /// Live notes linking to `key` (sorted by id).
    fn notes_linking_to(&self, key: &str) -> StorageResult<Vec<NoteId>>;
    /// Live notes whose key starts with `prefix`: `(note, sort field)`, by key, at most
    /// `limit`.
    fn notes_with_key_prefix(
        &self,
        prefix: &str,
        limit: u32,
    ) -> StorageResult<Vec<(NoteId, String)>>;
    /// Notes of a note type (sorted by id).
    fn note_ids_of_notetype(
        &self,
        notetype: NoteTypeId,
        include_deleted: bool,
    ) -> StorageResult<Vec<NoteId>>;
    /// The seven flags, by number.
    fn list_flag_defs(&self) -> StorageResult<Vec<FlagDef>>;
    /// Values of `field` of at most `limit` live notes of `notetype` with a live card in
    /// `deck`, excluding `exclude`. Notes are picked in a pseudo-random order fixed by `seed`
    /// (same seed + same data = same result); empty and missing values are skipped.
    fn sample_field_values(
        &self,
        sample: &FieldSample<'_>,
    ) -> StorageResult<Vec<String>>;
}

/// Parameters of [`ContentRead::sample_field_values`].
#[derive(Debug, Clone, Copy)]
pub struct FieldSample<'a> {
    /// Deck whose cards' notes are sampled.
    pub deck: DeckId,
    /// Note type of the sampled notes.
    pub notetype: NoteTypeId,
    /// Note left out (the one being rendered).
    pub exclude: NoteId,
    /// Field key.
    pub field: &'a str,
    /// Order seed.
    pub seed: u64,
    /// Maximum number of values.
    pub limit: u32,
}

/// Content writes.
pub trait ContentWrite: ContentRead {
    /// Inserts a media row (no-op if the hash exists).
    fn insert_media(&mut self, media: &MediaRecord) -> StorageResult<()>;
    /// Stores the file of `hash` (content-addressed: an existing file is left as is).
    fn write_media_file(&mut self, hash: &str, bytes: &[u8]) -> StorageResult<()>;
    /// Deletes a media row that no note references (the file stays, so redoing an undone
    /// `media.add` still works, until [`ContentWrite::delete_orphan_media_files`] runs).
    /// Returns whether a row was deleted.
    fn delete_unreferenced_media(&mut self, hash: &str) -> StorageResult<bool>;
    /// v1.4.1: deletes the media rows created before `created_before` that nothing uses any
    /// more (an undone import's files after its trashed notes were purged, an undone
    /// `media.add`): no note references it (trashed notes included), no Subtitle upload or
    /// capture, no note type's templates or CSS, no inbox item, draft or reading item, it is
    /// no TTS/clip/Subtitle original (`media_origins`), and **no oplog entry that undo or redo
    /// can still reach** (entries after the newest history barrier) names it. Returns the
    /// hashes; their files go with [`ContentWrite::delete_orphan_media_files`].
    fn delete_unused_media(&mut self, created_before: Timestamp) -> StorageResult<Vec<String>>;
    /// Removes stored files that have no `media` row (and interrupted partial writes).
    /// Returns how many files were removed.
    fn delete_orphan_media_files(&mut self) -> StorageResult<u64>;
    /// Replaces a note's references.
    fn set_note_refs(&mut self, note: NoteId, refs: &NoteRefs) -> StorageResult<()>;
    /// Replaces one flag definition.
    fn set_flag_def(&mut self, def: &FlagDef) -> StorageResult<()>;
}
