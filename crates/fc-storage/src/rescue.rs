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
        // (table, rowid, foreign key id): the rowid is NULL in a WITHOUT ROWID table
        // (`note_tags`, `links`, `attribute_values`, …).
        let bad: Vec<(String, Option<i64>, i64)> = {
            let mut st = conn.prepare("PRAGMA foreign_key_check").map_err(at("fk"))?;
            let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(3)?)))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        if bad.is_empty() {
            break;
        }
        let mut by_key: std::collections::BTreeSet<(String, i64)> = Default::default();
        for (table, rowid, fk) in bad {
            let n = match rowid {
                Some(rowid) => conn.execute(
                    &format!("DELETE FROM {} WHERE rowid = ?1", quote(&table)),
                    [rowid],
                )?,
                // No rowid to name the row by: every orphan of this foreign key at once.
                None if by_key.insert((table.clone(), fk)) => {
                    delete_orphans(&conn, &table, fk).map_err(at("fk"))?
                }
                None => 0,
            };
            if n > 0 {
                let e = report.tables.entry(table).or_default();
                e.0 = e.0.saturating_sub(n as u64);
                e.1 += n as u64;
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
    // Rows the migrations seeded in the fresh file (flag names, prices, counters) give way to
    // the collection's own: `OR IGNORE` would keep the defaults over what the user changed.
    let seeded = conn
        .query_row(
            &format!("SELECT EXISTS (SELECT 1 FROM main.{q})"),
            [],
            |r| r.get::<_, bool>(0),
        )
        .unwrap_or(false);
    let verb = if seeded { "REPLACE" } else { "IGNORE" };
    let insert = format!("INSERT OR {verb} INTO main.{q} ({into}) SELECT {from} FROM damaged.{q}");
    if let Ok(n) = conn.execute(&insert, []) {
        // Junk rows of a page whose header still reads come back as rows that break a
        // constraint and are ignored: what the damaged file counts but did not arrive is lost.
        let listed: Option<i64> = conn
            .query_row(&format!("SELECT count(*) FROM damaged.{q}"), [], |r| {
                r.get(0)
            })
            .ok();
        let listed = listed.and_then(|c| u64::try_from(c).ok()).unwrap_or(0);
        return (n as u64, listed.saturating_sub(n as u64));
    }
    if !rowid {
        return copy_rows_both_ways(conn, table, &q, &insert);
    }
    // Walk the rowids we can still list, from both ends: a walk stops at the first unreadable
    // page, so the rows behind it are reached from the other side (v1.4.2 review J1).
    let walk = |order: &str| -> (Vec<i64>, bool) {
        let mut ids = Vec::new();
        let mut complete = false;
        if let Ok(mut st) = conn.prepare(&format!(
            "SELECT rowid FROM damaged.{q} ORDER BY rowid {order}"
        )) {
            if let Ok(mut rows) = st.query([]) {
                loop {
                    match rows.next() {
                        Ok(Some(r)) => {
                            if let Ok(id) = r.get::<_, i64>(0) {
                                ids.push(id);
                            }
                        }
                        Ok(None) => {
                            complete = true;
                            break;
                        }
                        Err(_) => break,
                    }
                }
            }
        }
        (ids, complete)
    };
    let (mut ids, complete) = walk("ASC");
    // Rows on unreadable pages are never listed: estimated by the rowids between the two
    // walks (an upper bound: rowids of deleted rows are counted too).
    let mut unlisted = 0u64;
    if !complete {
        let (back, _) = walk("DESC");
        let low = ids.last().copied();
        let behind: Vec<i64> = back
            .into_iter()
            .filter(|id| low.is_none_or(|l| *id > l))
            .collect();
        unlisted = match (low, behind.iter().min()) {
            (Some(l), Some(h)) => u64::try_from(h - l - 1).unwrap_or(0).max(1),
            _ => 1,
        };
        ids.extend(behind);
        ids.sort_unstable();
        ids.dedup();
    }
    let one = format!("{insert} WHERE rowid BETWEEN ?1 AND ?2");
    let have = format!("SELECT count(*) FROM main.{q} WHERE rowid BETWEEN ?1 AND ?2");
    let mut lost = 0u64;
    let mut stack: Vec<&[i64]> = ids.chunks(512).rev().collect();
    while let Some(chunk) = stack.pop() {
        let (Some(first), Some(last)) = (chunk.first(), chunk.last()) else {
            continue;
        };
        // A range can also end early without an error (a junk row with a huge rowid ends
        // the scan), so a range counts as copied only when all its rows arrived.
        let arrived = conn.execute(&one, [first, last]).is_ok()
            && conn
                .query_row(&have, [first, last], |r| r.get::<_, i64>(0))
                .is_ok_and(|n| usize::try_from(n).is_ok_and(|n| n >= chunk.len()));
        if arrived {
            continue;
        }
        if chunk.len() == 1 {
            lost += 1;
        } else {
            let (a, b) = chunk.split_at(chunk.len() / 2);
            stack.push(b);
            stack.push(a);
        }
    }
    let copied = (ids.len() as u64).saturating_sub(lost);
    (copied, lost + unlisted)
}

/// A WITHOUT ROWID table that could not be copied whole: row by row in key order from both
/// ends (each walk stops at the first unreadable page). The rows in between are lost; their
/// number is unknown and counted as 1.
fn copy_rows_both_ways(conn: &Connection, table: &str, q: &str, insert: &str) -> (u64, u64) {
    let key: Vec<String> = match conn
        .prepare("SELECT name FROM pragma_table_info(?1, 'main') WHERE pk > 0 ORDER BY pk")
    {
        Ok(mut st) => st
            .query_map([table], |r| r.get::<_, String>(0))
            .and_then(|rows| rows.collect::<rusqlite::Result<Vec<_>>>())
            .unwrap_or_default(),
        Err(_) => Vec::new(),
    };
    if key.is_empty() {
        return (0, 1);
    }
    let cols: Vec<String> = key.iter().map(|k| quote(k)).collect();
    let key_list = cols.join(", ");
    let marks: Vec<String> = (1..=cols.len()).map(|i| format!("?{i}")).collect();
    let one = format!("{insert} WHERE ({key_list}) = ({})", marks.join(", "));
    let mut copied = 0u64;
    let mut complete = false;
    for order in ["ASC", "DESC"] {
        let by: Vec<String> = cols.iter().map(|c| format!("{c} {order}")).collect();
        let Ok(mut st) = conn.prepare(&format!(
            "SELECT {key_list} FROM damaged.{q} ORDER BY {}",
            by.join(", ")
        )) else {
            break;
        };
        let Ok(mut rows) = st.query([]) else { break };
        loop {
            match rows.next() {
                Ok(Some(r)) => {
                    let values: Vec<rusqlite::types::Value> =
                        (0..cols.len()).filter_map(|i| r.get(i).ok()).collect();
                    if values.len() == cols.len() {
                        if let Ok(n) = conn.execute(&one, rusqlite::params_from_iter(values.iter()))
                        {
                            copied += n as u64;
                        }
                    }
                }
                Ok(None) => {
                    complete = true;
                    break;
                }
                Err(_) => break,
            }
        }
        if complete {
            break;
        }
    }
    (copied, u64::from(!complete))
}

/// `"name"` (identifier quoting).
fn quote(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// Deletes the rows of `table` whose foreign key `fk` (an id of `PRAGMA foreign_key_list`)
/// points at no parent row; returns how many. Used where rows have no rowid.
fn delete_orphans(conn: &Connection, table: &str, fk: i64) -> rusqlite::Result<usize> {
    let mut st = conn.prepare(
        "SELECT \"table\", \"from\", \"to\" FROM pragma_foreign_key_list(?1) WHERE id = ?2
         ORDER BY seq",
    )?;
    let cols: Vec<(String, String, Option<String>)> = st
        .query_map(rusqlite::params![table, fk], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?))
        })?
        .collect::<rusqlite::Result<_>>()?;
    let Some((parent, _, _)) = cols.first() else {
        return Ok(0);
    };
    // `REFERENCES parent` without columns names the parent's primary key.
    let parent_pk: Vec<String> = {
        let mut st =
            conn.prepare("SELECT name FROM pragma_table_info(?1) WHERE pk > 0 ORDER BY pk")?;
        let v = st
            .query_map([parent], |r| r.get(0))?
            .collect::<rusqlite::Result<_>>()?;
        v
    };
    let (t, p) = (quote(table), quote(parent));
    // A NULL in any child column means "no reference" (SQLite's rule): never an orphan.
    let present: Vec<String> = cols
        .iter()
        .map(|(_, from, _)| format!("{t}.{} IS NOT NULL", quote(from)))
        .collect();
    let matches: Vec<String> = cols
        .iter()
        .enumerate()
        .map(|(i, (_, from, to))| {
            let to = match to {
                Some(to) => quote(to),
                None => parent_pk
                    .get(i)
                    .map_or_else(|| "rowid".to_owned(), |c| quote(c)),
            };
            format!("{p}.{to} = {t}.{}", quote(from))
        })
        .collect();
    conn.execute(
        &format!(
            "DELETE FROM {t} WHERE {} AND NOT EXISTS (SELECT 1 FROM {p} WHERE {})",
            present.join(" AND "),
            matches.join(" AND ")
        ),
        [],
    )
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
