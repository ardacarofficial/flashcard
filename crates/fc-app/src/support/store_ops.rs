//! Multi-row storage operations used by handlers: deck-subtree trash/restore with affected id
//! lists, option insert at an index and option remove returning what changed.
//!
//! Each function is the **single seam** for its operation and is bound to one native
//! `fc-storage` call (plus, where the handler needs previous values for its inverse, the reads
//! that capture them). Like storage, these hold no business rules (no validation, no inverse
//! building, no events) and never mint ids.

use crate::error::AppResult;
use fc_core::attribute::{AttributeOption, AttributeValue, EntityRef};
use fc_core::{AttributeDefId, AttributeOptionId, CardId, DeckId, NoteId, Timestamp};
use fc_storage::repo::DeckTrashSet;
use fc_storage::WriteTx;

/// Entities trashed together (one `deleted_at`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Trashed {
    /// Decks.
    pub decks: Vec<DeckId>,
    /// Notes.
    pub notes: Vec<NoteId>,
    /// Cards.
    pub cards: Vec<CardId>,
}

impl From<DeckTrashSet> for Trashed {
    fn from(s: DeckTrashSet) -> Self {
        Self {
            decks: s.decks,
            notes: s.notes,
            cards: s.cards,
        }
    }
}

impl Trashed {
    fn to_set(&self) -> DeckTrashSet {
        DeckTrashSet {
            decks: self.decks.clone(),
            cards: self.cards.clone(),
            notes: self.notes.clone(),
        }
    }
}

/// Soft-deletes `deck`, its live sub-decks, their live cards and the notes left without a live
/// card (plus card-less notes homed there), all with `at`. Returns exactly what was trashed.
pub(crate) fn trash_deck_subtree(
    tx: &mut dyn WriteTx,
    deck: DeckId,
    at: Timestamp,
) -> AppResult<Trashed> {
    Ok(tx.trash_deck_subtree(deck, at)?.into())
}

/// Trash restore of `deck`: everything trashed together with it (same `deleted_at`).
/// Returns what was restored (empty for a live deck).
pub(crate) fn restore_deck_subtree(tx: &mut dyn WriteTx, deck: DeckId) -> AppResult<Trashed> {
    Ok(tx.restore_deck_subtree(deck)?.into())
}

/// Clears `deleted_at` on exactly the listed (trashed) entities. Returns rows changed.
pub(crate) fn restore_trashed(tx: &mut dyn WriteTx, t: &Trashed) -> AppResult<u32> {
    Ok(tx.set_trash_set_deleted(&t.to_set(), None)?)
}

/// Inserts `option` with its own id at `index` of the sorted option list (clamped).
pub(crate) fn insert_option(
    tx: &mut dyn WriteTx,
    attr: AttributeDefId,
    option: &AttributeOption,
    index: usize,
    now: Timestamp,
) -> AppResult<()> {
    tx.insert_attribute_option(attr, option, index, now)?;
    Ok(())
}

/// Removes `option` from `attr`. Values using it move to `into` (merge) or lose it (delete;
/// an emptied value is removed). Returns the previous value of every entity whose value
/// changed, trashed notes included.
pub(crate) fn remove_option(
    tx: &mut dyn WriteTx,
    attr: AttributeDefId,
    option: AttributeOptionId,
    into: Option<AttributeOptionId>,
    now: Timestamp,
) -> AppResult<Vec<(EntityRef, AttributeValue)>> {
    let mut before = Vec::new();
    for entity in tx.entities_with_option(attr, option)? {
        let current = tx
            .get_attribute_values(entity)?
            .into_iter()
            .find(|(id, _)| *id == attr)
            .map(|(_, v)| v);
        if let Some(v) = current {
            before.push((entity, v));
        }
    }
    let removal = tx.remove_attribute_option(attr, option, into, now)?;
    Ok(before
        .into_iter()
        .filter(|(e, _)| removal.moved.contains(e) || removal.dropped.contains(e))
        .collect())
}
