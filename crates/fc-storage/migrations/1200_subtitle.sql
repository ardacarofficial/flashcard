-- 1200_subtitle (v1.2): the Subtitle browser extension's captures become notes.
--
-- * `subtitle_keys`: personal access keys made in the app. Only the key's SHA-256 hash is
--   stored (the key itself is shown once); `prefix` is its first characters, for display.
--   A revoked key keeps its row (`revoked_at`) so the oplog's actor id still names it.
-- * `subtitle_captures`: capture → note mapping. `capture_id` (the extension's UUIDv7) makes
--   resends idempotent; `dedup_key` (`<lang>|<lemma>|sha256(lower(collapse_spaces(
--   sentence)))`) makes the same word from the same line one card. No foreign keys: undo and
--   redo put a note and its mapping back in either order.
-- * `subtitle_uploads`: pictures and sounds uploaded before their capture, by capture id and
--   kind (a retried upload gets the same id); the bytes are in the media store.
CREATE TABLE subtitle_keys (
    id           TEXT PRIMARY KEY NOT NULL,
    name         TEXT NOT NULL,
    prefix       TEXT NOT NULL,
    token_hash   TEXT NOT NULL,
    created_at   INTEGER NOT NULL,
    last_used_at INTEGER,
    revoked_at   INTEGER
) STRICT;
CREATE UNIQUE INDEX subtitle_keys_token ON subtitle_keys (token_hash);

CREATE TABLE subtitle_captures (
    capture_id TEXT PRIMARY KEY NOT NULL,
    dedup_key  TEXT NOT NULL,
    lang       TEXT NOT NULL,
    lemma      TEXT NOT NULL,
    note_id    TEXT NOT NULL,
    card_id    TEXT NOT NULL,
    key_id     TEXT,
    created_at INTEGER NOT NULL
) STRICT;
CREATE UNIQUE INDEX subtitle_captures_dedup ON subtitle_captures (dedup_key);
CREATE INDEX subtitle_captures_lemma ON subtitle_captures (lang, lemma);

CREATE TABLE subtitle_uploads (
    id         TEXT PRIMARY KEY NOT NULL,
    capture_id TEXT NOT NULL,
    kind       TEXT NOT NULL CHECK (kind IN ('screenshot', 'audio')),
    media_hash TEXT NOT NULL,
    mime       TEXT NOT NULL,
    created_at INTEGER NOT NULL
) STRICT;
CREATE UNIQUE INDEX subtitle_uploads_capture ON subtitle_uploads (capture_id, kind);
CREATE INDEX subtitle_uploads_created ON subtitle_uploads (created_at);
