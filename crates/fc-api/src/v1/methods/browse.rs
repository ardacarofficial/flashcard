//! Card info, duplicates, trash, database check, command palette and shortcuts.

use crate::v1::browse::*;
use crate::v1::common::*;

methods! {
    // ---- browse ----
    /// Card details and review history.
    BrowseCardInfo = "browse.card_info", Query, "browse", "cards:read", Public, undo: false, CardInfoRef => CardInfo;
    /// Groups of duplicate notes.
    BrowseDuplicates = "browse.duplicates", Query, "browse", "notes:read", Public, undo: false, Empty => DuplicateList;
    /// Trashed decks and notes.
    BrowseTrash = "browse.trash", Query, "browse", "notes:read", Public, undo: false, ListTrash => TrashList;
    /// Database check (report only).
    BrowseCheckDatabase = "browse.check_database", Query, "browse", "notes:read", Public, undo: false, Empty => DatabaseReport;
    /// Repairs what the check found (undoable; re-indexing needs no undo).
    BrowseRepairDatabase = "browse.repair_database", Command, "browse", "notes:write", Public, undo: true, Empty => RepairResult;

    // ---- palette & shortcuts ----
    /// Every public method for the command palette.
    PaletteListMethod = "palette.list", Query, "palette", "modules:read", Public, undo: false, Empty => PaletteList;
    /// Shortcut bindings and conflicts.
    ShortcutsList = "shortcuts.list", Query, "palette", "settings:read", Public, undo: false, Empty => ShortcutList;
    /// Binds / unbinds an action.
    ShortcutsSet = "shortcuts.set", Command, "palette", "settings:write", Public, undo: true, SetShortcut => ShortcutList;
    /// Restores defaults.
    ShortcutsReset = "shortcuts.reset", Command, "palette", "settings:write", Public, undo: true, ResetShortcut => ShortcutList;
    /// Internal inverse of set/reset.
    ShortcutsRestore = "shortcuts.restore", Command, "palette", "settings:write", Internal, undo: true, RestoreShortcuts => ShortcutList;
}
