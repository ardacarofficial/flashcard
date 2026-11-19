-- v0.5 import/export: media content, keyed by the `media` row's hash.
-- Until a file-based media store exists the bytes live in the database (which also makes a
-- .fcbackup of the database complete). Content-addressed: one row per distinct file.
CREATE TABLE media_blobs (
    hash  TEXT PRIMARY KEY NOT NULL REFERENCES media(hash) ON DELETE CASCADE,
    data  BLOB NOT NULL
) STRICT;
