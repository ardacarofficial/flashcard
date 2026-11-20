-- v0.9: automation rules (docs/02 §23) — "when (trigger) + if (query) + do (commands)" — and
-- their run log. Rules are user data (soft delete, in dumps and backups); the log is a ledger
-- the app trims to the last runs.
CREATE TABLE automation_rules (
    id           TEXT PRIMARY KEY NOT NULL,
    name         TEXT NOT NULL,
    enabled      INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    position     INTEGER NOT NULL DEFAULT 0,
    -- `fc_core::automation::RuleTrigger` as JSON (`{"kind": …}`).
    trigger      TEXT NOT NULL CHECK (json_valid(trigger)),
    -- Query in canonical form (classification keys/options by id); '' = always.
    condition    TEXT NOT NULL DEFAULT '',
    -- `[{"method": …, "params": {…}}]`.
    actions      TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(actions)),
    last_run_at  INTEGER,
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL,
    deleted_at   INTEGER
) STRICT;

CREATE INDEX automation_rules_live ON automation_rules (position, created_at)
    WHERE deleted_at IS NULL;

CREATE TABLE automation_runs (
    seq          INTEGER PRIMARY KEY,
    rule_id      TEXT NOT NULL,
    at           INTEGER NOT NULL,
    trigger      TEXT NOT NULL,
    depth        INTEGER NOT NULL DEFAULT 0,
    matched      INTEGER NOT NULL DEFAULT 0,
    status       TEXT NOT NULL,
    batch_id     TEXT,
    error        TEXT CHECK (error IS NULL OR json_valid(error))
) STRICT;

CREATE INDEX automation_runs_rule ON automation_runs (rule_id, seq);
