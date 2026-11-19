-- v0.3 card types & content: media references, [[links]], flag names.
--
-- note_media: which media files a note's fields reference (`media:<sha256>`); maintained by
--   the app on every note write. media.ref_count mirrors the number of rows per hash
--   (trashed notes keep their references, so restoring them never loses media).
-- note_link_refs: `[[target]]` links by normalized target key (resolved at read time, so a
--   link to a note created later works without re-saving the source).
-- note_keys: each note's own normalized key (its sort field), the link target side.
--   (The v0.1 `links` table with resolved ids stays unused for now.)
-- card_flag_defs: the seven card flags (bit n-1 of cards.flags) with optional custom names.

CREATE TABLE note_media (
    note_id  TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
    hash     TEXT NOT NULL REFERENCES media(hash),
    PRIMARY KEY (note_id, hash)
) STRICT, WITHOUT ROWID;
CREATE INDEX note_media_hash ON note_media(hash);

CREATE TABLE note_link_refs (
    from_note   TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
    target_key  TEXT NOT NULL,
    label       TEXT NOT NULL DEFAULT '',
    PRIMARY KEY (from_note, target_key)
) STRICT, WITHOUT ROWID;
CREATE INDEX note_link_refs_target ON note_link_refs(target_key);

CREATE TABLE note_keys (
    note_id  TEXT PRIMARY KEY NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
    key      TEXT NOT NULL
) STRICT;
CREATE INDEX note_keys_key ON note_keys(key);

CREATE TABLE card_flag_defs (
    flag        INTEGER PRIMARY KEY NOT NULL CHECK (flag BETWEEN 1 AND 7),
    name        TEXT,
    color       TEXT NOT NULL,
    updated_at  INTEGER NOT NULL
) STRICT;
INSERT INTO card_flag_defs (flag, name, color, updated_at) VALUES
    (1, NULL, 'red', 0),
    (2, NULL, 'orange', 0),
    (3, NULL, 'green', 0),
    (4, NULL, 'blue', 0),
    (5, NULL, 'pink', 0),
    (6, NULL, 'turquoise', 0),
    (7, NULL, 'purple', 0);
