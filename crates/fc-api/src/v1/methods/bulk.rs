//! Bulk operations on a selection: one command, one transaction, one undo step each.

use crate::v1::bulk::*;

methods! {
    // ---- bulk ----
    /// Moves cards to a deck.
    BulkMoveCards = "bulk.move_cards", Command, "browse", "cards:write", Public, undo: true, MoveCards => BulkResult;
    /// Suspends / unsuspends cards.
    BulkSetSuspended = "bulk.set_suspended", Command, "browse", "cards:write", Public, undo: true, SetSuspended => BulkResult;
    /// Buries until tomorrow (UTC) / unburies cards.
    BulkSetBuried = "bulk.set_buried", Command, "browse", "cards:write", Public, undo: true, SetBuried => BulkResult;
    /// Sets / clears a flag.
    BulkSetFlag = "bulk.set_flag", Command, "browse", "cards:write", Public, undo: true, SetFlag => BulkResult;
    /// Forgets cards: back to new.
    BulkResetProgress = "bulk.reset_progress", Command, "browse", "cards:write", Public, undo: true, CardSelection => BulkResult;
    /// Adds tags to notes.
    BulkAddTags = "bulk.add_tags", Command, "browse", "notes:write", Public, undo: true, NoteTags => BulkResult;
    /// Removes tags (and their sub-tags) from notes.
    BulkRemoveTags = "bulk.remove_tags", Command, "browse", "notes:write", Public, undo: true, NoteTags => BulkResult;
    /// Find and replace in note fields.
    BulkFindReplace = "bulk.find_replace", Command, "browse", "notes:write", Public, undo: true, FindReplace => BulkResult;
    /// Sets / clears one classification on the selection.
    BulkSetAttribute = "bulk.set_attribute", Command, "browse", "attributes:write", Public, undo: true, SetAttributeBulk => BulkResult;
    /// Internal inverse: writes card snapshots back.
    BulkRestoreCards = "bulk.restore_cards", Command, "browse", "cards:write", Internal, undo: true, RestoreCards => BulkResult;
    /// Internal inverse: writes note contents back.
    BulkRestoreNotes = "bulk.restore_notes", Command, "browse", "notes:write", Internal, undo: true, RestoreNotes => BulkResult;
    /// Internal inverse: writes classification values back.
    BulkRestoreValues = "bulk.restore_values", Command, "browse", "attributes:write", Internal, undo: true, RestoreValues => BulkResult;
}
