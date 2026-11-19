//! Note types.

use crate::v1::common::*;
use crate::v1::notetypes::*;

methods! {
    // ---- note types ----
    /// All note types.
    NotetypesList = "notetypes.list", Query, "notetypes", "notetypes:read", Public, undo: false, Empty => NoteTypeList;
    /// One note type.
    NotetypesGet = "notetypes.get", Query, "notetypes", "notetypes:read", Public, undo: false, NoteTypeRef => NoteTypeDto;
}
