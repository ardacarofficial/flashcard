-- 1430_subtitle_capture_log (v1.4 A5): "what happened to my last capture?"
--
-- One row per capture attempt the Subtitle extension made (`POST /subtitle/v1/captures`, each
-- item of a batch): when, the result code (`created`, `duplicate`, `rejected`,
-- `media_missing`), a machine reason (an error code + field name, or which media and why),
-- the matched card and how long it took. Privacy: never the sentence, the word, a URL or a
-- video title — only ids, codes and times. Diagnostics, not user content: written straight
-- (no oplog), kept bounded (the app deletes all but the newest rows), not in backups.
CREATE TABLE subtitle_capture_log (
    seq         INTEGER PRIMARY KEY AUTOINCREMENT,
    capture_id  TEXT NOT NULL,
    at          INTEGER NOT NULL,
    result      TEXT NOT NULL
                CHECK (result IN ('created', 'duplicate', 'rejected', 'media_missing')),
    reason      TEXT,
    card_id     TEXT,
    duration_ms INTEGER NOT NULL
) STRICT;
CREATE INDEX subtitle_capture_log_capture ON subtitle_capture_log (capture_id);
