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
    /// Number of notes referencing it (live or trashed).
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
}

/// Content writes.
pub trait ContentWrite: ContentRead {
    /// Inserts a media row (no-op if the hash exists).
    fn insert_media(&mut self, media: &MediaRecord) -> StorageResult<()>;
    /// Stores the file of `hash` (content-addressed: an existing file is left as is).
    fn write_media_file(&mut self, hash: &str, bytes: &[u8]) -> StorageResult<()>;
    /// Deletes a media row that no note references (the file stays until a database check
    /// removes orphans). Returns whether a row was deleted.
    fn delete_unreferenced_media(&mut self, hash: &str) -> StorageResult<bool>;
    /// Replaces a note's references and refreshes `media.ref_count` of every affected hash.
    fn set_note_refs(&mut self, note: NoteId, refs: &NoteRefs) -> StorageResult<()>;
    /// Replaces one flag definition.
    fn set_flag_def(&mut self, def: &FlagDef) -> StorageResult<()>;
}
