//! Tag DTOs.

use fc_core::tag::TagName;
use fc_core::TagId;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A tag.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct TagDto {
    /// Id.
    pub id: TagId,
    /// Normalized hierarchical name `a::b::c`.
    pub name: TagName,
    /// Color.
    pub color: Option<String>,
    /// Number of (non-deleted) notes carrying exactly this tag.
    pub note_count: u32,
}

/// Response of `tags.list` (sorted by name).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct TagList {
    /// Tags.
    pub tags: Vec<TagDto>,
}

/// `tags.rename`: renames a tag **and its children** (`a::b` → `x` turns `a::b::c` into
/// `x::c`) on all notes. Renaming onto an existing tag merges.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct RenameTag {
    /// Existing name.
    pub from: String,
    /// New name.
    pub to: String,
}

/// `tags.merge`: replaces every `sources` tag with `into` on all notes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct MergeTags {
    /// Tags to remove.
    pub sources: Vec<String>,
    /// Tag to keep (created if missing).
    pub into: String,
}

/// `tags.set_color`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SetTagColor {
    /// Tag name.
    pub name: String,
    /// Color (`null` clears).
    pub color: Option<String>,
}

/// Result of tag rename/merge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TagChangeResult {
    /// Notes whose tag list changed.
    pub notes_affected: u32,
}
