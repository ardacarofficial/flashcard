//! Cards.

use super::*;

/// Filter for card listing (all set conditions must hold).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CardFilter {
    /// Cards in these decks.
    pub deck_ids: Option<Vec<DeckId>>,
    /// Cards of these notes.
    pub note_ids: Option<Vec<NoteId>>,
    /// Include trashed cards.
    pub include_deleted: bool,
}

/// Card reads.
pub trait CardRead {
    /// Card by id.
    fn get_card(&self, id: CardId) -> StorageResult<Option<Card>>;
    /// Cards matching `filter`.
    fn list_cards(&self, filter: &CardFilter) -> StorageResult<Vec<Card>>;
}

/// Card writes.
pub trait CardWrite: CardRead {
    /// Inserts a card.
    fn insert_card(&mut self, card: &Card) -> StorageResult<()>;
    /// Replaces a card's columns.
    fn update_card(&mut self, card: &Card) -> StorageResult<()>;
    /// Trash/restore. Returns rows changed.
    fn set_cards_deleted(
        &mut self,
        ids: &[CardId],
        deleted_at: Option<Timestamp>,
    ) -> StorageResult<u32>;
}
