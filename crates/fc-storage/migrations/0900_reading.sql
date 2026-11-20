-- v0.10 reading mode, subtitles and incremental reading (docs/02 §24–§26).
--
-- 1. `reading_items`: texts kept for reading — pasted/opened texts, subtitles (cues + an
--    optional video next to them for clips) and extracts (a part of a parent item). Every
--    item can sit in the incremental-reading queue: `queued`, `priority`, `due`,
--    `interval_days` (fc_core::reading::next_reading_interval). Soft-deleted like notes.
--    Offsets are UTF-16 code units (the API's unit, JavaScript string indices).
CREATE TABLE reading_items (
    id             TEXT PRIMARY KEY NOT NULL,
    kind           TEXT NOT NULL CHECK (kind IN ('text', 'subtitle', 'extract')),
    parent_id      TEXT REFERENCES reading_items(id) ON DELETE SET NULL,
    title          TEXT NOT NULL,
    language       TEXT NOT NULL,                 -- BCP-47 (`en`, `tr`, `en-GB`)
    source         TEXT,                          -- URL or file name it came from
    body           TEXT NOT NULL,                 -- plain text; subtitles: one cue per line
    length         INTEGER NOT NULL DEFAULT 0,    -- body length in UTF-16 units (set by the app)
    cues           TEXT CHECK (cues IS NULL OR json_valid(cues)),  -- [{start_ms,end_ms,text}]
    media_path     TEXT,                          -- local video/audio of a subtitle (clips)
    extract_start  INTEGER,                       -- extracts: UTF-16 range in the parent body
    extract_end    INTEGER,
    position       INTEGER NOT NULL DEFAULT 0,    -- UTF-16 offset read up to
    queued         INTEGER NOT NULL DEFAULT 0 CHECK (queued IN (0, 1)),
    priority       TEXT NOT NULL DEFAULT 'normal' CHECK (priority IN ('high', 'normal', 'low')),
    due            INTEGER,
    interval_days  INTEGER NOT NULL DEFAULT 0,
    read_count     INTEGER NOT NULL DEFAULT 0,
    last_read      INTEGER,
    created_at     INTEGER NOT NULL,
    updated_at     INTEGER NOT NULL,
    deleted_at     INTEGER
) STRICT;
CREATE INDEX reading_items_queue ON reading_items(due) WHERE queued = 1 AND deleted_at IS NULL;
CREATE INDEX reading_items_parent ON reading_items(parent_id) WHERE parent_id IS NOT NULL;

-- 2. `word_marks`: what the user said about a word that has no card ("I know this", "ignore
--    this"), per language (primary subtag) and lemma. Words with cards take their status from
--    FSRS data (fc_core::reading::word_status), never from here.
CREATE TABLE word_marks (
    language    TEXT NOT NULL,
    lemma       TEXT NOT NULL,
    status      TEXT NOT NULL CHECK (status IN ('known', 'ignored')),
    updated_at  INTEGER NOT NULL,
    PRIMARY KEY (language, lemma)
) STRICT, WITHOUT ROWID;
