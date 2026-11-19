-- 0001_init: initial schema (docs/03 §4).
--
-- Conventions
--   * ids: UUIDv7 as canonical lowercase hyphenated TEXT (36 chars).
--   * times: INTEGER, UTC milliseconds since the Unix epoch.
--   * JSON: TEXT containing JSON (validated by the app; json_valid() checks where cheap).
--   * soft delete: deleted_at IS NOT NULL = in trash.
--   * STRICT tables; foreign keys ON (set per connection).
-- Migration numbering: v0.1 = 0001-0099, v0.2 = 0100-0199, v0.3 = 0200-0299, ...

-- ---------------------------------------------------------------- presets
CREATE TABLE presets (
    id          TEXT PRIMARY KEY NOT NULL,
    name        TEXT NOT NULL,
    config      TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(config)),
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
) STRICT;

-- ---------------------------------------------------------------- decks
CREATE TABLE decks (
    id          TEXT PRIMARY KEY NOT NULL,
    parent_id   TEXT REFERENCES decks(id),
    name        TEXT NOT NULL CHECK (length(name) > 0 AND instr(name, '::') = 0),
    description TEXT NOT NULL DEFAULT '',
    icon        TEXT,
    color       TEXT,
    preset_id   TEXT REFERENCES presets(id),
    position    INTEGER NOT NULL DEFAULT 0,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    deleted_at  INTEGER
) STRICT;
CREATE INDEX decks_parent ON decks(parent_id);
-- Sibling names are unique among live decks (ASCII case-insensitive; the app also checks
-- Unicode case-insensitively).
CREATE UNIQUE INDEX decks_sibling_name ON decks(coalesce(parent_id, ''), name COLLATE NOCASE)
    WHERE deleted_at IS NULL;

-- ---------------------------------------------------------------- note types
CREATE TABLE notetypes (
    id          TEXT PRIMARY KEY NOT NULL,
    name        TEXT NOT NULL,
    kind        TEXT NOT NULL CHECK (kind IN ('standard', 'cloze')),
    fields      TEXT NOT NULL CHECK (json_valid(fields)),     -- [FieldDef]
    templates   TEXT NOT NULL CHECK (json_valid(templates)),  -- [CardTemplate]
    css         TEXT NOT NULL DEFAULT '',
    sort_field  TEXT NOT NULL,
    version     INTEGER NOT NULL DEFAULT 1,
    builtin     TEXT,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL,
    deleted_at  INTEGER
) STRICT;
CREATE UNIQUE INDEX notetypes_builtin ON notetypes(builtin) WHERE builtin IS NOT NULL;

-- ---------------------------------------------------------------- notes
-- home_deck_id: the deck whose classification definitions apply to the note (note-level
-- attributes); card-level attributes resolve from each card's own deck.
-- fts_rowid: rowid of the note's notes_fts row. notes_fts.note_id is UNINDEXED, so this link
-- makes FTS updates O(log n). Stored explicitly (notes.rowid may be renumbered by VACUUM).
CREATE TABLE notes (
    id           TEXT PRIMARY KEY NOT NULL,
    notetype_id  TEXT NOT NULL REFERENCES notetypes(id),
    home_deck_id TEXT NOT NULL REFERENCES decks(id),
    fields       TEXT NOT NULL CHECK (json_valid(fields)),    -- {field_key: value}
    sort_field   TEXT NOT NULL DEFAULT '',
    checksum     TEXT NOT NULL,                               -- fc_core::note::note_checksum
    created_at   INTEGER NOT NULL,
    updated_at   INTEGER NOT NULL,
    deleted_at   INTEGER,
    fts_rowid    INTEGER
) STRICT;
CREATE INDEX notes_notetype ON notes(notetype_id);
CREATE INDEX notes_home_deck ON notes(home_deck_id);
CREATE UNIQUE INDEX notes_fts_rowid ON notes(fts_rowid) WHERE fts_rowid IS NOT NULL;
CREATE INDEX notes_checksum ON notes(notetype_id, checksum) WHERE deleted_at IS NULL;
CREATE INDEX notes_updated ON notes(updated_at);

-- ---------------------------------------------------------------- cards
CREATE TABLE cards (
    id            TEXT PRIMARY KEY NOT NULL,
    note_id       TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
    deck_id       TEXT NOT NULL REFERENCES decks(id),
    template_key  TEXT NOT NULL,
    card_type     TEXT NOT NULL,
    state         TEXT NOT NULL DEFAULT 'new'
                  CHECK (state IN ('new', 'learning', 'review', 'relearning')),
    due           INTEGER,
    stability     REAL,
    difficulty    REAL,
    reps          INTEGER NOT NULL DEFAULT 0,
    lapses        INTEGER NOT NULL DEFAULT 0,
    last_review   INTEGER,
    flags         INTEGER NOT NULL DEFAULT 0,
    suspended     INTEGER NOT NULL DEFAULT 0 CHECK (suspended IN (0, 1)),
    buried_until  INTEGER,
    user_note     TEXT,
    created_at    INTEGER NOT NULL,
    updated_at    INTEGER NOT NULL,
    deleted_at    INTEGER
) STRICT;
CREATE INDEX cards_deck_due ON cards(deck_id, due);
CREATE INDEX cards_note ON cards(note_id);
CREATE UNIQUE INDEX cards_note_template ON cards(note_id, template_key);
-- Study queue: scheduled cards that can come due.
CREATE INDEX cards_due_active ON cards(due)
    WHERE state IN ('learning', 'review', 'relearning') AND suspended = 0
      AND deleted_at IS NULL;

-- ---------------------------------------------------------------- review log (immutable)
CREATE TABLE review_log (
    id                  TEXT PRIMARY KEY NOT NULL,
    card_id             TEXT NOT NULL REFERENCES cards(id) ON DELETE CASCADE,
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
CREATE INDEX review_log_card_time ON review_log(card_id, created_at);
CREATE INDEX review_log_time ON review_log(created_at);

-- ---------------------------------------------------------------- oplog (undo + audit)
CREATE TABLE oplog (
    id                TEXT PRIMARY KEY NOT NULL,
    batch_id          TEXT NOT NULL,
    seq               INTEGER NOT NULL,                     -- order within the batch
    command           TEXT NOT NULL,                        -- qualified method name
    payload           TEXT NOT NULL CHECK (json_valid(payload)),
    inverse           TEXT NOT NULL CHECK (json_valid(inverse)), -- [InvokeRequest]
    actor             TEXT NOT NULL,                        -- JSON Actor
    kind              TEXT NOT NULL CHECK (kind IN ('do', 'undo', 'redo')),
    reverts_batch_id  TEXT,
    summary           TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(summary)), -- I18nMessage
    created_at        INTEGER NOT NULL
) STRICT;
CREATE INDEX oplog_batch ON oplog(batch_id, seq);
CREATE INDEX oplog_time ON oplog(created_at);
CREATE INDEX oplog_reverts ON oplog(reverts_batch_id) WHERE reverts_batch_id IS NOT NULL;

-- ---------------------------------------------------------------- tags
CREATE TABLE tags (
    id          TEXT PRIMARY KEY NOT NULL,
    name        TEXT NOT NULL,
    color       TEXT,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
) STRICT;
CREATE UNIQUE INDEX tags_name ON tags(name COLLATE NOCASE);

CREATE TABLE note_tags (
    note_id  TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
    tag_id   TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    position INTEGER NOT NULL DEFAULT 0,                   -- keeps the user's tag order
    PRIMARY KEY (note_id, tag_id)
) STRICT, WITHOUT ROWID;
CREATE INDEX note_tags_tag ON note_tags(tag_id);

-- ---------------------------------------------------------------- classifications (02 §8.1)
CREATE TABLE attribute_defs (
    id             TEXT PRIMARY KEY NOT NULL,
    scope_deck_id  TEXT REFERENCES decks(id),              -- NULL = whole collection
    key            TEXT NOT NULL,
    name           TEXT NOT NULL,
    kind           TEXT NOT NULL CHECK (kind IN
                     ('ordered_enum', 'enum', 'multi_enum', 'number', 'text', 'date', 'bool')),
    options        TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(options)), -- [{id,label,color,order}]
    target         TEXT NOT NULL DEFAULT 'note' CHECK (target IN ('note', 'card')),
    required       INTEGER NOT NULL DEFAULT 0 CHECK (required IN (0, 1)),
    default_value  TEXT CHECK (default_value IS NULL OR json_valid(default_value)), -- AttributeValue JSON
    position       INTEGER NOT NULL DEFAULT 0,
    template_id    TEXT,
    created_at     INTEGER NOT NULL,
    updated_at     INTEGER NOT NULL,
    deleted_at     INTEGER
) STRICT;
CREATE UNIQUE INDEX attribute_defs_scope_key ON attribute_defs(coalesce(scope_deck_id, ''), key)
    WHERE deleted_at IS NULL;

-- One row per value; multi_enum uses one row per selected option, hence `value` is part of
-- the key. Representation: fc_core::attribute::AttributeValue::to_stored().
CREATE TABLE attribute_values (
    attr_id      TEXT NOT NULL REFERENCES attribute_defs(id) ON DELETE CASCADE,
    entity_kind  TEXT NOT NULL CHECK (entity_kind IN ('note', 'card')),
    entity_id    TEXT NOT NULL,
    value        TEXT NOT NULL,
    value_num    REAL,
    PRIMARY KEY (attr_id, entity_kind, entity_id, value)
) STRICT, WITHOUT ROWID;
CREATE INDEX attribute_values_entity ON attribute_values(entity_kind, entity_id);
CREATE INDEX attribute_values_value ON attribute_values(attr_id, value);
-- Range queries on number/bool attributes (`difficulty>3`).
CREATE INDEX attribute_values_num ON attribute_values(attr_id, value_num);

-- ---------------------------------------------------------------- media
CREATE TABLE media (
    hash           TEXT PRIMARY KEY NOT NULL,              -- sha256 hex; file at media/<ab>/<hash>
    mime           TEXT NOT NULL,
    size           INTEGER NOT NULL,
    original_name  TEXT,
    ref_count      INTEGER NOT NULL DEFAULT 0,
    created_at     INTEGER NOT NULL
) STRICT;

-- ---------------------------------------------------------------- links between notes
CREATE TABLE links (
    from_note   TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
    to_note     TEXT NOT NULL REFERENCES notes(id) ON DELETE CASCADE,
    label       TEXT NOT NULL DEFAULT '',
    created_at  INTEGER NOT NULL,
    PRIMARY KEY (from_note, to_note, label)
) STRICT, WITHOUT ROWID;
CREATE INDEX links_to ON links(to_note);

-- ---------------------------------------------------------------- saved queries
CREATE TABLE saved_queries (
    id          TEXT PRIMARY KEY NOT NULL,
    name        TEXT NOT NULL,
    query       TEXT NOT NULL,
    icon        TEXT,
    position    INTEGER NOT NULL DEFAULT 0,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
) STRICT;

-- ---------------------------------------------------------------- study sessions
CREATE TABLE sessions (
    id          TEXT PRIMARY KEY NOT NULL,
    kind        TEXT NOT NULL,
    query       TEXT,
    config      TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(config)),
    state       TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(state)),
    started_at  INTEGER NOT NULL,
    ended_at    INTEGER
) STRICT;

-- ---------------------------------------------------------------- settings
-- scope: fc_core::settings::SettingScope::storage_key() ('app', 'deck:<id>', ...)
CREATE TABLE settings (
    scope       TEXT NOT NULL,
    key         TEXT NOT NULL,
    value       TEXT NOT NULL CHECK (json_valid(value)),
    updated_at  INTEGER NOT NULL,
    PRIMARY KEY (scope, key)
) STRICT, WITHOUT ROWID;

-- ---------------------------------------------------------------- module switches
CREATE TABLE module_state (
    id          TEXT PRIMARY KEY NOT NULL,
    enabled     INTEGER NOT NULL CHECK (enabled IN (0, 1)),
    updated_at  INTEGER NOT NULL
) STRICT;

-- ---------------------------------------------------------------- background jobs
CREATE TABLE jobs (
    id          TEXT PRIMARY KEY NOT NULL,
    kind        TEXT NOT NULL,
    status      TEXT NOT NULL CHECK (status IN
                  ('queued', 'running', 'succeeded', 'failed', 'cancelled', 'interrupted')),
    progress    REAL,
    payload     TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(payload)),
    result      TEXT CHECK (result IS NULL OR json_valid(result)),
    error       TEXT CHECK (error IS NULL OR json_valid(error)),  -- ApiError JSON
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
) STRICT;
CREATE INDEX jobs_status ON jobs(status);

-- ---------------------------------------------------------------- inbox (external sources)
CREATE TABLE inbox (
    id          TEXT PRIMARY KEY NOT NULL,
    source      TEXT NOT NULL,
    payload     TEXT NOT NULL CHECK (json_valid(payload)),
    status      TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'accepted', 'rejected')),
    created_at  INTEGER NOT NULL
) STRICT;

-- ---------------------------------------------------------------- automations
CREATE TABLE automations (
    id          TEXT PRIMARY KEY NOT NULL,
    name        TEXT NOT NULL,
    trigger     TEXT NOT NULL CHECK (json_valid(trigger)),
    condition   TEXT,                                       -- query language
    actions     TEXT NOT NULL CHECK (json_valid(actions)),
    enabled     INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
) STRICT;

-- ---------------------------------------------------------------- plugins
CREATE TABLE plugins (
    id                   TEXT PRIMARY KEY NOT NULL,
    version              TEXT NOT NULL,
    manifest             TEXT NOT NULL CHECK (json_valid(manifest)),
    enabled              INTEGER NOT NULL DEFAULT 1 CHECK (enabled IN (0, 1)),
    granted_permissions  TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(granted_permissions)),
    installed_at         INTEGER NOT NULL,
    updated_at           INTEGER NOT NULL
) STRICT;

CREATE TABLE plugin_kv (
    plugin_id   TEXT NOT NULL REFERENCES plugins(id) ON DELETE CASCADE,
    key         TEXT NOT NULL,
    value       TEXT NOT NULL,
    updated_at  INTEGER NOT NULL,
    PRIMARY KEY (plugin_id, key)
) STRICT, WITHOUT ROWID;

-- ---------------------------------------------------------------- goals
CREATE TABLE goals (
    id          TEXT PRIMARY KEY NOT NULL,
    kind        TEXT NOT NULL,
    target      REAL NOT NULL,
    deadline    INTEGER,
    query       TEXT,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
) STRICT;

-- ---------------------------------------------------------------- AI usage accounting
CREATE TABLE ai_usage (
    id             TEXT PRIMARY KEY NOT NULL,
    provider       TEXT NOT NULL,
    model          TEXT NOT NULL,
    feature        TEXT NOT NULL,
    input_tokens   INTEGER NOT NULL,
    output_tokens  INTEGER NOT NULL,
    cost_estimate  REAL,
    created_at     INTEGER NOT NULL
) STRICT;
CREATE INDEX ai_usage_time ON ai_usage(created_at);

-- ---------------------------------------------------------------- full-text search
-- Maintained by the app (not triggers): content = fc_core::note::searchable_text().
CREATE VIRTUAL TABLE notes_fts USING fts5(
    note_id UNINDEXED,
    sort_field,
    content,
    tokenize = 'unicode61 remove_diacritics 2'
);
