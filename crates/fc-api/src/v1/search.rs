//! Search (query language, docs/02 §9) and saved queries DTOs.

use fc_core::card::CardState;
use fc_core::query::Span;
use fc_core::tag::TagName;
use fc_core::{CardId, DeckId, NoteId, NoteTypeId, SavedQueryId, Timestamp};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

/// Sort column of the card browser.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum CardSortKey {
    /// Note sort field.
    #[default]
    SortField,
    /// Card creation.
    Created,
    /// Note modification.
    Modified,
    /// Due time.
    Due,
    /// Stability.
    Stability,
    /// Difficulty.
    Difficulty,
    /// Lapses.
    Lapses,
    /// Reps.
    Reps,
}

/// `search.cards`: a page of cards matching a query.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct SearchCards {
    /// Query text (empty = everything).
    pub query: String,
    /// Sort column (default `sort_field`).
    pub sort: Option<CardSortKey>,
    /// Descending (default `false`).
    pub descending: Option<bool>,
    /// One row per note (default `false`).
    pub notes_only: Option<bool>,
    /// Rows to skip (default 0).
    pub offset: Option<u32>,
    /// Page size (default 100, max 500).
    pub limit: Option<u32>,
}

/// One browser row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct CardRow {
    /// Card.
    pub card_id: CardId,
    /// Note.
    pub note_id: NoteId,
    /// Deck of the card.
    pub deck_id: DeckId,
    /// Full deck path (`A::B`).
    pub deck_path: String,
    /// Note type.
    pub notetype_id: NoteTypeId,
    /// Note sort field text.
    pub sort_field: String,
    /// Template key.
    pub template_key: String,
    /// Card type engine id.
    pub card_type: String,
    /// Learning state.
    pub state: CardState,
    /// Due time.
    pub due: Option<Timestamp>,
    /// FSRS stability (days).
    pub stability: Option<f64>,
    /// FSRS difficulty.
    pub difficulty: Option<f64>,
    /// Reps.
    pub reps: u32,
    /// Lapses.
    pub lapses: u32,
    /// Flag bitmask.
    pub flags: u32,
    /// Suspended.
    pub suspended: bool,
    /// Buried now.
    pub buried: bool,
    /// Note tags.
    pub tags: Vec<TagName>,
    /// Card creation.
    pub created_at: Timestamp,
    /// Note modification.
    pub note_updated_at: Timestamp,
}

/// Response of `search.cards`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct CardSearchPage {
    /// Rows.
    pub items: Vec<CardRow>,
    /// Matches in total.
    pub total: u32,
    /// Offset of the first row.
    pub offset: u32,
}

/// `search.ids` / `search.check`: just a query.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct QueryText {
    /// Query text.
    pub query: String,
}

/// Response of `search.ids`: every match (selection "all", custom study, bulk operations).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SearchIds {
    /// Matching cards (creation order).
    pub card_ids: Vec<CardId>,
    /// Their distinct notes (first-seen order).
    pub note_ids: Vec<NoteId>,
}

/// A positioned query error (also sent as `details` of `VALIDATION_QUERY`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct QueryErrorDto {
    /// Error kind (`unclosed_quote`, `unknown_option`, …): i18n key `errors.query.<kind>`.
    pub kind: String,
    /// Character range of the problem.
    pub span: Span,
    /// Interpolation params (`key`, `value`, `op`, `message`).
    pub params: BTreeMap<String, String>,
}

/// Response of `search.check`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct QueryCheck {
    /// `null` = the query is valid.
    pub error: Option<QueryErrorDto>,
    /// Canonical form of the query (`null` when invalid).
    pub normalized: Option<String>,
}

/// A saved query.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SavedQueryDto {
    /// Id.
    pub id: SavedQueryId,
    /// Name.
    pub name: String,
    /// Query text.
    pub query: String,
    /// Icon key.
    pub icon: Option<String>,
    /// Sort position.
    pub position: i64,
    /// Creation time.
    pub created_at: Timestamp,
    /// Last modification time.
    pub updated_at: Timestamp,
}

/// Response of `saved_queries.list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct SavedQueryList {
    /// By position, then name.
    pub items: Vec<SavedQueryDto>,
}

/// `saved_queries.create`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct CreateSavedQuery {
    /// Name.
    pub name: String,
    /// Query text (must be valid now).
    pub query: String,
    /// Icon key.
    pub icon: Option<String>,
    /// Position (default: last).
    pub position: Option<i64>,
}

/// `saved_queries.update`: replaces name, query and icon.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct UpdateSavedQuery {
    /// Id.
    pub id: SavedQueryId,
    /// Name.
    pub name: String,
    /// Query text (must be valid now).
    pub query: String,
    /// Icon key.
    pub icon: Option<String>,
    /// Position (`null` = unchanged).
    pub position: Option<i64>,
}

/// `saved_queries.delete` / `saved_queries.get`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SavedQueryRef {
    /// Id.
    pub id: SavedQueryId,
}

/// `saved_queries.restore` (internal inverse of delete): puts the row back as it was.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct RestoreSavedQuery {
    /// The row.
    pub query: SavedQueryDto,
}
