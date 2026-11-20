-- v1.0 maintenance (track O, docs/02 §34): drafts survive a crash.
--
-- `drafts`: what a form held while the user was typing (the note being added, …), saved by
-- the UI every few seconds through `maintenance.drafts.save` and offered back after a crash or
-- an unclean shutdown. One row per form key (`notes.add`); the payload is the form's JSON.
-- Scratch state, not collection data: not an undo step, never in the oplog. Kept in backups
-- like every user table (a restore brings the backup's drafts).
CREATE TABLE drafts (
    key        TEXT PRIMARY KEY NOT NULL,
    payload    TEXT NOT NULL CHECK (json_valid(payload)),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
) WITHOUT ROWID;

CREATE INDEX drafts_updated ON drafts(updated_at);
