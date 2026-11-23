//! Decks (and per-deck card counts).

use super::*;

/// Per-deck card counts (own cards only; the app aggregates sub-decks).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CardCounts {
    /// New, not suspended/buried.
    pub new: u32,
    /// Learning/relearning with `due <= now`.
    pub learning: u32,
    /// Review with `due <= now`.
    pub review_due: u32,
    /// All live cards.
    pub total: u32,
}

/// Entities trashed (or restored) together by a deck delete/restore. Lists are sorted by id
/// and contain only rows whose `deleted_at` actually changed, so the app can store them in
/// the inverse and replay them exactly ([`DeckWrite::set_trash_set_deleted`]).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct DeckTrashSet {
    /// The deck and its sub-decks.
    pub decks: Vec<DeckId>,
    /// Cards in those decks.
    pub cards: Vec<CardId>,
    /// Notes left without a live card (or card-less notes homed in those decks).
    pub notes: Vec<NoteId>,
}

impl DeckTrashSet {
    /// Whether nothing changed.
    pub fn is_empty(&self) -> bool {
        self.decks.is_empty() && self.cards.is_empty() && self.notes.is_empty()
    }
}

/// Deck reads.
pub trait DeckRead {
    /// Deck by id (also if trashed).
    fn get_deck(&self, id: DeckId) -> StorageResult<Option<Deck>>;
    /// All decks.
    fn list_decks(&self, include_deleted: bool) -> StorageResult<Vec<Deck>>;
    /// Live child of `parent` named `name` (Unicode case-insensitive).
    fn find_child_deck(&self, parent: Option<DeckId>, name: &str) -> StorageResult<Option<Deck>>;
    /// Own card counts per deck at `now` (decks without cards may be omitted). Cards of
    /// trashed notes are not counted.
    fn card_counts_by_deck(&self, now: Timestamp) -> StorageResult<Vec<(DeckId, CardCounts)>>;
    /// Live cards per deck: [`CardCounts::total`] of [`Self::card_counts_by_deck`] without
    /// the state counts (v1.4 Z2: SQLite answers it from an index).
    fn card_totals_by_deck(&self) -> StorageResult<Vec<(DeckId, u32)>> {
        Ok(self
            .card_counts_by_deck(Timestamp(0))?
            .into_iter()
            .map(|(d, c)| (d, c.total))
            .collect())
    }
    /// Ancestry of `id`: root first, `id` itself last (trashed decks included). Fails with
    /// `NotFound` for an unknown deck and `Corrupt` on a parent cycle.
    fn deck_ancestry(&self, id: DeckId) -> StorageResult<Vec<DeckId>>;
}

/// Deck writes.
pub trait DeckWrite: DeckRead {
    /// Inserts a new deck.
    fn insert_deck(&mut self, deck: &Deck) -> StorageResult<()>;
    /// Replaces all columns of an existing deck (by id).
    fn update_deck(&mut self, deck: &Deck) -> StorageResult<()>;
    /// Trash (`Some`) or restore (`None`) deck rows only (no cascade). Returns rows changed.
    fn set_decks_deleted(
        &mut self,
        ids: &[DeckId],
        deleted_at: Option<Timestamp>,
    ) -> StorageResult<u32>;
    /// Deck delete rule: trashes `deck` and its live sub-decks (walking through live decks
    /// only), every live card in them, and every live note that is left without a live card
    /// (plus live card-less notes homed in those decks) — all with the same `deleted_at = at`.
    /// Returns exactly what changed (empty if `deck` is already trashed). `NotFound` for an
    /// unknown deck.
    fn trash_deck_subtree(&mut self, deck: DeckId, at: Timestamp) -> StorageResult<DeckTrashSet>;
    /// Trash restore of a deck: with `T = deck.deleted_at`, restores the deck, the sub-decks
    /// reachable through decks trashed at `T`, their cards trashed at `T` and the notes trashed
    /// at `T` that own one of those cards or are homed in those decks. Returns what changed
    /// (empty if the deck is live). `NotFound` for an unknown deck; a sibling-name clash is
    /// `Constraint { "decks_sibling_name" }`.
    fn restore_deck_subtree(&mut self, deck: DeckId) -> StorageResult<DeckTrashSet>;
    /// Applies `deleted_at` to exactly the listed decks, cards and notes (undo/redo of the two
    /// methods above). Trash touches live rows, restore trashed rows. Returns rows changed.
    fn set_trash_set_deleted(
        &mut self,
        set: &DeckTrashSet,
        deleted_at: Option<Timestamp>,
    ) -> StorageResult<u32>;
}
