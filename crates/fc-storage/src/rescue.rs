//! A collection file that does not open or fails its integrity check (v1.4.1 H2): looking at
//! it without changing it ([`inspect_file`]) and copying what is still readable into a fresh
//! collection ([`salvage_into`]). Nothing here writes to, renames or deletes the damaged file;
//! `fc-app`'s rescue moves it aside first and keeps it.

use crate::error::{StorageError, StorageResult};
use crate::migrate;
use rusqlite::{Connection, OpenFlags, OptionalExtension};
use std::collections::BTreeMap;
use std::path::Path;

/// What [`inspect_file`] found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileHealth {
    /// SQLite could open the file and read its schema.
    pub opens: bool,
    /// Schema version in the file (0 when unreadable).
    pub schema_version: u32,
    /// `PRAGMA integrity_check` said `ok` and `foreign_key_check` found nothing.
    pub ok: bool,
    /// The first problems found (SQLite's own words; empty when `ok`).
    pub problems: Vec<String>,
}

/// Problems kept by [`inspect_file`].
const MAX_PROBLEMS: usize = 20;

/// Opens `path` read-only (never creating it, never migrating, never checkpointing a WAL
/// into it) and runs a full integrity check and a foreign-key check.
pub fn inspect_file(path: &Path) -> FileHealth {
    let mut health = FileHealth {
        opens: false,
        schema_version: 0,
        ok: false,
        problems: Vec::new(),
    };
    let conn = match open_read_only(path) {
        Ok(c) => c,
        Err(e) => {
            health.problems.push(e.to_string());
            return health;
        }
    };
    match migrate::current_version(&conn).and_then(|v| {
        // Reading the schema is what fails on a file that is not a database.
        conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| {
            r.get::<_, i64>(0)
        })?;
        Ok(v)
    }) {
        Ok(v) => {
            health.opens = true;
            health.schema_version = v;
        }
        Err(e) => {
            health.problems.push(e.to_string());
            return health;
        }
    }
    let mut problems = Vec::new();
    match all_strings(&conn, &format!("PRAGMA integrity_check({MAX_PROBLEMS})")) {
        Ok(rows) => problems.extend(rows.into_iter().filter(|r| r != "ok")),
        Err(e) => problems.push(e.to_string()),
    }
    match conn.prepare("PRAGMA foreign_key_check").and_then(|mut st| {
        st.query_map([], |r| r.get::<_, Option<String>>(0))?
            .count_ok()
    }) {
        Ok(0) => {}
        Ok(n) => problems.push(format!("{n} foreign key violation(s)")),
        Err(e) => problems.push(e.to_string()),
    }
    problems.truncate(MAX_PROBLEMS);
    health.ok = problems.is_empty();
    health.problems = problems;
    health
}

trait CountOk {
    fn count_ok(self) -> rusqlite::Result<usize>;
}

impl<I: Iterator<Item = rusqlite::Result<T>>, T> CountOk for I {
    fn count_ok(self) -> rusqlite::Result<usize> {
        let mut n = 0;
        for r in self {
            r?;
            n += 1;
        }
        Ok(n)
    }
}

fn open_read_only(path: &Path) -> StorageResult<Connection> {
    if !path.is_file() {
        return Err(StorageError::File {
            path: path.display().to_string(),
            message: "no such file".into(),
        });
    }
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conn.pragma_update(None, "busy_timeout", migrate::BUSY_TIMEOUT_MS)?;
    Ok(conn)
}

fn all_strings(conn: &Connection, sql: &str) -> rusqlite::Result<Vec<String>> {
    let mut st = conn.prepare(sql)?;
    let rows = st.query_map([], |r| r.get::<_, String>(0))?;
    rows.collect()
}

/// Per table: rows copied and rows that could not be read (or broke a foreign key and were
/// left out).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SalvageReport {
    /// Schema version of the damaged file the rows were read at.
    pub from_version: u32,
    /// `table → (copied, lost)`.
    pub tables: BTreeMap<String, (u64, u64)>,
}

impl SalvageReport {
    /// Rows copied over all tables.
    pub fn copied(&self) -> u64 {
        self.tables.values().map(|t| t.0).sum()
    }
    /// Rows lost over all tables.
    pub fn lost(&self) -> u64 {
        self.tables.values().map(|t| t.1).sum()
    }
}

/// Copies every row still readable from the damaged collection at `damaged` into a NEW
/// collection at `fresh` (which must not exist), then brings it to this build's schema.
///
/// The fresh file is created at the damaged file's schema version (so rows copy column for
/// column), filled table by table — a table that cannot be read whole is read in ever smaller
/// `rowid` ranges, skipping only the rows that fail — then rows that break a foreign key
/// (e.g. a card whose note was lost) are removed and counted as lost, and the remaining
/// migrations run. `damaged` is opened read-only.
pub fn salvage_into(damaged: &Path, fresh: &Path) -> StorageResult<SalvageReport> {
    if fresh.exists() {
        return Err(StorageError::File {
            path: fresh.display().to_string(),
            message: "already exists".into(),
        });
    }
    let from_version = {
        let src = open_read_only(damaged)?;
        migrate::current_version(&src).map_err(at("version"))?
    };
    let latest = migrate::latest_version();
    if from_version > latest {
        return Err(StorageError::TooNew {
            found: from_version,
            supported: latest,
        });
    }
    let upto: Vec<migrate::Migration> = migrate::MIGRATIONS
        .iter()
        .copied()
        .filter(|m| m.version <= from_version)
        .collect();
    let mut conn = Connection::open(fresh).map_err(at("create"))?;
    migrate::configure_connection(&conn).map_err(at("configure"))?;
    migrate::migrate_with(&mut conn, &upto).map_err(at("schema"))?;
    let mut report = SalvageReport {
        from_version,
        ..SalvageReport::default()
    };
    conn.pragma_update(None, "foreign_keys", "OFF")?;
    let uri = format!(
        "file:{}?mode=ro",
        damaged
            .to_string_lossy()
            .replace('?', "%3f")
            .replace('#', "%23")
    );
    conn.execute("ATTACH DATABASE ?1 AS damaged", [uri])
        .map_err(at("attach"))?;
    let tables = copyable_tables(&conn).map_err(at("tables"))?;
    // One statement per range, each its own transaction: a read of a damaged page fails
    // only that statement (a shared transaction would not commit after it).
    for (table, rowid) in &tables {
        let (copied, lost) = copy_table(&conn, table, *rowid);
        report.tables.insert(table.clone(), (copied, lost));
    }
    conn.execute("DETACH DATABASE damaged", [])
        .map_err(at("detach"))?;
    // Rows whose parent was lost go too (repeat: removing one may orphan another).
    for _ in 0..8 {
        let bad: Vec<(String, i64)> = {
            let mut st = conn.prepare("PRAGMA foreign_key_check").map_err(at("fk"))?;
            let rows = st.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        if bad.is_empty() {
            break;
        }
        for (table, rowid) in bad {
            let n = conn.execute(
                &format!(
                    "DELETE FROM \"{}\" WHERE rowid = ?1",
                    table.replace('"', "\"\"")
                ),
                [rowid],
            )?;
            if n > 0 {
                let e = report.tables.entry(table).or_default();
                e.0 = e.0.saturating_sub(1);
                e.1 += 1;
            }
        }
    }
    conn.pragma_update(None, "foreign_keys", "ON")?;
    migrate::migrate(&mut conn).map_err(at("migrate"))?;
    drop(conn);
    Ok(report)
}

/// Names the step of a salvage that failed.
fn at<E: Into<StorageError> + std::fmt::Display>(step: &'static str) -> impl Fn(E) -> StorageError {
    move |e| StorageError::Corrupt {
        table: "salvage",
        message: format!("{step}: {e}"),
    }
}

/// Tables of the fresh file that the damaged one has too (`name`, has a rowid): everything
/// except SQLite's own tables, the full-text index's shadow tables (written through
/// `notes_fts` itself) and the migration ledger (already written for the same versions).
fn copyable_tables(conn: &Connection) -> StorageResult<Vec<(String, bool)>> {
    let mut st = conn.prepare(
        "SELECT name, sql FROM main.sqlite_master WHERE type = 'table'
           AND name NOT LIKE 'sqlite_%' AND name <> 'schema_migrations'
         ORDER BY name",
    )?;
    let all: Vec<(String, Option<String>)> = st
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<rusqlite::Result<_>>()?;
    let virtuals: Vec<String> = all
        .iter()
        .filter(|(_, sql)| {
            sql.as_deref().is_some_and(|s| {
                s.trim_start()
                    .to_ascii_uppercase()
                    .starts_with("CREATE VIRTUAL")
            })
        })
        .map(|(n, _)| format!("{n}_"))
        .collect();
    let mut out = Vec::new();
    for (name, sql) in all {
        if virtuals.iter().any(|v| name.starts_with(v.as_str())) {
            continue;
        }
        let in_damaged: Option<i64> = conn
            .query_row(
                "SELECT 1 FROM damaged.sqlite_master WHERE type = 'table' AND name = ?1",
                [&name],
                |r| r.get(0),
            )
            .optional()
            .unwrap_or(None);
        if in_damaged.is_none() {
            continue;
        }
        let rowid = !sql
            .as_deref()
            .is_some_and(|s| s.to_ascii_uppercase().contains("WITHOUT ROWID"));
        out.push((name, rowid));
    }
    Ok(out)
}

/// Copies one table; returns `(copied, lost)`.
fn copy_table(conn: &Connection, table: &str, rowid: bool) -> (u64, u64) {
    let q = format!("\"{}\"", table.replace('"', "\"\""));
    let cols = match column_list(conn, table) {
        Ok(c) => c,
        Err(_) => return (0, 0),
    };
    // A rowid table keeps its rowids (the full-text table's are referenced by notes).
    let (into, from) = if rowid {
        (format!("rowid, {cols}"), format!("rowid, {cols}"))
    } else {
        (cols.clone(), cols.clone())
    };
    let insert = format!("INSERT OR IGNORE INTO main.{q} ({into}) SELECT {from} FROM damaged.{q}");
    if let Ok(n) = conn.execute(&insert, []) {
        return (n as u64, 0);
    }
    if !rowid {
        // No rowid to walk: whatever could not be read whole is lost (counted as unknown: 0
        // copied); the integrity report of the damaged file names the table.
        return (0, 0);
    }
    // Walk the rowids we can still list, in ranges that halve on failure.
    let ids: Vec<i64> = match conn.prepare(&format!("SELECT rowid FROM damaged.{q} ORDER BY rowid"))
    {
        Ok(mut st) => {
            let mut ids = Vec::new();
            if let Ok(mut rows) = st.query([]) {
                // Stops at the first unreadable page; the ids before it are still good.
                while let Ok(Some(r)) = rows.next() {
                    if let Ok(id) = r.get::<_, i64>(0) {
                        ids.push(id);
                    }
                }
            }
            ids
        }
        Err(_) => Vec::new(),
    };
    let one = format!("{insert} WHERE rowid BETWEEN ?1 AND ?2");
    let (mut copied, mut lost) = (0u64, 0u64);
    let mut stack: Vec<&[i64]> = ids.chunks(512).rev().collect();
    while let Some(chunk) = stack.pop() {
        let (Some(first), Some(last)) = (chunk.first(), chunk.last()) else {
            continue;
        };
        match conn.execute(&one, [first, last]) {
            Ok(n) => copied += n as u64,
            Err(_) if chunk.len() == 1 => lost += 1,
            Err(_) => {
                let (a, b) = chunk.split_at(chunk.len() / 2);
                stack.push(b);
                stack.push(a);
            }
        }
    }
    (copied, lost)
}

fn column_list(conn: &Connection, table: &str) -> rusqlite::Result<String> {
    let mut st = conn.prepare("SELECT name FROM pragma_table_info(?1, 'main')")?;
    let names: Vec<String> = st
        .query_map([table], |r| r.get(0))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(names
        .iter()
        .map(|n| format!("\"{}\"", n.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(", "))
}
