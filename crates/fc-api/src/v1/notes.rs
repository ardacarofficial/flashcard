//! Note DTOs.

use super::attributes::AttributeAssignment;
use fc_core::card::CardState;
use fc_core::paging::PageRequest;
use fc_core::tag::TagName;
use fc_core::{CardId, DeckId, NoteId, NoteTypeId, Timestamp};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

/// Summary of a card generated from a note.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct NoteCardDto {
    /// Card id.
    pub id: CardId,
    /// Deck of the card.
    pub deck_id: DeckId,
    /// Template key.
    pub template_key: String,
    /// Learning state.
    pub state: CardState,
    /// Due time (`null` for new cards).
    pub due: Option<Timestamp>,
    /// Suspended.
    pub suspended: bool,
}

/// A note with everything the editor needs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct NoteDto {
    /// Id.
    pub id: NoteId,
    /// Note type.
    pub notetype_id: NoteTypeId,
    /// Field values keyed by field key.
    pub fields: BTreeMap<String, String>,
    /// Tags.
    pub tags: Vec<TagName>,
    /// Attribute values on the note.
    pub attributes: Vec<AttributeAssignment>,
    /// Cards generated from the note.
    pub cards: Vec<NoteCardDto>,
    /// Stripped sort-field text.
    pub sort_field: String,
    /// Creation time.
    pub created_at: Timestamp,
    /// Last modification time.
    pub updated_at: Timestamp,
}

/// `notes.create`: creates a note and its cards in `deck_id`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct CreateNote {
    /// Note type.
    pub notetype_id: NoteTypeId,
    /// Deck for the generated cards.
    pub deck_id: DeckId,
    /// Field values keyed by field key (missing = default/empty).
    pub fields: BTreeMap<String, String>,
    /// Tags (normalized server-side).
    #[serde(default)]
    pub tags: Vec<String>,
    /// Attribute values; omitted attributes get their default.
    #[serde(default)]
    pub attributes: Vec<AttributeAssignment>,
    /// Create even if a duplicate exists (default `false` → `CONFLICT_NOTE_DUPLICATE`).
    pub allow_duplicate: Option<bool>,
}

/// `notes.update`: replaces fields, tags and attributes of a note (full replace).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct UpdateNote {
    /// Note id.
    pub id: NoteId,
    /// Field values.
    pub fields: BTreeMap<String, String>,
    /// Tags.
    pub tags: Vec<String>,
    /// Attribute values (assignments with `value: null` clear).
    pub attributes: Vec<AttributeAssignment>,
}

/// `notes.get`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct NoteRef {
    /// Note id.
    pub id: NoteId,
}

/// `notes.delete` / `notes.restore`: soft-deletes (trash) or restores notes with their cards.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct NoteIds {
    /// Note ids.
    pub ids: Vec<NoteId>,
}

/// `notes.list`: notes having at least one card in the deck.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct ListNotes {
    /// Deck filter (`null` = all decks).
    pub deck_id: Option<DeckId>,
    /// Include notes of sub-decks (default `true`).
    pub include_subdecks: Option<bool>,
    /// Paging.
    #[serde(default)]
    pub page: PageRequest,
}

/// A row in the note list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct NoteListItem {
    /// Note id.
    pub id: NoteId,
    /// Note type.
    pub notetype_id: NoteTypeId,
    /// Stripped sort-field text.
    pub sort_field: String,
    /// Tags.
    pub tags: Vec<TagName>,
    /// Number of cards.
    pub card_count: u32,
    /// Last modification time.
    pub updated_at: Timestamp,
}

/// Response of `notes.list`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct NotePage {
    /// Rows, newest first.
    pub items: Vec<NoteListItem>,
    /// Cursor for the next page (`null` = end).
    pub next_cursor: Option<String>,
}
