//! Search (query language), saved queries and the read-only maintenance reports of v0.4: trash
//! listing, duplicates, card review history and the integrity check.
//!
//! A [`Filter`] is already resolved (ids, numbers, times); storage only evaluates it.
//! Trashed cards and notes never match.

use super::*;
use fc_core::query::{Filter, SavedQuery};
use fc_core::SavedQueryId;

/// One `note_origins` row (v1.4, migration 1460): where a note came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoteOriginRecord {
    /// The note.
    pub note_id: NoteId,
    /// Origin (never [`fc_core::query::NoteOrigin::Manual`]: that is the absence of a row).
    pub origin: fc_core::query::NoteOrigin,
    /// Source title (video title with its episode, reading text title); may be empty.
    pub source: String,
    /// Site (`youtube`, `netflix`); may be empty.
    pub site: String,
    /// Video id; may be empty.
    pub video_id: String,
    /// When it was recorded.
    pub created_at: Timestamp,
}

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
    /// Rows to skip (ignored with `after`).
    pub offset: u32,
    /// Maximum rows.
    pub limit: u32,
    /// Keyset paging (v1.4): the page starts right after this row of the same search (same
    /// filter, sort, direction and `notes_only`) instead of at `offset`.
    pub after: Option<CardCursor>,
}

/// A value of a sort column, as the browser orders it (NULL first ascending, last descending;
/// text compared ASCII-case-insensitively like SQLite `NOCASE`).
#[derive(Debug, Clone, PartialEq)]
pub enum SortValue {
    /// No value (a new card's due, stability, difficulty).
    Null,
    /// Integer columns and times (ms).
    Int(i64),
    /// Stability, difficulty.
    Real(f64),
    /// Sort field.
    Text(String),
}

impl SortValue {
    /// The browser order of two values of one column.
    pub fn cmp_sorted(&self, other: &SortValue) -> std::cmp::Ordering {
        use std::cmp::Ordering;
        match (self, other) {
            (SortValue::Null, SortValue::Null) => Ordering::Equal,
            (SortValue::Null, _) => Ordering::Less,
            (_, SortValue::Null) => Ordering::Greater,
            (SortValue::Int(a), SortValue::Int(b)) => a.cmp(b),
            (SortValue::Real(a), SortValue::Real(b)) => a.partial_cmp(b).unwrap_or(Ordering::Equal),
            (SortValue::Text(a), SortValue::Text(b)) => a
                .bytes()
                .map(|c| c.to_ascii_lowercase())
                .cmp(b.bytes().map(|c| c.to_ascii_lowercase())),
            // One column never mixes kinds; a stable answer all the same.
            (a, b) => a.rank().cmp(&b.rank()),
        }
    }

    fn rank(&self) -> u8 {
        match self {
            SortValue::Null => 0,
            SortValue::Int(_) => 1,
            SortValue::Real(_) => 2,
            SortValue::Text(_) => 3,
        }
    }
}

/// Keyset position (v1.4): the sort value and card id of the last row of the previous page.
#[derive(Debug, Clone, PartialEq)]
pub struct CardCursor {
    /// Sort column value of that row ([`CardSort::key`]).
    pub key: SortValue,
    /// Its card (ties are ordered by card id).
    pub card_id: CardId,
}

impl CardSort {
    /// Whether the column can be NULL (then NULL sorts first ascending, last descending).
    pub fn nullable(self) -> bool {
        matches!(self, CardSort::Due | CardSort::Stability | CardSort::Difficulty)
    }

    /// The value a row is ordered by.
    pub fn key(self, card: &Card, sort_field: &str, note_updated_at: Timestamp) -> SortValue {
        let s = &card.scheduling;
        let real = |v: Option<f64>| v.map_or(SortValue::Null, SortValue::Real);
        match self {
            CardSort::SortField => SortValue::Text(sort_field.to_owned()),
            CardSort::Created => SortValue::Int(card.created_at.0),
            CardSort::Modified => SortValue::Int(note_updated_at.0),
            CardSort::Due => s.due.map_or(SortValue::Null, |d| SortValue::Int(d.0)),
            CardSort::Stability => real(s.stability),
            CardSort::Difficulty => real(s.difficulty),
            CardSort::Lapses => SortValue::Int(i64::from(s.lapses)),
            CardSort::Reps => SortValue::Int(i64::from(s.reps)),
        }
    }

    /// The cursor of the page after `hit`.
    pub fn cursor_after(self, hit: &CardHit) -> CardCursor {
        CardCursor {
            key: self.key(&hit.card, &hit.sort_field, hit.note_updated_at),
            card_id: hit.card.id,
        }
    }
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
    /// More rows follow this page.
    pub more: bool,
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
    /// Live notes lacking a field their note type defines (extra keys are not reported: a
    /// note type change may keep the data of removed fields).
    pub broken_fields: Vec<NoteId>,
}

/// What [`SearchWrite::purge_trash`] removed for good.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PurgeCounts {
    /// Notes (with all their cards).
    pub notes: u32,
    /// Cards trashed on their own (their note stays).
    pub cards: u32,
    /// Decks.
    pub decks: u32,
}

/// Search reads.
pub trait SearchRead {
    /// A page of matching cards.
    fn search_cards(&self, search: &CardSearch) -> StorageResult<SearchPage>;
    /// Every matching card id (creation order).
    fn search_card_ids(&self, filter: &Filter, now: Timestamp) -> StorageResult<Vec<CardId>>;
    /// The cards of `among` that match (creation order). Costs in proportion to `among`, not
    /// to the collection (automation rules check the few cards one change touched, v0.9).
    fn search_card_ids_among(
        &self,
        filter: &Filter,
        now: Timestamp,
        among: &[CardId],
    ) -> StorageResult<Vec<CardId>>;
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
    /// v1.4: where these notes came from (rows only for notes that have one).
    fn note_origins_of(&self, notes: &[NoteId]) -> StorageResult<Vec<NoteOriginRecord>>;
}

/// Search writes.
pub trait SearchWrite: SearchRead {
    /// Inserts or replaces a saved query.
    fn upsert_saved_query(&mut self, query: &SavedQuery) -> StorageResult<()>;
    /// Deletes a saved query. Returns whether it existed.
    fn delete_saved_query(&mut self, id: SavedQueryId) -> StorageResult<bool>;
    /// Deletes classification values whose note/card no longer exists (see
    /// [`IntegrityReport::dangling_values`]). Returns the number of rows removed.
    fn delete_dangling_values(&mut self) -> StorageResult<u32>;
    /// Permanently deletes what was trashed before `before`: notes (their cards, links,
    /// tags, search rows and review history go with them), cards trashed on their own, then
    /// decks nothing refers to any more (no notes, cards, child decks or classification
    /// scopes), and the classification values left without an entity.
    fn purge_trash(&mut self, before: Timestamp) -> StorageResult<PurgeCounts>;
    /// v1.4: records (or replaces) where a note came from. Derived metadata like the word
    /// index's note language: written by the producer next to the note it makes.
    fn set_note_origin(&mut self, origin: &NoteOriginRecord) -> StorageResult<()>;
}
