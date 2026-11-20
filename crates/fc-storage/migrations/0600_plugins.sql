-- v0.7 plugins: run state on the v0.1 `plugins` row, and installed packages.
--
-- `plugins` (0001) keeps one row per installed plugin. Uninstalling soft-deletes it
-- (`deleted_at`) so undo/redo never needs the package bytes in the oplog; `package_sha256`
-- points at the active package. `failures`/`last_error`/`auto_disabled` implement automatic
-- disabling after repeated failures (docs/05 §7).
ALTER TABLE plugins ADD COLUMN package_sha256 TEXT NOT NULL DEFAULT '';
ALTER TABLE plugins ADD COLUMN failures INTEGER NOT NULL DEFAULT 0 CHECK (failures >= 0);
ALTER TABLE plugins ADD COLUMN last_error TEXT;
ALTER TABLE plugins ADD COLUMN auto_disabled INTEGER NOT NULL DEFAULT 0 CHECK (auto_disabled IN (0, 1));
ALTER TABLE plugins ADD COLUMN deleted_at INTEGER;

-- Every package version ever installed (content-addressed by the WASM's SHA-256): undoing
-- an update or an uninstall switches `plugins.package_sha256` back without re-reading files.
CREATE TABLE plugin_packages (
    plugin_id   TEXT NOT NULL,
    sha256      TEXT NOT NULL CHECK (length(sha256) = 64),
    version     TEXT NOT NULL,
    manifest    TEXT NOT NULL CHECK (json_valid(manifest)),
    wasm        BLOB NOT NULL,
    settings    TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(settings)),
    locales     TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(locales)),
    created_at  INTEGER NOT NULL,
    PRIMARY KEY (plugin_id, sha256)
) STRICT;
