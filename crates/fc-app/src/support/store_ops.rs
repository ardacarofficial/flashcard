//! Multi-row storage operations the store will provide natively (storage lane, v0.1
//! integration): deck-subtree trash/restore with affected id lists, option insert at an index
//! and option remove returning what changed.
//!
//! Until those land they are composed here from today's repository primitives. Each function
//! is the **single seam** for its operation: the integrator rebinds the body to one storage
//! call and keeps the signature, so handlers do not change. Like storage, these hold no
//! business rules (no validation, no inverse building, no events) and never mint ids.

use crate::error::AppResult;
use fc_core::attribute::{AttributeOption, AttributeValue, EntityRef};
use fc_core::deck::Deck;
use fc_core::{AttributeDefId, AttributeOptionId, CardId, DeckId, NoteId, Timestamp};
use fc_storage::repo::CardFilter;
use fc_storage::{Reads, StorageError, WriteTx};
use std::collections::BTreeSet;

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

fn subtree(all: &[Deck], root: DeckId, keep: impl Fn(&Deck) -> bool) -> Vec<DeckId> {
    let mut out = vec![root];
    let mut i = 0;
    while i < out.len() {
        let parent = out[i];
        out.extend(
            all.iter()
                .filter(|d| d.parent_id == Some(parent) && keep(d))
                .map(|d| d.id),
        );
        i += 1;
    }
    out
}

/// Soft-deletes `deck`, its live sub-decks, their live cards and the notes left without a live
/// card, all with `at`. Returns exactly what was trashed.
pub(crate) fn trash_deck_subtree(
    tx: &mut dyn WriteTx,
    deck: DeckId,
    at: Timestamp,
) -> AppResult<Trashed> {
    let all = tx.list_decks(false)?;
    let decks = subtree(&all, deck, |_| true);
    let cards = tx.list_cards(&CardFilter {
        deck_ids: Some(decks.clone()),
        note_ids: None,
        include_deleted: false,
    })?;
    let card_ids: Vec<CardId> = cards.iter().map(|c| c.id).collect();
    let touched: BTreeSet<NoteId> = cards.iter().map(|c| c.note_id).collect();
    let mut notes = Vec::new();
    for n in touched {
        let live = tx.list_cards(&CardFilter {
            deck_ids: None,
            note_ids: Some(vec![n]),
            include_deleted: false,
        })?;
        if live.iter().all(|c| card_ids.contains(&c.id)) {
            notes.push(n);
        }
    }
    tx.set_decks_deleted(&decks, Some(at))?;
    tx.set_notes_deleted(&notes, Some(at))?;
    tx.set_cards_deleted(&card_ids, Some(at))?;
    Ok(Trashed {
        decks,
        notes,
        cards: card_ids,
    })
}

/// What was trashed together with the trashed `deck`: its sub-decks, their cards and those
/// cards' notes whose `deleted_at` equals the deck's. Empty for a live deck.
pub(crate) fn trashed_with_deck(reads: &dyn Reads, deck: &Deck) -> AppResult<Trashed> {
    let Some(at) = deck.deleted_at else {
        return Ok(Trashed::default());
    };
    let all = reads.list_decks(true)?;
    let decks = subtree(&all, deck.id, |d| d.deleted_at == Some(at));
    let cards: Vec<_> = reads
        .list_cards(&CardFilter {
            deck_ids: Some(decks.clone()),
            note_ids: None,
            include_deleted: true,
        })?
        .into_iter()
        .filter(|c| c.deleted_at == Some(at))
        .collect();
    let mut notes = Vec::new();
    for c in &cards {
        if notes.contains(&c.note_id) {
            continue;
        }
        let note = reads
            .get_note(c.note_id)?
            .ok_or_else(|| StorageError::NotFound {
                entity: "note",
                id: c.note_id.to_string(),
            })?;
        if note.deleted_at == Some(at) {
            notes.push(note.id);
        }
    }
    Ok(Trashed {
        decks,
        notes,
        cards: cards.iter().map(|c| c.id).collect(),
    })
}

/// Clears `deleted_at` on the listed entities. Returns the rows changed per table.
pub(crate) fn restore_trashed(tx: &mut dyn WriteTx, t: &Trashed) -> AppResult<(u32, u32, u32)> {
    Ok((
        tx.set_decks_deleted(&t.decks, None)?,
        tx.set_notes_deleted(&t.notes, None)?,
        tx.set_cards_deleted(&t.cards, None)?,
    ))
}

fn def_not_found(attr: AttributeDefId) -> StorageError {
    StorageError::NotFound {
        entity: "attribute",
        id: attr.to_string(),
    }
}

/// Inserts `option` with its own id at `index` of the sorted option list (clamped).
pub(crate) fn insert_option(
    tx: &mut dyn WriteTx,
    attr: AttributeDefId,
    option: &AttributeOption,
    index: usize,
) -> AppResult<()> {
    let mut def = tx
        .get_attribute_def(attr)?
        .ok_or_else(|| def_not_found(attr))?;
    def.insert_option_at(option.clone(), index)?;
    tx.update_attribute_def(&def)?;
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
) -> AppResult<Vec<(EntityRef, AttributeValue)>> {
    let mut def = tx
        .get_attribute_def(attr)?
        .ok_or_else(|| def_not_found(attr))?;
    let mut changed = Vec::new();
    for entity in tx.entities_with_option(attr, option)? {
        let current = tx
            .get_attribute_values(entity)?
            .into_iter()
            .find(|(id, _)| *id == attr)
            .map(|(_, v)| v);
        let Some(current) = current else {
            continue;
        };
        let new = super::attrs::replace_option(&current, option, into);
        tx.set_attribute_values(entity, &[(attr, new)])?;
        changed.push((entity, current));
    }
    def.remove_option(option)?;
    tx.update_attribute_def(&def)?;
    Ok(changed)
}
