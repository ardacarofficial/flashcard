-- 1420_media_origins (v1.4 W4, docs/03 §13, docs/02 §9.1): where a stored sound or picture came
-- from, and the TTS cache.
--
-- * `media_origins`: one row per media file (content hash) whose origin is known:
--   `original` (captured from the film by the Subtitle extension), `clip` (cut from a local
--   video file with ffmpeg), `tts` (synthesised; `provider`, `voice`, `language`, `rate_milli`
--   = speed × 1000 and `text_hash` = SHA-256 of the text with whitespace collapsed) or `user`.
--   A file without a row is `user`, unless its original name marks it as a Subtitle upload
--   (`subtitle-…`, the rule B3's `ses:video` used), which is `original`. `source` = the video
--   file name a clip was cut from (no path).
--   Derived metadata like `note_origins`: undoing the command that added the file keeps the
--   row harmlessly (same bytes = same file = same origin) and a redo finds it again.
-- * The TTS cache is this table: (`provider`, `language`, `voice`, `rate_milli`,
--   `text_hash`) → the file, so the same word is synthesised once.
--
-- Backfill: every file the Subtitle extension uploaded (`subtitle_uploads`) and every file
-- named `subtitle-…` is `original` (the encounters' screenshot/audio hashes of 1490 come from
-- those uploads). Nothing else is guessed: other files stay `user`.

CREATE TABLE media_origins (
    hash       TEXT PRIMARY KEY NOT NULL,
    origin     TEXT NOT NULL CHECK (origin IN ('original', 'clip', 'tts', 'user')),
    provider   TEXT NOT NULL DEFAULT '',
    voice      TEXT NOT NULL DEFAULT '',
    language   TEXT NOT NULL DEFAULT '',
    rate_milli INTEGER NOT NULL DEFAULT 0,
    text_hash  TEXT NOT NULL DEFAULT '',
    source     TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL
) STRICT, WITHOUT ROWID;

CREATE INDEX media_origins_tts ON media_origins (text_hash, language, voice, rate_milli, provider)
    WHERE origin = 'tts';
CREATE INDEX media_origins_origin ON media_origins (origin);

INSERT OR IGNORE INTO media_origins (hash, origin, created_at)
SELECT m.hash, 'original', m.created_at
  FROM media m
 WHERE m.hash IN (SELECT media_hash FROM subtitle_uploads)
    OR (m.original_name IS NOT NULL AND lower(substr(m.original_name, 1, 9)) = 'subtitle-');
