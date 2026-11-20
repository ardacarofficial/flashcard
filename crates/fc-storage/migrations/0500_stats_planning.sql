-- v0.6 statistics & smart planning.
--
-- 1. Statistics cache: per-study-day review aggregates (`fc_core::stats::DayAggregate`, JSON)
--    of the whole collection for one study calendar ("<tz offset mins>:<rollover hour>").
--    Derived data, always reproducible from `review_log`: the app folds rows appended after
--    `upto_seq` (review_log rowid) on read and rebuilds when `generation` or `row_count` no
--    longer match (rows changed or disappeared: undo, purge, restore).
CREATE TABLE stats_day_cache (
    calendar  TEXT NOT NULL,
    day       INTEGER NOT NULL,
    data      TEXT NOT NULL CHECK (json_valid(data)),
    PRIMARY KEY (calendar, day)
) STRICT;

CREATE TABLE stats_cache_meta (
    calendar    TEXT PRIMARY KEY NOT NULL,
    upto_seq    INTEGER NOT NULL,   -- highest review_log rowid folded in
    row_count   INTEGER NOT NULL,   -- review_log rows (any mode) with rowid <= upto_seq then
    generation  INTEGER NOT NULL,   -- review_log_generation.n then
    updated_at  INTEGER NOT NULL
) STRICT;

-- Bumped whenever an existing review_log row changes or disappears; appends do not bump it
-- (they get higher rowids and are folded incrementally).
CREATE TABLE review_log_generation (
    id  INTEGER PRIMARY KEY CHECK (id = 1),
    n   INTEGER NOT NULL
) STRICT;
INSERT INTO review_log_generation (id, n) VALUES (1, 0);

CREATE TRIGGER review_log_generation_on_delete AFTER DELETE ON review_log
BEGIN
    UPDATE review_log_generation SET n = n + 1 WHERE id = 1;
END;

CREATE TRIGGER review_log_generation_on_update AFTER UPDATE ON review_log
BEGIN
    UPDATE review_log_generation SET n = n + 1 WHERE id = 1;
END;

-- 2. Exam-date mode: an exam on a local date for a deck subtree and/or a query (canonical
--    v0.4 query text, e.g. a classification value `attr_<id>:<option id>`).
CREATE TABLE exam_dates (
    id          TEXT PRIMARY KEY NOT NULL,
    name        TEXT NOT NULL,
    date        TEXT NOT NULL,       -- YYYY-MM-DD, local
    deck_id     TEXT REFERENCES decks(id) ON DELETE CASCADE,
    query       TEXT,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
) STRICT;
CREATE INDEX exam_dates_date ON exam_dates(date);

-- 3. Goals use the `goals` table of 0001 (kind, target, deadline, query) as is.
