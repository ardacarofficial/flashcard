//! Decks.

use crate::v1::common::*;
use crate::v1::decks::*;

methods! {
    // ---- decks ----
    /// Deck tree with counts.
    DecksTree = "decks.tree", Query, "decks", "decks:read", Public, undo: false, Empty => DeckTree;
    /// Flat deck list.
    DecksList = "decks.list", Query, "decks", "decks:read", Public, undo: false, Empty => DeckList;
    /// One deck.
    DecksGet = "decks.get", Query, "decks", "decks:read", Public, undo: false, DeckRef => DeckDto;
    /// Creates a deck (and missing parents) from a path.
    DecksCreate = "decks.create", Command, "decks", "decks:write", Public, undo: true, CreateDeck => DeckDto;
    /// Renames a deck (leaf name).
    DecksRename = "decks.rename", Command, "decks", "decks:write", Public, undo: true, RenameDeck => DeckDto;
    /// Re-parents/reorders a deck.
    DecksMove = "decks.move", Command, "decks", "decks:write", Public, undo: true, MoveDeck => DeckDto;
    /// Updates description/icon/color.
    DecksUpdate = "decks.update", Command, "decks", "decks:write", Public, undo: true, UpdateDeck => DeckDto;
    /// Moves a deck, its sub-decks, notes and cards to the trash.
    DecksDelete = "decks.delete", Command, "decks", "decks:write", Public, undo: true, DeckRef => AffectedCount;
    /// Restores a deck deleted by `decks.delete` (with what was deleted along with it).
    DecksRestore = "decks.restore", Command, "decks", "decks:write", Public, undo: true, DeckRef => AffectedCount;
    /// Internal inverse of `decks.delete`: restores exactly the listed decks/notes/cards.
    DecksRestoreAffected = "decks.restore_affected", Command, "decks", "decks:write", Internal, undo: true, RestoreDeckAffected => AffectedCount;
}
