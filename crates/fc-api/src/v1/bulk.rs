//! Bulk operations on a selection (docs/02 §10). Each is **one command**: one transaction, one
//! oplog entry, one undo step. Their inverses are the internal `bulk.restore_*` methods, which
//! write back exact snapshots.

use fc_core::attribute::{AttributeValue, EntityRef};
use fc_core::card::CardState;
use fc_core::{AttributeDefId, CardId, DeckId, NoteId, Timestamp};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

/// Selected cards.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct CardSelection {
    /// Cards.
    pub card_ids: Vec<CardId>,
}

/// `bulk.move_cards`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct MoveCards {
    /// Cards.
    pub card_ids: Vec<CardId>,
    /// Target deck (live).
    pub deck_id: DeckId,
}

/// `bulk.set_suspended`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SetSuspended {
    /// Cards.
    pub card_ids: Vec<CardId>,
    /// Suspend (`true`) or unsuspend.
    pub suspended: bool,
}

/// `bulk.set_buried`: bury until the next UTC midnight, or unbury.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SetBuried {
    /// Cards.
    pub card_ids: Vec<CardId>,
    /// Bury (`true`) or unbury.
    pub buried: bool,
}

/// `bulk.set_flag`: sets/clears flag `flag` (1–7); `flag = 0` clears every flag.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SetFlag {
    /// Cards.
    pub card_ids: Vec<CardId>,
    /// Flag number (0 = all).
    pub flag: u32,
    /// Set (`true`) or clear.
    pub on: bool,
}

/// `bulk.add_tags` / `bulk.remove_tags`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct NoteTags {
    /// Notes.
    pub note_ids: Vec<NoteId>,
    /// Tags (normalized server-side).
    pub tags: Vec<String>,
}

/// `bulk.find_replace`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct FindReplace {
    /// Notes.
    pub note_ids: Vec<NoteId>,
    /// Text (or regex) to find.
    pub find: String,
    /// Replacement (`$1` groups with `regex`).
    pub replace: String,
    /// Only this field key (`null` = every field).
    pub field: Option<String>,
    /// `find` is a regex (default `false`).
    pub regex: Option<bool>,
    /// Case-sensitive (default `false`).
    pub match_case: Option<bool>,
}

/// `bulk.set_attribute`: sets (or clears with `value: null`) one classification on the
/// selection. Note-level classifications apply to the cards' notes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SetAttributeBulk {
    /// Cards (their notes for note-level classifications).
    pub card_ids: Vec<CardId>,
    /// Classification.
    pub attr_id: AttributeDefId,
    /// Value (`null` = clear).
    pub value: Option<AttributeValue>,
}

/// Result of a bulk operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
pub struct BulkResult {
    /// Entities changed.
    pub changed: u32,
    /// Entities left alone (already in the target state, trashed, or not applicable).
    pub skipped: u32,
}

/// Every mutable column of a card, as written back by `bulk.restore_cards`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct CardSnapshot {
    /// Card.
    pub id: CardId,
    /// Deck.
    pub deck_id: DeckId,
    /// State.
    pub state: CardState,
    /// Due.
    pub due: Option<Timestamp>,
    /// Stability.
    pub stability: Option<f64>,
    /// Difficulty.
    pub difficulty: Option<f64>,
    /// Reps.
    pub reps: u32,
    /// Lapses.
    pub lapses: u32,
    /// Last review.
    pub last_review: Option<Timestamp>,
    /// Flags.
    pub flags: u32,
    /// Suspended.
    pub suspended: bool,
    /// Buried until.
    pub buried_until: Option<Timestamp>,
    /// Trashed at.
    pub deleted_at: Option<Timestamp>,
}

/// `bulk.restore_cards` (internal).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct RestoreCards {
    /// Snapshots.
    pub cards: Vec<CardSnapshot>,
}

/// Content of a note, as written back by `bulk.restore_notes`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct NoteSnapshot {
    /// Note.
    pub id: NoteId,
    /// Fields.
    pub fields: BTreeMap<String, String>,
    /// Tags.
    pub tags: Vec<String>,
}

/// `bulk.restore_notes` (internal).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct RestoreNotes {
    /// Snapshots.
    pub notes: Vec<NoteSnapshot>,
}

/// One entity's value of a classification (`null` = none).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct EntityValue {
    /// Entity.
    pub entity: EntityRef,
    /// Value.
    pub value: Option<AttributeValue>,
}

/// `bulk.restore_values` (internal).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct RestoreValues {
    /// Classification.
    pub attr_id: AttributeDefId,
    /// Values to write back.
    pub values: Vec<EntityValue>,
}
