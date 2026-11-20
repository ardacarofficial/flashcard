-- 1071_session_queue (v1.0 performance): a study session's queue as rows.
--
-- * session_queue: the queue was a JSON array inside `sessions.state`, so every answer parsed
--   and rewrote the whole queue (tens of thousands of entries with many learning cards). An
--   answer now deletes/inserts only the entries it changed; `seq` orders the entries (gaps
--   leave room for insertions). Existing queues move here; `sessions.state` keeps the rest
--   (counters, mistakes, trial states, mode state) with an empty `queue`.
-- * cards_new_order: the first new cards of a session in creation order, read in order
--   instead of sorting every new card of the collection.
CREATE TABLE session_queue (
    session_id  TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    seq         INTEGER NOT NULL,
    card_id     TEXT NOT NULL,
    note_id     TEXT NOT NULL,
    kind        TEXT NOT NULL CHECK (kind IN ('learning', 'review', 'new')),
    due         INTEGER,
    PRIMARY KEY (session_id, seq)
) STRICT, WITHOUT ROWID;

INSERT INTO session_queue (session_id, seq, card_id, note_id, kind, due)
SELECT s.id, (q.key + 1) * 1048576, json_extract(q.value, '$.card_id'),
       json_extract(q.value, '$.note_id'), json_extract(q.value, '$.kind'),
       json_extract(q.value, '$.due')
FROM sessions s, json_each(s.state, '$.queue') q;

UPDATE sessions SET state = json_set(state, '$.queue', json('[]'))
WHERE json_type(state, '$.queue') = 'array' AND json_array_length(state, '$.queue') > 0;

CREATE INDEX cards_new_order ON cards(created_at, id)
    WHERE state = 'new' AND suspended = 0 AND deleted_at IS NULL;
