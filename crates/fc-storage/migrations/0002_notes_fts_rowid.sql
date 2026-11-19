-- 0002_notes_fts_rowid: link each note to its notes_fts row.
--
-- notes_fts.note_id is UNINDEXED, so finding a note's FTS row by note_id scans the whole
-- index. Storing the FTS rowid on the note makes FTS updates O(log n). The rowid is stored
-- explicitly (not derived from notes.rowid, which VACUUM may renumber).

ALTER TABLE notes ADD COLUMN fts_rowid INTEGER;

UPDATE notes SET fts_rowid = (
    SELECT min(f.rowid) FROM notes_fts f WHERE f.note_id = notes.id
);

CREATE UNIQUE INDEX notes_fts_rowid ON notes(fts_rowid) WHERE fts_rowid IS NOT NULL;
