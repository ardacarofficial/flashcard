-- v1.0 (first run, reminders; docs/02 §19, §32): small per-collection bookkeeping that is
-- not user content and is never undone — "the first-run guide was finished", "the last day a
-- study reminder was shown". Key → JSON value. Profiles and the app lock live outside every
-- collection (the profile registry), not here.
CREATE TABLE ux_state (
    key        TEXT PRIMARY KEY NOT NULL,
    value      TEXT NOT NULL CHECK (json_valid(value)),
    updated_at INTEGER NOT NULL
) STRICT;
