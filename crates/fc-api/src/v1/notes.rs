//! Note DTOs.

use super::attributes::AttributeAssignment;
use fc_core::card::CardState;
use fc_core::paging::PageRequest;
use fc_core::stats::Mastery;
use fc_core::tag::TagName;
use fc_core::{AttributeDefId, CardId, DeckId, NoteId, NoteTypeId, Timestamp};
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
    /// Home deck: where note-level classifications are resolved from (card-level ones come
    /// from each card's deck).
    pub home_deck_id: DeckId,
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
    /// Home deck of the note and deck of the generated cards.
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

/// `notes.update`: replaces fields and tags, sets the listed attributes, and optionally
/// changes the home deck (cards stay where they are).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct UpdateNote {
    /// Note id.
    pub id: NoteId,
    /// New home deck (`null` = unchanged).
    #[serde(default)]
    pub deck_id: Option<DeckId>,
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

/// One side of a term row: a note field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TermField {
    /// Field key (what `notes.update` takes).
    pub key: String,
    /// Stored value (may hold Markdown/HTML).
    pub value: String,
    /// Visible text of the value (tags stripped).
    pub text: String,
}

/// A classification value of a note (the level chip of a term row).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TermChip {
    /// Classification.
    pub attr_id: AttributeDefId,
    /// Classification name (user text).
    pub name: String,
    /// Value label (user text).
    pub label: String,
}

/// A row of the deck page's term list (v1.1): a note as term | definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct NoteTerm {
    /// Note.
    pub note_id: NoteId,
    /// Note type.
    pub notetype_id: NoteTypeId,
    /// Its live cards (flags apply to all of them).
    pub card_ids: Vec<CardId>,
    /// Flag bits set on every one of its cards.
    pub flags: u32,
    /// Least advanced mastery among its cards.
    pub mastery: Mastery,
    /// The term column: the field with the `term` role (v1.3), else the first field.
    pub front: TermField,
    /// The definition column: the field with the `meaning` role (v1.3), else the second field
    /// (`null` for a one-field note type).
    pub back: Option<TermField>,
    /// Key of the field shown (and edited) as the term (`front.key`).
    pub term_field: String,
    /// Key of the field shown (and edited) as the definition (`back.key`; `null` without one).
    pub definition_field: Option<String>,
    /// The note type has no `term` + `meaning` roles, so the two columns are only its first
    /// two fields: a guess. The UI opens the full note editor instead of editing inline.
    pub ambiguous: bool,
    /// Its choice classification values (in classification order).
    pub chips: Vec<TermChip>,
}

/// Response of `notes.terms`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TermPage {
    /// Rows, newest first (the order of `notes.list`).
    pub items: Vec<NoteTerm>,
    /// Cursor for the next page (`null` = end).
    pub next_cursor: Option<String>,
}

/// Where `notes.create_many` puts the notes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NotesTarget {
    /// An existing deck.
    Deck {
        /// Deck id.
        id: DeckId,
    },
    /// A deck created in the same step (as `decks.create`).
    NewDeck {
        /// Name, or `A::B` path relative to `parent_id`.
        path: String,
        /// Parent deck (`null` = top level).
        #[serde(default)]
        parent_id: Option<DeckId>,
        /// Description (Markdown).
        #[serde(default)]
        description: Option<String>,
    },
}

/// One note of `notes.create_many`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct NoteFieldsDraft {
    /// Field values keyed by field key (missing = default/empty).
    pub fields: BTreeMap<String, String>,
}

/// `notes.create_many` (v1.1): creates several notes of one note type — optionally in a new
/// deck — as ONE command and one undo step (the Create screen's "Save"). Each note runs
/// `notes.create` nested, so validation, cards and classifications are exactly its own.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct CreateNotes {
    /// Note type of every note.
    pub notetype_id: NoteTypeId,
    /// Existing or new deck.
    pub target: NotesTarget,
    /// The notes in order (1–1000; rows whose fields are all blank are skipped).
    pub notes: Vec<NoteFieldsDraft>,
    /// Tags of every note.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Classification values of every note.
    #[serde(default)]
    pub attributes: Vec<AttributeAssignment>,
    /// Create duplicates too (default `false`: a duplicate row is skipped and reported).
    pub allow_duplicate: Option<bool>,
}

/// Response of `notes.create_many`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct NotesCreated {
    /// Deck the notes went to (the new one for `new_deck`).
    pub deck_id: DeckId,
    /// Created notes, in row order.
    pub note_ids: Vec<NoteId>,
    /// Indexes (0-based) of rows skipped because all their fields were blank.
    pub empty_rows: Vec<u32>,
    /// Indexes (0-based) of rows skipped because an equal note exists.
    pub duplicate_rows: Vec<u32>,
}
