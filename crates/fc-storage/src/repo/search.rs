//! Search (query language), saved queries and the read-only maintenance reports of v0.4: trash
//! listing, duplicates, card review history and the integrity check.
//!
//! A [`Filter`] is already resolved (ids, numbers, times); storage only evaluates it.
//! Trashed cards and notes never match.

use super::*;
use fc_core::query::{Filter, SavedQuery};
use fc_core::SavedQueryId;

/// Sort column of a card search.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CardSort {
    /// Note sort field (text).
    #[default]
    SortField,
    /// Card creation.
    Created,
    /// Note modification.
    Modified,
    /// Due time (new cards first when ascending).
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

/// A paged card search.
#[derive(Debug, Clone, PartialEq)]
pub struct CardSearch {
    /// Resolved filter.
    pub filter: Filter,
    /// "Now" (due/buried/retrievability).
    pub now: Timestamp,
    /// Sort column.
    pub sort: CardSort,
    /// Descending order.
    pub descending: bool,
    /// One row per note (its first matching card) instead of one per card.
    pub notes_only: bool,
    /// Rows to skip.
    pub offset: u32,
    /// Maximum rows.
    pub limit: u32,
}

/// One matching card with the note data a browser row needs.
#[derive(Debug, Clone, PartialEq)]
pub struct CardHit {
    /// The card.
    pub card: Card,
    /// Note type of its note.
    pub notetype_id: NoteTypeId,
    /// Note sort field.
    pub sort_field: String,
    /// Note tags (stored order).
    pub tags: Vec<TagName>,
    /// Note modification time.
    pub note_updated_at: Timestamp,
}

/// A page of hits plus the total number of matches.
#[derive(Debug, Clone, PartialEq)]
pub struct SearchPage {
    /// Hits in sort order.
    pub hits: Vec<CardHit>,
    /// Matches in total (cards, or notes with `notes_only`).
    pub total: u32,
}

/// One `review_log` row (card history).
#[derive(Debug, Clone, PartialEq)]
pub struct ReviewEntry {
    /// Rating (`again`/`hard`/`good`/`easy`).
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
    /// Session mode (`normal`, `practice`, …).
    pub mode: String,
    /// Stability after the review.
    pub stability_after: Option<f64>,
    /// Difficulty after the review.
    pub difficulty_after: Option<f64>,
    /// When.
    pub created_at: Timestamp,
}

/// Findings of the database check (ids sorted).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IntegrityReport {
    /// Live cards whose note is trashed or missing.
    pub orphan_cards: Vec<CardId>,
    /// Live cards in a trashed deck.
    pub cards_in_trashed_decks: Vec<CardId>,
    /// Scheduled (non-new) live cards without a due time.
    pub unscheduled_cards: Vec<CardId>,
    /// Live notes without a full-text row.
    pub missing_search_index: Vec<NoteId>,
    /// Live notes without any live card.
    pub cardless_notes: Vec<NoteId>,
    /// Classification values whose note/card no longer exists.
    pub dangling_values: u32,
}

/// Search reads.
pub trait SearchRead {
    /// A page of matching cards.
    fn search_cards(&self, search: &CardSearch) -> StorageResult<SearchPage>;
    /// Every matching card id (creation order).
    fn search_card_ids(&self, filter: &Filter, now: Timestamp) -> StorageResult<Vec<CardId>>;
    /// Saved queries by position, then name.
    fn list_saved_queries(&self) -> StorageResult<Vec<SavedQuery>>;
    /// Saved query by id.
    fn get_saved_query(&self, id: SavedQueryId) -> StorageResult<Option<SavedQuery>>;
    /// Groups (size ≥ 2) of live notes sharing note type and checksum.
    fn duplicate_note_groups(&self) -> StorageResult<Vec<Vec<NoteId>>>;
    /// Trashed notes, most recently trashed first.
    fn list_trashed_notes(&self, limit: u32) -> StorageResult<Vec<Note>>;
    /// Review history of a card, oldest first.
    fn card_reviews(&self, card: CardId) -> StorageResult<Vec<ReviewEntry>>;
    /// Database check.
    fn integrity_report(&self) -> StorageResult<IntegrityReport>;
}

/// Search writes.
pub trait SearchWrite: SearchRead {
    /// Inserts or replaces a saved query.
    fn upsert_saved_query(&mut self, query: &SavedQuery) -> StorageResult<()>;
    /// Deletes a saved query. Returns whether it existed.
    fn delete_saved_query(&mut self, id: SavedQueryId) -> StorageResult<bool>;
}
