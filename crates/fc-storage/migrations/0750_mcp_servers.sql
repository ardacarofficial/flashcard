-- v0.8: external MCP servers the app connects to as a client (docs/06 §2.2). No environment
-- variables / secrets here: API keys live only in the OS keychain (CLAUDE.md rule 10).
CREATE TABLE mcp_servers (
    id          TEXT PRIMARY KEY NOT NULL,
    name        TEXT NOT NULL,
    transport   TEXT NOT NULL CHECK (transport IN ('stdio', 'http')),
    command     TEXT NOT NULL DEFAULT '',
    args        TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(args)),
    url         TEXT,
    enabled     INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
) STRICT;

CREATE UNIQUE INDEX mcp_servers_name ON mcp_servers (lower(name));
