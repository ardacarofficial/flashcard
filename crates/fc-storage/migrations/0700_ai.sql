-- v0.8 AI (docs/06 §1). API keys are never stored here: they live in the OS keychain under
-- the account `ai.provider.<id>`.

-- Configured providers (several may exist; one is the default).
CREATE TABLE ai_providers (
    id             TEXT PRIMARY KEY NOT NULL,
    kind           TEXT NOT NULL
                   CHECK (kind IN ('anthropic', 'openai', 'openai_compatible', 'ollama')),
    name           TEXT NOT NULL,
    base_url       TEXT NOT NULL,
    default_model  TEXT NOT NULL DEFAULT '',
    models         TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(models)),  -- fetched or typed in
    is_default     INTEGER NOT NULL DEFAULT 0 CHECK (is_default IN (0, 1)),
    position       INTEGER NOT NULL DEFAULT 0,
    created_at     INTEGER NOT NULL,
    updated_at     INTEGER NOT NULL,
    deleted_at     INTEGER
) STRICT;

-- Per-feature provider/model choice (no row = the default provider and its model).
CREATE TABLE ai_routes (
    feature      TEXT PRIMARY KEY NOT NULL,
    provider_id  TEXT,
    model        TEXT,
    updated_at   INTEGER NOT NULL
) STRICT;

-- Price table for cost estimates, USD per million tokens. Editable: providers change prices.
-- A model without a row is estimated at 0 (local models).
CREATE TABLE ai_prices (
    model            TEXT PRIMARY KEY NOT NULL,
    input_per_mtok   REAL NOT NULL CHECK (input_per_mtok >= 0),
    output_per_mtok  REAL NOT NULL CHECK (output_per_mtok >= 0),
    updated_at       INTEGER NOT NULL
) STRICT;

INSERT INTO ai_prices (model, input_per_mtok, output_per_mtok, updated_at) VALUES
    ('claude-opus-5-5', 5.0, 25.0, 0),
    ('claude-sonnet-5-5', 3.0, 15.0, 0),
    ('claude-haiku-5-5', 1.0, 5.0, 0);

-- Usage rows are read by time range and summed per feature/model (`ai_usage_time` exists).
CREATE INDEX ai_usage_feature ON ai_usage(feature, created_at);
