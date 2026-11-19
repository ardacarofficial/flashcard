//! Tags.

use super::*;

/// Result of [`TagWrite::ensure_tags`] for one requested name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnsuredTag {
    /// Id of the tag now carrying the name (the existing one, or the caller's id).
    pub id: TagId,
    /// Whether the tag was inserted by this call (with the caller's id).
    pub created: bool,
}

/// Tag reads.
pub trait TagRead {
    /// All tags with the number of live notes carrying each.
    fn list_tags(&self) -> StorageResult<Vec<(Tag, u32)>>;
    /// Tag by name (case-insensitive).
    fn get_tag_by_name(&self, name: &TagName) -> StorageResult<Option<Tag>>;
}

/// Tag writes. Tag assignment happens through [`NoteWrite`].
pub trait TagWrite: TagRead {
    /// Inserts or replaces a tag row.
    fn upsert_tag(&mut self, tag: &Tag) -> StorageResult<()>;
    /// For each `(name, id)`: returns the existing tag with that name (Unicode
    /// case-insensitive), or inserts `name` with the caller's `id` (created/updated at `now`).
    /// Results are in input order; a name repeated in the input resolves to the same tag.
    /// Storage never mints ids (all come from the app's `IdGen`).
    fn ensure_tags(
        &mut self,
        tags: &[(TagName, TagId)],
        now: Timestamp,
    ) -> StorageResult<Vec<EnsuredTag>>;
    /// Removes tags no note references (also not trashed notes). Returns rows deleted.
    fn delete_unused_tags(&mut self) -> StorageResult<u32>;
    /// Moves every `note_tags` reference from tag `from` to tag `into` (a note carrying both
    /// keeps `into` at its own position), then deletes `from`. Sets `updated_at = now` on
    /// every note that carried `from` and returns those notes (sorted by id; trashed notes
    /// included). Hierarchical renames are `upsert_tag` (same id, new name) per tag, or this
    /// when the new name exists.
    fn merge_tag(&mut self, from: TagId, into: TagId, now: Timestamp)
        -> StorageResult<Vec<NoteId>>;
}
