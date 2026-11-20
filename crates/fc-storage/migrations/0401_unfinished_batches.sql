-- v0.5: undo groups a job writes across several transactions (chunked imports). The marker
-- row is written before the first part; each part's inverse calls are appended in that
-- part's own transaction, so the rows always describe exactly what is committed. Both are
-- removed in the transaction that writes the group to the oplog (or discards it). A marker
-- found at startup means the app stopped mid-import: `rollback` (an internal command call,
-- JSON InvokeRequest, given the parts' inverse calls newest first as `params.calls`) removes
-- what the committed parts wrote, so no change is left without an undo step.
CREATE TABLE unfinished_batches (
    batch_id    TEXT PRIMARY KEY NOT NULL,
    job_id      TEXT,
    state       TEXT NOT NULL CHECK (state IN ('started', 'writing')),
    rollback    TEXT NOT NULL CHECK (json_valid(rollback)),
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
) STRICT;

CREATE TABLE unfinished_batch_parts (
    batch_id    TEXT NOT NULL REFERENCES unfinished_batches(batch_id) ON DELETE CASCADE,
    part        INTEGER NOT NULL,
    inverse     TEXT NOT NULL CHECK (json_valid(inverse)),  -- [[InvokeRequest]] per oplog entry
    PRIMARY KEY (batch_id, part)
) STRICT;
