//! Deck DTOs.

use fc_core::{DeckId, PresetId, Timestamp};
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct CreateDeck {
    /// `A::B::C`.
    pub path: String,
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
