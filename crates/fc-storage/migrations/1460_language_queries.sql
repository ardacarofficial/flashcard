-- 1460_language_queries (v1.4 B3, docs/02 §9.1): indexed language-learning predicates of the
-- query language (`nereden:`, `kaynak:`, `kelime:`, `ses:`, `görsel:`).
--
-- * `note_origins`: where a note came from — `subtitle` (Subtitle extension capture),
--   `reading` (reading mode), `anki` (Anki package), `import` (text / Quizlet / JSON /
--   `.fcpack`) — and its source: the title (video title with its episode, e.g.
--   "The Night Train · S1E2", or the reading text's title), the site and the video id. Written
--   by those producers next to the note they make; a note without a row was made by hand (or
--   its origin is unknown). Derived metadata like `word_note_language`: an undone note keeps
--   its row harmlessly (queries only look at live notes) and a redo finds it again.
-- * `word_index_notes_key_only`: `kelime:`/`word:` matches a lemma key in any language.
-- * `media_mime`: `ses:`/`görsel:` start from the audio / image files (a `mime` range).
--
-- Existing collections: Subtitle notes (from `subtitle_captures`) and reading-mode notes (from
-- `word_note_language`) get their origin, with the title taken from the note's `source` field
-- (`fc_source_title`: the text without the trailing `· 1:23`). Earlier imports stay unknown.

CREATE TABLE note_origins (
    note_id    TEXT PRIMARY KEY NOT NULL,
    origin     TEXT NOT NULL CHECK (origin IN ('subtitle', 'reading', 'anki', 'import')),
    source     TEXT NOT NULL DEFAULT '',
    site       TEXT NOT NULL DEFAULT '',
    video_id   TEXT NOT NULL DEFAULT '',
    created_at INTEGER NOT NULL
) STRICT, WITHOUT ROWID;
CREATE INDEX note_origins_origin ON note_origins (origin);

CREATE INDEX word_index_notes_key_only ON word_index_notes (key);
CREATE INDEX media_mime ON media (mime);

INSERT INTO note_origins (note_id, origin, source, site, video_id, created_at)
SELECT c.note_id, 'subtitle',
       fc_source_title(coalesce(json_extract(n.fields, '$.source'), '')), '', '', min(c.created_at)
  FROM subtitle_captures c JOIN notes n ON n.id = c.note_id
 GROUP BY c.note_id;

INSERT INTO note_origins (note_id, origin, source, site, video_id, created_at)
SELECT l.note_id, 'reading',
       fc_source_title(coalesce(json_extract(n.fields, '$.source'), '')), '', '', n.created_at
  FROM word_note_language l JOIN notes n ON n.id = l.note_id
 WHERE l.note_id NOT IN (SELECT note_id FROM note_origins);
