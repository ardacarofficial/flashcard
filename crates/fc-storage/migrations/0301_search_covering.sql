-- 0301_search_covering (v0.4): indexes for the card search join (fc-storage sqlite/search.rs).
--
-- * notes_id_live / cards_note_live: the cards ⋈ notes join, the liveness checks, `is:suspended`
--   and the matched card ids are answered from these indexes alone, without fetching the
--   (large) note and card rows.
-- * notes_trashed: the (usually tiny) set of trashed notes; a search that reads no note column
--   checks "note is live" against it instead of joining every card to its note.
CREATE INDEX notes_id_live ON notes(id, deleted_at);
CREATE INDEX cards_note_live ON cards(note_id, deleted_at, suspended, id);
CREATE INDEX notes_trashed ON notes(id) WHERE deleted_at IS NOT NULL;
