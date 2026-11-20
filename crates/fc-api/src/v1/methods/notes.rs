//! Notes.

use crate::v1::common::*;
use crate::v1::notes::*;

methods! {
    // ---- notes ----
    /// Creates a note and its cards.
    NotesCreate = "notes.create", Command, "notes", "notes:write", Public, undo: true, CreateNote => NoteDto;
    /// Replaces fields/tags/attributes of a note.
    NotesUpdate = "notes.update", Command, "notes", "notes:write", Public, undo: true, UpdateNote => NoteDto;
    /// Moves notes (and their cards) to the trash.
    NotesDelete = "notes.delete", Command, "notes", "notes:write", Public, undo: true, NoteIds => AffectedCount;
    /// Restores notes from the trash.
    NotesRestore = "notes.restore", Command, "notes", "notes:write", Public, undo: true, NoteIds => AffectedCount;
    /// One note.
    NotesGet = "notes.get", Query, "notes", "notes:read", Public, undo: false, NoteRef => NoteDto;
    /// Paged note list.
    NotesList = "notes.list", Query, "notes", "notes:read", Public, undo: false, ListNotes => NotePage;
    /// Paged term list of a deck: notes as term | definition with mastery, flags and level
    /// chips (v1.1 deck page).
    NotesTerms = "notes.terms", Query, "notes", "notes:read", Public, undo: false, ListNotes => TermPage;
}
