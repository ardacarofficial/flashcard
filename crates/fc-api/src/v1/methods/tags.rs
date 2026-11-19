//! Tags.

use crate::v1::common::*;
use crate::v1::tags::*;

methods! {
    // ---- tags ----
    /// All tags.
    TagsList = "tags.list", Query, "tags", "tags:read", Public, undo: false, Empty => TagList;
    /// Renames a tag subtree on all notes.
    TagsRename = "tags.rename", Command, "tags", "tags:write", Public, undo: true, RenameTag => TagChangeResult;
    /// Merges tags.
    TagsMerge = "tags.merge", Command, "tags", "tags:write", Public, undo: true, MergeTags => TagChangeResult;
    /// Sets a tag color.
    TagsSetColor = "tags.set_color", Command, "tags", "tags:write", Public, undo: true, SetTagColor => TagDto;
}
