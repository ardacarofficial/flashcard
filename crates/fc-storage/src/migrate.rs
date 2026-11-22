//! Schema migrations.
//!
//! * Files `migrations/NNNN_name.sql` are collected automatically (`build.rs`); adding a
//!   migration = adding its file.
//! * Numbering is reserved per release track so parallel branches never collide (docs/07 §5,
//!   [`RELEASE_RANGES`]): v0.1 = 1–99, v0.2 = 100–199, v0.3 = 200–299, v0.4 = 300–399,
//!   v0.5 = 400–499, … Gaps are fine; order is by number. A migration is **never edited after
//!   merge**.
//! * Applied migrations are recorded in `schema_migrations (version, name, checksum,
//!   applied_at)`. The checksum (FNV-1a 64 of the SQL with `\r` removed, so CRLF checkouts
//!   agree) guards against an edited migration: a mismatch refuses to open the database.
//! * `PRAGMA user_version` mirrors the highest applied version (cheap check, and lets
//!   databases created before `schema_migrations` existed be adopted: their rows are
//!   back-filled).
//! * Each migration runs in its own `BEGIN IMMEDIATE` transaction. Missing migrations are
//!   applied in ascending order, even if a higher one is already applied (lane merges).

use crate::error::{in_table, StorageError, StorageResult};
use fc_core::note::fnv1a64;
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use std::collections::BTreeMap;

/// One migration.
#[derive(Debug, Clone, Copy)]
pub struct Migration {
    /// Version number (`0001_init.sql` → 1).
    pub version: u32,
    /// Name (file stem).
    pub name: &'static str,
    /// SQL batch.
    pub sql: &'static str,
}

impl Migration {
    /// Checksum recorded in `schema_migrations` (16 hex chars).
    pub fn checksum(&self) -> String {
        checksum(self.sql)
    }
}

/// Checksum of a migration's SQL: FNV-1a 64 over the text without `\r`.
pub fn checksum(sql: &str) -> String {
    let normalized: String = sql.chars().filter(|c| *c != '\r').collect();
    format!("{:016x}", fnv1a64(normalized.as_bytes()))
}

// `MIGRATIONS`: every `migrations/NNNN_name.sql`, ascending, collected by `build.rs` (a track
// adds a migration by adding its file).
include!(concat!(env!("OUT_DIR"), "/migrations.rs"));

/// Migration number range reserved for each release track (docs/07 §5), so parallel branches
/// never collide. A migration outside every range fails the tests.
pub const RELEASE_RANGES: &[(&str, std::ops::RangeInclusive<u32>)] = &[
    ("v0.1", 1..=99),
    ("v0.2", 100..=199),
    ("v0.3", 200..=299),
    ("v0.4", 300..=399),
    ("v0.5", 400..=499),
    ("v0.6", 500..=599),
    ("v0.7", 600..=699),
    ("v0.8", 700..=799),
    ("v0.9", 800..=899),
    ("v0.10", 900..=999),
    ("v1.0", 1000..=1099),
    // v1.1 (the interface redesign) has no migration.
    ("v1.1", 1100..=1199),
    ("v1.2", 1200..=1299),
    // v1.3 ("usable"): 1300–1329 vocabulary field roles (R2); the rest by worker brief.
    ("v1.3", 1300..=1399),
    // v1.4: 1410–1429 word index (A3), 1430–1439 subtitle capture diagnostics (A5); the rest
    // by worker brief.
    ("v1.4", 1400..=1499),
];

/// Lanes inside the v1.0 range (docs/07 §5, docs/10 §35): the v1.0 tracks ran in parallel, so
/// each owns a slice; the second v1.0 round (perf/i18n/themes) owns `1070`–`1089`.
pub const V1_LANE_RANGES: &[(&str, std::ops::RangeInclusive<u32>)] = &[
    ("v1.0 ux", 1000..=1049),
    ("v1.0 release", 1050..=1069),
    ("v1.0 perf", 1070..=1089),
    ("v1.0 quality", 1090..=1099),
];

/// v1.0 lane that owns migration number `version` (`None` = not in any v1.0 lane).
pub fn v1_lane_of(version: u32) -> Option<&'static str> {
    V1_LANE_RANGES
        .iter()
        .find(|(_, r)| r.contains(&version))
        .map(|(name, _)| *name)
}

/// Release track that owns migration number `version` (`None` = outside every range).
pub fn release_of(version: u32) -> Option<&'static str> {
    RELEASE_RANGES
        .iter()
        .find(|(_, r)| r.contains(&version))
        .map(|(name, _)| *name)
}

/// Highest known schema version.
pub fn latest_version() -> u32 {
    MIGRATIONS.last().map_or(0, |m| m.version)
}

/// Current schema version of `conn` (`PRAGMA user_version`).
pub fn current_version(conn: &Connection) -> StorageResult<u32> {
    let v: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    Ok(u32::try_from(v).unwrap_or(0))
}

/// Applies pending [`MIGRATIONS`]. Returns the versions applied (empty = up to date).
/// Fails with [`StorageError::TooNew`] if the database is ahead of this build, and with
/// [`StorageError::Migration`] on a checksum mismatch or an unknown recorded migration.
///
/// (Taking a file snapshot before migrating — docs/03 §15 — is done by
/// [`crate::SqliteStore::open`], which knows the path.)
pub fn migrate(conn: &mut Connection) -> StorageResult<Vec<u32>> {
    migrate_with(conn, MIGRATIONS)
}

/// [`migrate`] against an explicit migration list (ascending by version). For tests.
pub fn migrate_with(conn: &mut Connection, migrations: &[Migration]) -> StorageResult<Vec<u32>> {
    let pending = check(conn, migrations)?;
    let mut applied = Vec::new();
    for m in pending {
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        tx.execute_batch(m.sql)
            .map_err(|e| StorageError::Migration {
                version: m.version,
                message: e.to_string(),
            })?;
        tx.execute(
            "INSERT INTO schema_migrations (version, name, checksum, applied_at) \
             VALUES (?1, ?2, ?3, CAST((julianday('now') - 2440587.5) * 86400000 AS INTEGER))",
            params![m.version, m.name, m.checksum()],
        )?;
        let top: u32 = tx.query_row("SELECT max(version) FROM schema_migrations", [], |r| {
            r.get(0)
        })?;
        tx.pragma_update(None, "user_version", top)?;
        tx.commit()?;
        applied.push(m.version);
    }
    Ok(applied)
}

/// Versions of `MIGRATIONS` not yet applied to `conn` (after validating what is recorded).
pub fn pending_versions(conn: &mut Connection) -> StorageResult<Vec<u32>> {
    pending_versions_with(conn, MIGRATIONS)
}

/// [`pending_versions`] against an explicit migration list. For tests.
pub fn pending_versions_with(
    conn: &mut Connection,
    migrations: &[Migration],
) -> StorageResult<Vec<u32>> {
    Ok(check(conn, migrations)?
        .into_iter()
        .map(|m| m.version)
        .collect())
}

/// Ensures `schema_migrations` exists (back-filling it for legacy databases), validates the
/// recorded rows against `migrations` and returns the pending ones in order.
fn check<'m>(
    conn: &mut Connection,
    migrations: &'m [Migration],
) -> StorageResult<Vec<&'m Migration>> {
    let latest = migrations.last().map_or(0, |m| m.version);
    let current = current_version(conn)?;
    if current > latest {
        return Err(StorageError::TooNew {
            found: current,
            supported: latest,
        });
    }
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    tx.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version     INTEGER PRIMARY KEY NOT NULL,
            name        TEXT NOT NULL,
            checksum    TEXT NOT NULL,
            applied_at  INTEGER NOT NULL
        ) STRICT;",
    )?;
    let mut recorded = load_recorded(&tx)?;
    if recorded.is_empty() && current > 0 {
        // Database created before `schema_migrations` existed: adopt what user_version says.
        for m in migrations.iter().filter(|m| m.version <= current) {
            tx.execute(
                "INSERT INTO schema_migrations (version, name, checksum, applied_at) \
                 VALUES (?1, ?2, ?3, 0)",
                params![m.version, m.name, m.checksum()],
            )?;
        }
        recorded = load_recorded(&tx)?;
    }
    for (version, sum) in &recorded {
        let Some(m) = migrations.iter().find(|m| m.version == *version) else {
            if *version > latest {
                return Err(StorageError::TooNew {
                    found: *version,
                    supported: latest,
                });
            }
            return Err(StorageError::Migration {
                version: *version,
                message: "recorded migration is unknown to this build".to_owned(),
            });
        };
        if m.checksum() != *sum {
            return Err(StorageError::Migration {
                version: *version,
                message: format!(
                    "checksum mismatch for {}: database has {sum}, build has {} \
                     (migrations must never be edited after merge)",
                    m.name,
                    m.checksum()
                ),
            });
        }
    }
    tx.commit()?;
    Ok(migrations
        .iter()
        .filter(|m| !recorded.contains_key(&m.version))
        .collect())
}

fn load_recorded(conn: &Connection) -> StorageResult<BTreeMap<u32, String>> {
    let mut stmt = conn.prepare("SELECT version, checksum FROM schema_migrations")?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, u32>(0)?, r.get::<_, String>(1)?)))?
        .collect::<Result<BTreeMap<_, _>, _>>()
        .map_err(|e| in_table(e, "schema_migrations"))?;
    Ok(rows)
}

/// Whether `table` (or index/virtual table) exists in the schema.
pub fn schema_object_exists(conn: &Connection, name: &str) -> StorageResult<bool> {
    Ok(conn
        .query_row("SELECT 1 FROM sqlite_master WHERE name = ?1", [name], |r| {
            r.get::<_, i64>(0)
        })
        .optional()?
        .is_some())
}

/// Settings every writer connection uses: WAL, foreign keys, busy timeout, `synchronous =
/// NORMAL` (safe with WAL; a power cut may lose the last commits but never corrupts),
/// in-memory temp storage.
pub fn configure_connection(conn: &Connection) -> StorageResult<()> {
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "busy_timeout", BUSY_TIMEOUT_MS)?;
    // In-memory databases report "memory"; ignore the returned mode.
    let _mode: String = conn.pragma_update_and_check(None, "journal_mode", "WAL", |r| r.get(0))?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "temp_store", "MEMORY")?;
    conn.pragma_update(None, "cache_size", CACHE_SIZE_KIB)?;
    crate::sqlite::search::register_functions(conn)?;
    Ok(())
}

/// Settings of read-pool connections: like [`configure_connection`] but `query_only` (a read
/// transaction can never write, even by mistake) and no journal-mode change.
pub fn configure_reader(conn: &Connection) -> StorageResult<()> {
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "busy_timeout", BUSY_TIMEOUT_MS)?;
    conn.pragma_update(None, "temp_store", "MEMORY")?;
    conn.pragma_update(None, "query_only", "ON")?;
    conn.pragma_update(None, "cache_size", CACHE_SIZE_KIB)?;
    crate::sqlite::search::register_functions(conn)?;
    Ok(())
}

/// Page cache per connection, in KiB (negative = KiB for `PRAGMA cache_size`): 64 MiB keeps a
/// 100k-note collection's hot pages in memory so a search does not go back to the OS cache
/// (docs/04 v0.4 benchmark).
pub const CACHE_SIZE_KIB: i64 = -65_536;

/// SQLite busy timeout, also used for acquiring a pooled connection.
pub const BUSY_TIMEOUT_MS: u32 = 5000;
