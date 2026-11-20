-- v0.9: inbox items gain a state-change time, the notes they became, and the local API
-- client that sent them (with its idempotency key); local HTTP API clients (only the token's
-- SHA-256 hash is stored — the token itself never is).
ALTER TABLE inbox ADD COLUMN updated_at INTEGER NOT NULL DEFAULT 0;
ALTER TABLE inbox ADD COLUMN note_ids TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(note_ids));
ALTER TABLE inbox ADD COLUMN client_id TEXT;
ALTER TABLE inbox ADD COLUMN client_ref TEXT;
UPDATE inbox SET updated_at = created_at;
CREATE INDEX inbox_status_created ON inbox (status, created_at DESC, id DESC);
CREATE UNIQUE INDEX inbox_client_ref ON inbox (client_id, client_ref)
    WHERE client_id IS NOT NULL AND client_ref IS NOT NULL;

CREATE TABLE api_clients (
    id               TEXT PRIMARY KEY NOT NULL,
    name             TEXT NOT NULL,
    kind             TEXT NOT NULL,
    origin           TEXT,
    requested_scopes TEXT NOT NULL CHECK (json_valid(requested_scopes)),
    scopes           TEXT NOT NULL CHECK (json_valid(scopes)),
    status           TEXT NOT NULL CHECK (status IN ('pending', 'approved', 'denied', 'revoked')),
    token_hash       TEXT NOT NULL,
    created_at       INTEGER NOT NULL,
    updated_at       INTEGER NOT NULL,
    last_used_at     INTEGER
) STRICT;
CREATE UNIQUE INDEX api_clients_token ON api_clients (token_hash);
