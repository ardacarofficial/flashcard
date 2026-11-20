-- v0.6: review history outlives its card.
--
-- 0001 declared `review_log.card_id REFERENCES cards(id) ON DELETE CASCADE`, so purging a
-- trashed card for good erased its reviews — and with them heatmap days, streaks, totals and
-- FSRS optimizer data. `review_log` is the immutable source of statistics (docs/03 §4), so it
-- is rebuilt without the foreign key: `card_id` stays a plain column (+ the same indices) and
-- may point to a card that no longer exists.
--
-- SQLite cannot drop a constraint, hence the copy. Rowids are kept, so the statistics cache
-- (`stats_cache_meta.upto_seq`) stays meaningful; it is cleared anyway (derived data, rebuilt
-- on the next refresh), so the generation counter need not move. The
-- 0500 generation triggers and the 0100 session index go with the old table and are
-- recreated. Nothing references `review_log`, so the swap is safe with foreign keys on.

CREATE TABLE review_log_new (
    id                  TEXT PRIMARY KEY NOT NULL,
    card_id             TEXT NOT NULL,          -- no FK: history survives a purged card
    session_id          TEXT,
    rating              TEXT NOT NULL CHECK (rating IN ('again', 'hard', 'good', 'easy')),
    state_before        TEXT NOT NULL,
    state_after         TEXT NOT NULL,
    scheduled_days      INTEGER NOT NULL,
    elapsed_days        INTEGER NOT NULL,
    duration_ms         INTEGER NOT NULL,
    answer_duration_ms  INTEGER,
    hints_used          INTEGER NOT NULL DEFAULT 0,
    mode                TEXT NOT NULL DEFAULT 'normal',
    stability_after     REAL,
    difficulty_after    REAL,
    created_at          INTEGER NOT NULL
) STRICT;

INSERT INTO review_log_new (rowid, id, card_id, session_id, rating, state_before, state_after,
                            scheduled_days, elapsed_days, duration_ms, answer_duration_ms,
                            hints_used, mode, stability_after, difficulty_after, created_at)
SELECT rowid, id, card_id, session_id, rating, state_before, state_after,
       scheduled_days, elapsed_days, duration_ms, answer_duration_ms,
       hints_used, mode, stability_after, difficulty_after, created_at
FROM review_log;

DROP TABLE review_log;
ALTER TABLE review_log_new RENAME TO review_log;

CREATE INDEX review_log_card_time ON review_log(card_id, created_at);
CREATE INDEX review_log_time ON review_log(created_at);
CREATE INDEX review_log_session ON review_log(session_id) WHERE session_id IS NOT NULL;

CREATE TRIGGER review_log_generation_on_delete AFTER DELETE ON review_log
BEGIN
    UPDATE review_log_generation SET n = n + 1 WHERE id = 1;
END;

CREATE TRIGGER review_log_generation_on_update AFTER UPDATE ON review_log
BEGIN
    UPDATE review_log_generation SET n = n + 1 WHERE id = 1;
END;

DELETE FROM stats_day_cache;
DELETE FROM stats_cache_meta;
