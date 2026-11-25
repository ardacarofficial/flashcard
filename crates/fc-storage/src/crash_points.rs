//! Kill-test support (v1.4.1 H2, feature `crash-points`; never in a release build): a child
//! process started by a test kills itself (`std::process::abort`: no destructor, no rollback,
//! no flush) at a chosen point **inside an open write transaction** of a writer connection.
//!
//! * [`crash_after_row_changes`]`(k)`: at the `k`-th row inserted/updated/deleted from now on
//!   by this process' writer connections.
//! * [`crash_at_next_commits`]`(k)`: inside the `k`-th `COMMIT` from now on (SQLite's commit
//!   hook runs before the transaction is durable).
//! * The same before a store is opened (migrations): `FC_CRASH_AT_ROW_CHANGE=<k>` /
//!   `FC_CRASH_AT_COMMIT=<k>` in the environment, read when a writer connection is configured.
//!
//! [`row_changes`] / [`commits`] count what happened so far, so a test can first measure how
//! many row changes a command makes and then kill it at each of them.

use rusqlite::Connection;
use std::sync::atomic::{AtomicU64, Ordering};

/// Row change (counted from process start) at which the process aborts, set from the
/// environment before a store opens.
pub const AT_ROW_CHANGE: &str = "FC_CRASH_AT_ROW_CHANGE";
/// Commit (counted from process start) at which the process aborts.
pub const AT_COMMIT: &str = "FC_CRASH_AT_COMMIT";

static ROW_CHANGES: AtomicU64 = AtomicU64::new(0);
static COMMITS: AtomicU64 = AtomicU64::new(0);
/// 0 = not armed.
static ROW_TARGET: AtomicU64 = AtomicU64::new(0);
static COMMIT_TARGET: AtomicU64 = AtomicU64::new(0);

/// Row changes made by writer connections of this process so far.
pub fn row_changes() -> u64 {
    ROW_CHANGES.load(Ordering::SeqCst)
}

/// Commits of writer connections of this process so far.
pub fn commits() -> u64 {
    COMMITS.load(Ordering::SeqCst)
}

/// Aborts the process at the `k`-th row change from now (`k >= 1`).
pub fn crash_after_row_changes(k: u64) {
    ROW_TARGET.store(row_changes() + k.max(1), Ordering::SeqCst);
}

/// Aborts the process inside the `k`-th commit from now (`k >= 1`).
pub fn crash_at_next_commits(k: u64) {
    COMMIT_TARGET.store(commits() + k.max(1), Ordering::SeqCst);
}

fn from_env(var: &str, target: &AtomicU64) {
    if let Some(n) = std::env::var(var)
        .ok()
        .and_then(|v| v.trim().parse::<u64>().ok())
        .filter(|n| *n > 0)
    {
        let _ = target.compare_exchange(0, n, Ordering::SeqCst, Ordering::SeqCst);
    }
}

/// Installs the counting hooks on a writer connection.
pub(crate) fn install(conn: &mut Connection) {
    from_env(AT_ROW_CHANGE, &ROW_TARGET);
    from_env(AT_COMMIT, &COMMIT_TARGET);
    // Hooks cannot fail to install on an open connection; a failure only disarms the test.
    let _ = conn.update_hook(Some(|_, _: &str, _: &str, _| {
        let n = ROW_CHANGES.fetch_add(1, Ordering::SeqCst) + 1;
        let at = ROW_TARGET.load(Ordering::SeqCst);
        if at != 0 && n >= at {
            std::process::abort();
        }
    }));
    let _ = conn.commit_hook(Some(|| {
        let n = COMMITS.fetch_add(1, Ordering::SeqCst) + 1;
        let at = COMMIT_TARGET.load(Ordering::SeqCst);
        if at != 0 && n >= at {
            std::process::abort();
        }
        false
    }));
}

/// Kill-test support: writes a new collection file at `path` at the **older** schema
/// `dump.schema_versions` holding the dump's rows (a dump cut down with
/// [`crate::fit_dump_to_schema`]), so a test can kill the migrations the next open runs.
pub fn write_older_collection(
    path: &std::path::Path,
    dump: &crate::repo::CollectionDump,
) -> crate::StorageResult<()> {
    let at: Vec<crate::migrate::Migration> = crate::migrate::MIGRATIONS
        .iter()
        .filter(|m| dump.schema_versions.contains(&m.version))
        .copied()
        .collect();
    let store = crate::SqliteStore::open_with_migrations(path, &at)?;
    let mut tx = crate::Store::write(&store)?;
    tx.load_collection(dump)?;
    tx.commit()
}

/// Runs `sql` on the writer connection of `store`, outside any transaction (tests: a trigger
/// that makes one statement of a command fail, a table that cannot be read).
///
/// # Errors
/// The SQLite error as text.
pub fn run_sql(store: &crate::SqliteStore, sql: &str) -> Result<(), String> {
    store.with_writer(|conn| conn.execute_batch(sql).map_err(|e| e.to_string()))
}

/// A full disk for the writer connection of `store`: with `Some(extra)` the database may grow
/// by `extra` more pages and then every statement that needs another one stops with SQLite's
/// own "database or disk is full"; `None` lifts the limit. Returns the page count now.
///
/// # Errors
/// The SQLite error as text.
pub fn allow_pages(store: &crate::SqliteStore, extra: Option<u32>) -> Result<u32, String> {
    store.with_writer(|conn| {
        let pages: u32 = conn
            .pragma_query_value(None, "page_count", |r| r.get(0))
            .map_err(|e| e.to_string())?;
        let limit = extra.map_or(u32::MAX - 1, |e| pages.saturating_add(e));
        conn.pragma_update_and_check(None, "max_page_count", limit, |r| r.get::<_, i64>(0))
            .map_err(|e| e.to_string())?;
        Ok(pages)
    })
}
