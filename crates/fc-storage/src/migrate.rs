//! Schema migrations. Versions are tracked with `PRAGMA user_version`.
//!
//! Numbering is reserved per release lane so parallel branches never collide (docs/07 §5):
//! v0.1 = 1–99, v0.2 = 100–199, v0.3 = 200–299, v0.4 = 300–399, v0.5 = 400–499, …
//! Gaps are fine; order is by number. A migration is never edited after merge.

use crate::error::{StorageError, StorageResult};
use rusqlite::Connection;

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

/// All migrations, ascending by version.
pub const MIGRATIONS: &[Migration] = &[Migration {
    version: 1,
    name: "0001_init",
    sql: include_str!("../migrations/0001_init.sql"),
}];

/// Highest known schema version.
pub fn latest_version() -> u32 {
    MIGRATIONS.last().map_or(0, |m| m.version)
}

/// Current schema version of `conn`.
pub fn current_version(conn: &Connection) -> StorageResult<u32> {
    let v: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    Ok(u32::try_from(v).unwrap_or(0))
}

/// Applies pending migrations, each in its own transaction. Returns the versions applied.
/// Fails with [`StorageError::TooNew`] if the database is ahead of this build.
///
/// (Taking a file snapshot before migrating — docs/03 §15 — is the caller's job, because it
/// needs the file path.)
pub fn migrate(conn: &mut Connection) -> StorageResult<Vec<u32>> {
    let current = current_version(conn)?;
    let latest = latest_version();
    if current > latest {
        return Err(StorageError::TooNew {
            found: current,
            supported: latest,
        });
    }
    let mut applied = Vec::new();
    for m in MIGRATIONS.iter().filter(|m| m.version > current) {
        let tx = conn.transaction()?;
        tx.execute_batch(m.sql)
            .map_err(|e| StorageError::Migration {
                version: m.version,
                message: e.to_string(),
            })?;
        tx.pragma_update(None, "user_version", m.version)?;
        tx.commit()?;
        applied.push(m.version);
    }
    Ok(applied)
}

/// Connection settings every connection must use (WAL, foreign keys, busy timeout).
pub fn configure_connection(conn: &Connection) -> StorageResult<()> {
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "busy_timeout", 5000)?;
    // In-memory databases report "memory"; ignore the returned mode.
    let _mode: String = conn.pragma_update_and_check(None, "journal_mode", "WAL", |r| r.get(0))?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    Ok(())
}
