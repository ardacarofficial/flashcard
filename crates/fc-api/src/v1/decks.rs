//! Deck DTOs.

use fc_core::{AttributeDefId, CardId, DeckId, NoteId, PresetId, Timestamp};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A deck as seen by clients.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct DeckDto {
    /// Id.
    pub id: DeckId,
    /// Parent deck (`null` = top level).
    pub parent_id: Option<DeckId>,
    /// Leaf name.
    pub name: String,
    /// Full path `A::B::C`.
    pub path: String,
    /// Description (Markdown).
    pub description: String,
    /// Icon key/emoji.
    pub icon: Option<String>,
    /// Color token or `#rrggbb`.
    pub color: Option<String>,
    /// Options preset (`null` = default).
    pub preset_id: Option<PresetId>,
    /// Position among siblings.
    pub position: i64,
    /// Creation time.
    pub created_at: Timestamp,
    /// Last modification time.
    pub updated_at: Timestamp,
}

/// Card counts shown next to a deck (including sub-decks).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
pub struct DeckCounts {
    /// New cards available today.
    pub new: u32,
    /// Cards in (re)learning due today.
    pub learning: u32,
    /// Review cards due today.
    pub due: u32,
    /// All cards (not deleted).
    pub total: u32,
}

/// A node of the deck tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct DeckTreeNode {
    /// The deck.
    pub deck: DeckDto,
    /// Counts for this deck and its descendants.
    pub counts: DeckCounts,
    /// Child decks, sorted by position then name.
    pub children: Vec<DeckTreeNode>,
}

/// Response of `decks.tree`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct DeckTree {
    /// Top-level decks.
    pub roots: Vec<DeckTreeNode>,
}

/// Response of `decks.list` (flat, sorted by path).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct DeckList {
    /// Decks.
    pub decks: Vec<DeckDto>,
}

/// `decks.create`: creates the deck at `path`, creating missing parents. Fails with
/// `CONFLICT_DECK_EXISTS` if the full path already exists.
///
/// With `parent_id`, `path` is relative to that (live) deck: `{parent_id, path: "Verbs"}`
/// creates a child named `Verbs`. Clients never join paths themselves.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct CreateDeck {
    /// `A::B::C` (relative to `parent_id` when given; a plain name for one child).
    pub path: String,
    /// Parent deck the path is relative to (`null` = top level).
    #[serde(default)]
    pub parent_id: Option<DeckId>,
    /// Description.
    pub description: Option<String>,
    /// Icon.
    pub icon: Option<String>,
    /// Color.
    pub color: Option<String>,
}

/// `decks.get`, `decks.delete`, `decks.restore`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct DeckRef {
    /// Deck id.
    pub id: DeckId,
}

/// Internal `decks.restore_affected` (the inverse of `decks.delete`): restores exactly the
/// decks, notes and cards that delete trashed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct RestoreDeckAffected {
    /// The deck that was deleted (its parent must be live).
    pub id: DeckId,
    /// Trashed decks (`id` and its sub-decks).
    pub decks: Vec<DeckId>,
    /// Trashed notes.
    pub notes: Vec<NoteId>,
    /// Trashed cards.
    pub cards: Vec<CardId>,
}

/// `decks.rename`: changes the leaf name only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct RenameDeck {
    /// Deck id.
    pub id: DeckId,
    /// New leaf name (no `::`).
    pub name: String,
}

/// `decks.move`: re-parents and/or reorders a deck.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct MoveDeck {
    /// Deck id.
    pub id: DeckId,
    /// New parent (`null` = top level).
    pub parent_id: Option<DeckId>,
    /// Position among new siblings (`null` = last).
    pub position: Option<i64>,
}

/// `decks.update`: replaces the presentational fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct UpdateDeck {
    /// Deck id.
    pub id: DeckId,
    /// Description.
    pub description: String,
    /// Icon (`null` clears).
    pub icon: Option<String>,
    /// Color (`null` clears).
    pub color: Option<String>,
}

/// A classification value common in a deck (`Seviye: B1`), for the chips of the Library and
/// the deck page (v1.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DeckChip {
    /// Classification.
    pub attr_id: AttributeDefId,
    /// Classification name (user text).
    pub name: String,
    /// Value label (user text).
    pub label: String,
    /// Notes of the deck (with sub-decks) carrying the value.
    pub notes: u32,
}

/// Progress of a deck with its sub-decks (v1.1 Library / deck page): every live card by
/// [`fc_core::stats::Mastery`], when it was last studied and its most common classification values.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DeckSummary {
    /// Deck.
    pub deck_id: DeckId,
    /// Never studied cards.
    pub new: u32,
    /// Cards being learnt (learning steps, short intervals).
    pub learning: u32,
    /// Mastered (mature) cards.
    pub mastered: u32,
    /// `mastered` out of all cards, 0–100.
    pub mastered_pct: u8,
    /// Last review of any of its cards (`null` = never studied).
    pub last_studied_at: Option<Timestamp>,
    /// Up to three most common classification values, most notes first.
    pub chips: Vec<DeckChip>,
}

/// Response of `decks.summaries`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DeckSummaries {
    /// Today's counts of the whole collection (the sum of the top-level decks of `decks.tree`).
    pub today: DeckCounts,
    /// Every live deck, in tree order.
    pub decks: Vec<DeckSummary>,
    /// Recently studied decks, most recent first (at most [`RECENT_DECKS`]).
    pub recent: Vec<DeckId>,
}

/// Length of [`DeckSummaries::recent`].
pub const RECENT_DECKS: usize = 6;
