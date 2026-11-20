//! Card info, duplicates, trash, database check (docs/02 §10–11) and the command palette /
//! keyboard shortcuts (docs/02 §13).

use super::notes::NoteDto;
use super::search::CardRow;
use crate::method::MethodKind;
use fc_core::{CardId, DeckId, NoteId, Timestamp};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

/// `browse.card_info`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct CardInfoRef {
    /// Card.
    pub card_id: CardId,
}

/// One review of a card.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ReviewDto {
    /// When.
    pub at: Timestamp,
    /// `again` / `hard` / `good` / `easy`.
    pub rating: String,
    /// State before.
    pub state_before: String,
    /// State after.
    pub state_after: String,
    /// Scheduled interval (days).
    pub scheduled_days: i64,
    /// Days since the previous review.
    pub elapsed_days: i64,
    /// Time on the card (ms).
    pub duration_ms: i64,
    /// Hints used.
    pub hints_used: i64,
    /// Session mode.
    pub mode: String,
    /// Stability after.
    pub stability: Option<f64>,
    /// Difficulty after.
    pub difficulty: Option<f64>,
}

/// Response of `browse.card_info`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct CardInfo {
    /// The card as a browser row.
    pub card: CardRow,
    /// Its note (fields, tags, classifications).
    pub note: NoteDto,
    /// Last review.
    pub last_review: Option<Timestamp>,
    /// Probability of recall now (FSRS forgetting curve; `null` without data).
    pub retrievability: Option<f64>,
    /// Review history, oldest first.
    pub reviews: Vec<ReviewDto>,
}

/// One group of duplicate notes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DuplicateGroup {
    /// Sort field of the first note.
    pub sort_field: String,
    /// Notes (oldest id first).
    pub note_ids: Vec<NoteId>,
}

/// Response of `browse.duplicates`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DuplicateList {
    /// Groups of ≥ 2 notes with the same note type and normalized unique fields.
    pub groups: Vec<DuplicateGroup>,
}

/// `browse.trash`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct ListTrash {
    /// Maximum notes (default 200).
    pub limit: Option<u32>,
}

/// A trashed note.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TrashedNote {
    /// Note.
    pub id: NoteId,
    /// Sort field.
    pub sort_field: String,
    /// When it was trashed.
    pub deleted_at: Timestamp,
}

/// A trashed deck (only roots of trashed subtrees).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TrashedDeck {
    /// Deck.
    pub id: DeckId,
    /// Full path.
    pub path: String,
    /// When it was trashed.
    pub deleted_at: Timestamp,
}

/// Response of `browse.trash`. Restore with `notes.restore` / `decks.restore`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TrashList {
    /// Trashed decks, newest first.
    pub decks: Vec<TrashedDeck>,
    /// Trashed notes (outside trashed decks too), newest first.
    pub notes: Vec<TrashedNote>,
}

/// Response of `browse.check_database`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
pub struct DatabaseReport {
    /// Live cards whose note is trashed or missing (repair: trash them).
    pub orphan_cards: Vec<CardId>,
    /// Live cards in a trashed deck (repair: trash them).
    pub cards_in_trashed_decks: Vec<CardId>,
    /// Scheduled cards without a due time (repair: due now).
    pub unscheduled_cards: Vec<CardId>,
    /// Notes missing from the search index (repair: re-index).
    pub missing_search_index: Vec<NoteId>,
    /// Notes without any card (report only).
    pub cardless_notes: Vec<NoteId>,
    /// Classification values of deleted entities (repair: remove them; they are invisible, so
    /// there is nothing observable to undo).
    pub dangling_values: u32,
    /// Notes lacking a field their note type defines (repair: add it empty).
    pub broken_fields: Vec<NoteId>,
    /// Nothing to repair.
    pub healthy: bool,
}

/// Response of `browse.repair_database`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
pub struct RepairResult {
    /// Cards moved to the trash.
    pub trashed_cards: u32,
    /// Cards given a due time.
    pub rescheduled_cards: u32,
    /// Notes re-indexed.
    pub reindexed_notes: u32,
    /// Notes given their missing fields.
    pub fixed_notes: u32,
    /// Dangling classification values removed.
    pub removed_values: u32,
}

/// `browse.purge_trash`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct PurgeTrash {
    /// Purge what was trashed more than this many days ago (`null` = the
    /// `browse.trash_retention_days` setting; `0` = everything, "empty the trash").
    pub older_than_days: Option<u32>,
}

/// Response of `browse.purge_trash`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PurgeResult {
    /// Notes deleted for good (with their cards).
    pub notes: u32,
    /// Cards trashed on their own, deleted for good.
    pub cards: u32,
    /// Decks deleted for good.
    pub decks: u32,
    /// Everything trashed before this moment was eligible.
    pub before: Timestamp,
}

/// Where a shortcut is active.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ShortcutContext {
    /// Everywhere (conflicts with every context).
    Global,
    /// Card browser.
    Browse,
    /// Study screen.
    Study,
}

/// One bindable action and its effective key combination.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ShortcutDto {
    /// Action id: a UI action (`ui.palette`) or a qualified method (`v1.oplog.undo`).
    pub action: String,
    /// i18n key of the label.
    pub label_key: String,
    /// Context.
    pub context: ShortcutContext,
    /// Effective combination (`Ctrl+Shift+Z`), `null` = unbound.
    pub combo: Option<String>,
    /// Built-in default.
    pub default_combo: Option<String>,
}

/// A conflict: several actions of overlapping contexts share a combination.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ShortcutConflict {
    /// Combination.
    pub combo: String,
    /// Actions.
    pub actions: Vec<String>,
}

/// Response of `shortcuts.list` / `shortcuts.set` / `shortcuts.reset`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ShortcutList {
    /// Every bindable action (UI actions first, then public commands by name).
    pub items: Vec<ShortcutDto>,
    /// Conflicts (empty when the user resolved them all).
    pub conflicts: Vec<ShortcutConflict>,
}

/// `shortcuts.set`: binds (or unbinds with `combo: null`) an action. A combination already
/// used by an action of an overlapping context is rejected (`CONFLICT_SHORTCUT`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SetShortcut {
    /// Action id.
    pub action: String,
    /// Combination (`ctrl+k`, normalized server-side) or `null`.
    pub combo: Option<String>,
}

/// `shortcuts.reset`: restores the default of one action (`action`) or of all (`null`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct ResetShortcut {
    /// Action id, `null` = every action.
    pub action: Option<String>,
}

/// `shortcuts.restore` (internal inverse): the whole override map as it was.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct RestoreShortcuts {
    /// `action → combo` (`null` = explicitly unbound); `null` = no overrides.
    pub overrides: Option<BTreeMap<String, Option<String>>>,
}

/// One command palette entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PaletteEntry {
    /// Qualified method name (`v1.decks.create`).
    pub method: String,
    /// Command or query.
    pub kind: MethodKind,
    /// Owning module.
    pub module: String,
    /// i18n key of the label (`commands.<method>`; the UI falls back to the name).
    pub label_key: String,
    /// The method takes no parameters (runs directly).
    pub direct: bool,
    /// Its module is enabled.
    pub enabled: bool,
    /// Bound shortcut.
    pub shortcut: Option<String>,
}

/// Response of `palette.list`: **every public method of the registry**.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PaletteList {
    /// Entries sorted by method name.
    pub entries: Vec<PaletteEntry>,
}
