//! Import/export support: bulk review log rows, whole-collection dump/load (in memory, or
//! streamed as JSON row by row: v1.4 Z2).

use super::study::{review_row, REVIEW_COLS};
use super::{all, count, enum_str, exec, json_list};
use super::{Mode, SqliteTx, WriteMode};
use crate::error::{StorageError, StorageResult};
use crate::repo::{
    read_dump_json, CollectionDump, DumpSink, DumpStream, DumpStreamError, DumpSummary,
    InteropRead, InteropWrite, PurgeRows, ReviewLogRecord, RowHook, SqlValue, TableDump,
};
use fc_core::CardId;
use rusqlite::types::{Value, ValueRef};
use rusqlite::{params, params_from_iter, Connection};
use serde::ser::{SerializeStruct, Serializer};
use serde::Serialize;
use std::cell::RefCell;
use std::collections::{BTreeMap, HashSet};

const REVIEWS: &str = "review_log";
const BACKUP: &str = "backup";

/// Tables a dump never contains. `unfinished_batches(_parts)` are in-flight job state (an
/// import's crash marker): jobs run one at a time and leftovers are rolled back at startup,
/// so a backup never has rows there, and a restore must not touch a running group's marker.
/// The v0.6 statistics cache (0500) is derived data: never dumped; a restore empties it and
/// bumps the generation (`load_collection`), the app rebuilds it on the next refresh.
/// Empties the statistics cache and invalidates any copy of its counters (restore).
const RESET_STATS_CACHE: &str = "DELETE FROM stats_day_cache;
     DELETE FROM stats_cache_meta;
     UPDATE review_log_generation SET n = n + 1 WHERE id = 1;";

const EXCLUDED: &[&str] = &[
    "schema_migrations",
    "unfinished_batches",
    "unfinished_batch_parts",
    "stats_day_cache",
    "stats_cache_meta",
    "review_log_generation",
    // v1.4 word index queues: transient (the triggers refill them as a restore writes rows).
    "word_index_dirty",
    "word_index_dirty_keys",
];

/// Rows of a table a dump leaves out, as a `WHERE` condition on the rows it keeps. The word
/// data the app ships (v1.4 B2, `word_data_packs.origin = 'bundled'`) is reinstalled from the
/// binary, so a backup carries only imported packs.
fn dump_filter(table: &str) -> Option<&'static str> {
    match table {
        "word_data_packs" => Some("origin = 'imported'"),
        "word_data" | "word_family" => {
            Some("language IN (SELECT language FROM word_data_packs WHERE origin = 'imported')")
        }
        _ => None,
    }
}

fn corrupt(message: impl ToString) -> StorageError {
    StorageError::Corrupt {
        table: BACKUP,
        message: message.to_string(),
    }
}

fn quote(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

struct TableInfo {
    name: String,
    with_rowid: bool,
    columns: Vec<String>,
    pk: Vec<String>,
}

/// User tables of the collection (dump scope), sorted by name.
fn user_tables(conn: &Connection) -> StorageResult<Vec<TableInfo>> {
    let mut st = conn.prepare(
        "SELECT name, sql FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%'
         ORDER BY name",
    )?;
    let raw: Vec<(String, String)> = st
        .query_map([], |r| {
            Ok((
                r.get(0)?,
                r.get::<_, Option<String>>(1)?.unwrap_or_default(),
            ))
        })?
        .collect::<Result<_, _>>()?;
    let virtual_prefixes: Vec<String> = raw
        .iter()
        .filter(|(_, sql)| sql.to_ascii_uppercase().starts_with("CREATE VIRTUAL TABLE"))
        .map(|(name, _)| format!("{name}_"))
        .collect();
    let mut out = Vec::new();
    for (name, sql) in raw {
        if EXCLUDED.contains(&name.as_str())
            || virtual_prefixes
                .iter()
                .any(|p| name.starts_with(p.as_str()))
        {
            continue;
        }
        let mut info = conn.prepare(&format!("PRAGMA table_info({})", quote(&name)))?;
        let cols: Vec<(String, i64)> = info
            .query_map([], |r| Ok((r.get(1)?, r.get(5)?)))?
            .collect::<Result<_, _>>()?;
        let mut pk: Vec<(i64, String)> = cols
            .iter()
            .filter(|(_, p)| *p > 0)
            .map(|(n, p)| (*p, n.clone()))
            .collect();
        pk.sort();
        out.push(TableInfo {
            with_rowid: !sql.to_ascii_uppercase().contains("WITHOUT ROWID"),
            columns: cols.into_iter().map(|(n, _)| n).collect(),
            pk: pk.into_iter().map(|(_, n)| n).collect(),
            name,
        });
    }
    Ok(out)
}

fn hex(bytes: &[u8]) -> String {
    const H: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(H[(b >> 4) as usize] as char);
        s.push(H[(b & 15) as usize] as char);
    }
    s
}

fn unhex(s: &str) -> StorageResult<Vec<u8>> {
    if s.len() % 2 != 0 {
        return Err(corrupt("odd hex length"));
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(corrupt))
        .collect()
}

fn to_sql_value(v: ValueRef<'_>) -> SqlValue {
    match v {
        ValueRef::Null => SqlValue::Null,
        ValueRef::Integer(i) => SqlValue::Integer(i),
        ValueRef::Real(f) => SqlValue::Real(format!("{f:?}")),
        ValueRef::Text(t) => SqlValue::Text(String::from_utf8_lossy(t).into_owned()),
        ValueRef::Blob(b) => SqlValue::Blob(hex(b)),
    }
}

fn from_sql_value(v: &SqlValue) -> StorageResult<Value> {
    Ok(match v {
        SqlValue::Null => Value::Null,
        SqlValue::Integer(i) => Value::Integer(*i),
        SqlValue::Real(s) => Value::Real(s.parse().map_err(|e| corrupt(format!("real {s}: {e}")))?),
        SqlValue::Text(s) => Value::Text(s.clone()),
        SqlValue::Blob(h) => Value::Blob(unhex(h)?),
    })
}

impl<M: Mode> InteropRead for SqliteTx<'_, M> {
    fn review_logs_for_cards(&self, cards: &[CardId]) -> StorageResult<Vec<ReviewLogRecord>> {
        all(
            self.conn(),
            REVIEWS,
            &format!(
                "SELECT {REVIEW_COLS} FROM review_log
                 WHERE card_id IN (SELECT value FROM json_each(?1)) ORDER BY created_at, id"
            ),
            [json_list(cards)],
            review_row,
        )
    }

    fn dump_collection(&self) -> StorageResult<CollectionDump> {
        let conn = self.conn();
        let versions = schema_versions(conn)?;
        let mut tables = Vec::new();
        for q in dump_queries(conn)? {
            let mut st = conn.prepare(&q.sql)?;
            let n = q.columns.len();
            let rows: Vec<Vec<SqlValue>> = st
                .query_map([], |r| dump_row(r, n))?
                .collect::<Result<_, _>>()?;
            tables.push(TableDump {
                name: q.name,
                columns: q.columns,
                rows,
            });
        }
        Ok(CollectionDump {
            schema_versions: versions,
            tables,
        })
    }

    fn dump_collection_json(
        &self,
        out: &mut dyn std::io::Write,
    ) -> Result<DumpSummary, DumpStreamError> {
        let conn = self.conn();
        let w = JsonDump {
            conn,
            versions: schema_versions(conn)?,
            tables: dump_queries(conn)?,
            counts: RefCell::new(BTreeMap::new()),
            failed: RefCell::new(None),
        };
        let written = serde_json::to_writer(out, &w);
        if let Some(e) = w.failed.take() {
            return Err(DumpStreamError::Storage(e));
        }
        written.map_err(DumpStreamError::Json)?;
        Ok(DumpSummary {
            schema_versions: w.versions,
            counts: w.counts.into_inner(),
        })
    }
    fn dump_row_counts(&self) -> StorageResult<std::collections::BTreeMap<String, u64>> {
        let conn = self.conn();
        let mut counts = std::collections::BTreeMap::new();
        for t in user_tables(conn)? {
            let filter = dump_filter(&t.name).map_or(String::new(), |w| format!(" WHERE {w}"));
            let n: i64 = conn.query_row(
                &format!("SELECT COUNT(*) FROM {}{filter}", quote(&t.name)),
                [],
                |r| r.get(0),
            )?;
            counts.insert(t.name, u64::try_from(n).unwrap_or(0));
        }
        Ok(counts)
    }
}

impl InteropWrite for SqliteTx<'_, WriteMode> {
    fn insert_review_logs(&mut self, rows: &[ReviewLogRecord]) -> StorageResult<u32> {
        let mut n = 0;
        for r in rows {
            n += exec(
                self.conn(),
                REVIEWS,
                &format!(
                    "INSERT OR IGNORE INTO review_log ({REVIEW_COLS})
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)"
                ),
                params![
                    r.id.to_string(),
                    r.card_id.to_string(),
                    r.session_id.map(|s| s.to_string()),
                    enum_str(REVIEWS, &r.rating)?,
                    enum_str(REVIEWS, &r.state_before)?,
                    enum_str(REVIEWS, &r.state_after)?,
                    r.scheduled_days,
                    r.elapsed_days,
                    r.duration_ms,
                    r.answer_duration_ms,
                    r.hints_used,
                    r.mode,
                    r.stability_after,
                    r.difficulty_after,
                    r.created_at.0,
                ],
            )?;
        }
        Ok(count(n))
    }

    fn load_collection(&mut self, dump: &CollectionDump) -> StorageResult<()> {
        let conn = self.conn();
        conn.execute_batch("PRAGMA defer_foreign_keys = ON")?;
        let current = user_tables(conn)?;
        // Validate before deleting anything.
        for t in &dump.tables {
            check_table(&current, &t.name, &t.columns)?;
            if t.rows.iter().any(|r| r.len() != t.columns.len()) {
                return Err(corrupt(format!("ragged rows in {}", t.name)));
            }
        }
        empty_tables(conn, &current)?;
        for t in &dump.tables {
            if t.rows.is_empty() {
                continue;
            }
            insert_rows(conn, &insert_sql(&t.name, &t.columns), &t.rows)?;
        }
        finish_load(conn)
    }

    fn load_collection_json(
        &mut self,
        json: &mut dyn std::io::Read,
        hook: RowHook<'_>,
    ) -> Result<DumpStream, DumpStreamError> {
        let mut sink = JsonLoad {
            conn: self.conn(),
            current: Vec::new(),
            insert: String::new(),
        };
        read_dump_json(json, &mut sink, hook)
    }

    fn purge_rows(&mut self, rows: &PurgeRows) -> StorageResult<()> {
        let conn = self.conn();
        conn.execute_batch("PRAGMA defer_foreign_keys = ON")?;
        let notes = json_list(&rows.notes);
        let cards = json_list(&rows.cards);
        const NOTES: &str = "SELECT value FROM json_each(?1)";
        let tags: Vec<String> = if rows.tags_created_since.is_some() {
            let mut st = conn.prepare(&format!(
                "SELECT DISTINCT tag_id FROM note_tags WHERE note_id IN ({NOTES})"
            ))?;
            let out = st
                .query_map([&notes], |r| r.get(0))?
                .collect::<Result<_, _>>()?;
            out
        } else {
            Vec::new()
        };
        conn.execute(
            &format!(
                "DELETE FROM attribute_values
                 WHERE (entity_kind = 'note' AND entity_id IN ({NOTES}))
                    OR (entity_kind = 'card' AND (entity_id IN (SELECT value FROM json_each(?2))
                        OR entity_id IN (SELECT id FROM cards WHERE note_id IN ({NOTES}))))"
            ),
            params![notes, cards],
        )?;
        conn.execute(
            &format!(
                "DELETE FROM notes_fts WHERE rowid IN
                 (SELECT fts_rowid FROM notes WHERE id IN ({NOTES}) AND fts_rowid IS NOT NULL)"
            ),
            [&notes],
        )?;
        // Undoing an import removes the history it brought too (`review_log` has no cascade
        // since 0501: a purged card's history is kept, an undone import's is not).
        conn.execute(
            &format!(
                "DELETE FROM review_log WHERE card_id IN (SELECT value FROM json_each(?2))
                 OR card_id IN (SELECT id FROM cards WHERE note_id IN ({NOTES}))"
            ),
            params![notes, cards],
        )?;
        conn.execute(
            &format!(
                "DELETE FROM cards WHERE id IN (SELECT value FROM json_each(?2))
                 OR note_id IN ({NOTES})"
            ),
            params![notes, cards],
        )?;
        conn.execute(
            &format!("DELETE FROM notes WHERE id IN ({NOTES})"),
            [&notes],
        )?;
        conn.execute(
            &format!("DELETE FROM attribute_defs WHERE id IN ({NOTES})"),
            [json_list(&rows.attribute_defs)],
        )?;
        conn.execute(
            &format!("DELETE FROM decks WHERE id IN ({NOTES})"),
            [json_list(&rows.decks)],
        )?;
        conn.execute(
            &format!("DELETE FROM notetypes WHERE id IN ({NOTES})"),
            [json_list(&rows.notetypes)],
        )?;
        if let Some(since) = rows.tags_created_since {
            conn.execute(
                &format!(
                    "DELETE FROM tags WHERE id IN ({NOTES}) AND created_at >= ?2
                     AND NOT EXISTS (SELECT 1 FROM note_tags WHERE tag_id = tags.id)"
                ),
                params![json_list(&tags), since.0],
            )?;
        }
        Ok(())
    }
}

/// Applied migration versions, in order.
fn schema_versions(conn: &Connection) -> StorageResult<Vec<u32>> {
    let mut st = conn.prepare("SELECT version FROM schema_migrations ORDER BY version")?;
    let versions = st
        .query_map([], |r| r.get::<_, u32>(0))?
        .collect::<Result<_, _>>()?;
    Ok(versions)
}

/// How one table is dumped.
struct DumpQuery {
    name: String,
    /// `rowid` first when the table has one.
    columns: Vec<String>,
    sql: String,
}

/// The dump's query per user table (see [`CollectionDump`]).
fn dump_queries(conn: &Connection) -> StorageResult<Vec<DumpQuery>> {
    Ok(user_tables(conn)?
        .into_iter()
        .map(|t| {
            let mut columns: Vec<String> = Vec::new();
            if t.with_rowid {
                columns.push("rowid".into());
            }
            columns.extend(t.columns.iter().cloned());
            let select: Vec<String> = columns.iter().map(|c| quote(c)).collect();
            let order = if t.with_rowid {
                "rowid".to_owned()
            } else if t.pk.is_empty() {
                "1".to_owned()
            } else {
                t.pk.iter().map(|c| quote(c)).collect::<Vec<_>>().join(", ")
            };
            let filter = dump_filter(&t.name).map_or(String::new(), |w| format!(" WHERE {w}"));
            let sql = format!(
                "SELECT {} FROM {}{filter} ORDER BY {order}",
                select.join(", "),
                quote(&t.name)
            );
            DumpQuery {
                name: t.name,
                columns,
                sql,
            }
        })
        .collect())
}

fn dump_row(r: &rusqlite::Row<'_>, n: usize) -> rusqlite::Result<Vec<SqlValue>> {
    (0..n).map(|i| r.get_ref(i).map(to_sql_value)).collect()
}

/// The collection as `serde_json` would write its [`CollectionDump`] (same struct and field
/// names, same order), each table read with a cursor as it is written. A storage error is
/// kept in `failed` (the serializer only carries text); rows per table go to `counts`.
struct JsonDump<'c> {
    conn: &'c Connection,
    versions: Vec<u32>,
    tables: Vec<DumpQuery>,
    counts: RefCell<BTreeMap<String, u64>>,
    failed: RefCell<Option<StorageError>>,
}

impl JsonDump<'_> {
    fn fail<E: serde::ser::Error>(&self, e: impl Into<StorageError>) -> E {
        *self.failed.borrow_mut() = Some(e.into());
        E::custom("the dump could not be read")
    }
}

impl Serialize for JsonDump<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut st = s.serialize_struct("CollectionDump", 2)?;
        st.serialize_field("schema_versions", &self.versions)?;
        st.serialize_field("tables", &JsonTables(self))?;
        st.end()
    }
}

struct JsonTables<'a, 'c>(&'a JsonDump<'c>);

impl Serialize for JsonTables<'_, '_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_seq(self.0.tables.iter().map(|q| JsonTable(self.0, q)))
    }
}

struct JsonTable<'a, 'c>(&'a JsonDump<'c>, &'a DumpQuery);

impl Serialize for JsonTable<'_, '_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let mut st = s.serialize_struct("TableDump", 3)?;
        st.serialize_field("name", &self.1.name)?;
        st.serialize_field("columns", &self.1.columns)?;
        st.serialize_field("rows", &JsonRows(self.0, self.1))?;
        st.end()
    }
}

struct JsonRows<'a, 'c>(&'a JsonDump<'c>, &'a DumpQuery);

impl Serialize for JsonRows<'_, '_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeSeq;
        let (w, q) = (self.0, self.1);
        let mut st = w.conn.prepare(&q.sql).map_err(|e| w.fail::<S::Error>(e))?;
        let mut rows = st.query([]).map_err(|e| w.fail::<S::Error>(e))?;
        let n = q.columns.len();
        let mut seq = s.serialize_seq(None)?;
        let mut count = 0u64;
        while let Some(r) = rows.next().map_err(|e| w.fail::<S::Error>(e))? {
            let row = dump_row(r, n).map_err(|e| w.fail::<S::Error>(e))?;
            seq.serialize_element(&row)?;
            count += 1;
        }
        w.counts.borrow_mut().insert(q.name.clone(), count);
        seq.end()
    }
}

/// The table `name` of this schema, if the dump's `columns` all exist there.
fn check_table<'t>(
    current: &'t [TableInfo],
    name: &str,
    columns: &[String],
) -> StorageResult<&'t TableInfo> {
    let info = current
        .iter()
        .find(|t| t.name == name)
        .ok_or_else(|| corrupt(format!("unknown table {name}")))?;
    let cols: HashSet<&str> = info.columns.iter().map(String::as_str).collect();
    for c in columns {
        let rowid_ok = c == "rowid" && info.with_rowid;
        if !rowid_ok && !cols.contains(c.as_str()) {
            return Err(corrupt(format!("unknown column {name}.{c}")));
        }
    }
    Ok(info)
}

/// Empties every table of the dump scope (tables the dump lacks stay empty).
fn empty_tables(conn: &Connection, current: &[TableInfo]) -> StorageResult<()> {
    for t in current {
        conn.execute_batch(&format!("DELETE FROM {}", quote(&t.name)))?;
    }
    Ok(())
}

fn insert_sql(table: &str, columns: &[String]) -> String {
    let cols: Vec<String> = columns.iter().map(|c| quote(c)).collect();
    let marks = vec!["?"; cols.len()].join(", ");
    format!(
        "INSERT INTO {} ({}) VALUES ({marks})",
        quote(table),
        cols.join(", ")
    )
}

fn insert_rows(conn: &Connection, sql: &str, rows: &[Vec<SqlValue>]) -> StorageResult<()> {
    let mut st = conn.prepare_cached(sql)?;
    for row in rows {
        let values = row
            .iter()
            .map(from_sql_value)
            .collect::<StorageResult<Vec<_>>>()?;
        st.execute(params_from_iter(values))?;
    }
    Ok(())
}

/// What every load ends with: derived data rebuilt or invalidated.
fn finish_load(conn: &Connection) -> StorageResult<()> {
    // The search index is derived: rebuild it from the loaded notes instead of trusting
    // the backup's copy (older builds indexed differently).
    conn.execute_batch(super::search::REBUILD_SEARCH_INDEX)?;
    // The v0.6 statistics cache is derived too and never in a backup: drop the local one
    // and move the generation, so no reader trusts it even if the restored history happens
    // to match its counters; the next `stats.refresh_cache` rebuilds it. Older schemas
    // (`upgrade_dump` loads into them) have no cache yet.
    let has_cache: bool = conn.query_row(
        "SELECT count(*) > 0 FROM sqlite_master WHERE type = 'table' AND name = 'stats_cache_meta'",
        [],
        |r| r.get(0),
    )?;
    if has_cache {
        conn.execute_batch(RESET_STATS_CACHE)?;
    }
    Ok(())
}

/// [`InteropWrite::load_collection_json`] into SQLite: the tables are emptied once the dump
/// is known to be at this schema, then each chunk is inserted as it arrives.
struct JsonLoad<'c> {
    conn: &'c Connection,
    current: Vec<TableInfo>,
    /// Insert statement of the table being read.
    insert: String,
}

impl DumpSink for JsonLoad<'_> {
    fn begin(&mut self) -> StorageResult<()> {
        self.conn.execute_batch("PRAGMA defer_foreign_keys = ON")?;
        self.current = user_tables(self.conn)?;
        empty_tables(self.conn, &self.current)
    }

    fn table(&mut self, name: &str, columns: &[String]) -> StorageResult<()> {
        check_table(&self.current, name, columns)?;
        self.insert = insert_sql(name, columns);
        Ok(())
    }

    fn rows(&mut self, rows: &[Vec<SqlValue>]) -> StorageResult<()> {
        insert_rows(self.conn, &self.insert, rows)
    }

    fn finish(&mut self) -> StorageResult<()> {
        finish_load(self.conn)
    }
}

/// Brings a dump taken at an older schema up to this build's schema (`.fcbackup` restore,
/// docs/03 §15): loads it into a private in-memory database migrated only as far as the
/// dump's own versions, runs the remaining migrations there (so data migrations such as
/// re-folding search text apply to the restored rows), and dumps the result. A dump already
/// at the current schema is returned as is. Fails if the dump has a migration this build
/// does not know.
pub fn upgrade_dump(dump: CollectionDump) -> StorageResult<CollectionDump> {
    upgrade_dump_with(dump, crate::migrate::MIGRATIONS)
}

/// Cuts a dump down to what a build with only the migrations `versions` would have written:
/// tables and columns that schema does not have are dropped, `schema_versions` is set.
/// Generic, so fixtures of "a backup from an older build" need no per-migration edits when
/// later tracks add tables. (Rows of tables an older schema derived differently are kept
/// as they are.) Fails on a version this build does not know.
pub fn fit_dump_to_schema(
    mut dump: CollectionDump,
    versions: &[u32],
) -> StorageResult<CollectionDump> {
    let at: Vec<crate::migrate::Migration> = crate::migrate::MIGRATIONS
        .iter()
        .filter(|m| versions.contains(&m.version))
        .copied()
        .collect();
    if at.len() != versions.len() {
        return Err(corrupt(format!("unknown schema versions {versions:?}")));
    }
    let mut conn = Connection::open_in_memory()?;
    crate::migrate::configure_connection(&conn)?;
    crate::migrate::migrate_with(&mut conn, &at)?;
    let schema: std::collections::HashMap<String, Vec<String>> = user_tables(&conn)?
        .into_iter()
        .map(|t| {
            let mut cols = t.columns;
            if t.with_rowid {
                cols.insert(0, "rowid".into());
            }
            (t.name, cols)
        })
        .collect();
    dump.tables.retain(|t| schema.contains_key(&t.name));
    for t in &mut dump.tables {
        let known = &schema[&t.name];
        let keep: Vec<bool> = t.columns.iter().map(|c| known.contains(c)).collect();
        if keep.iter().all(|k| *k) {
            continue;
        }
        let mut k = keep.iter();
        t.columns.retain(|_| *k.next().unwrap_or(&true));
        for row in &mut t.rows {
            let mut k = keep.iter();
            row.retain(|_| *k.next().unwrap_or(&true));
        }
    }
    dump.schema_versions = versions.to_vec();
    Ok(dump)
}

/// [`upgrade_dump`] against an explicit migration list (tests).
pub fn upgrade_dump_with(
    dump: CollectionDump,
    migrations: &[crate::migrate::Migration],
) -> StorageResult<CollectionDump> {
    let all: Vec<u32> = migrations.iter().map(|m| m.version).collect();
    if dump.schema_versions == all {
        return Ok(dump);
    }
    let at: Vec<crate::migrate::Migration> = migrations
        .iter()
        .filter(|m| dump.schema_versions.contains(&m.version))
        .copied()
        .collect();
    if at.len() != dump.schema_versions.len() {
        return Err(corrupt(format!(
            "unknown schema versions {:?}",
            dump.schema_versions
        )));
    }
    let mut conn = Connection::open_in_memory()?;
    crate::migrate::configure_connection(&conn)?;
    crate::migrate::migrate_with(&mut conn, &at)?;
    {
        let guard = std::sync::Mutex::new(conn);
        {
            let lock = guard.lock().map_err(|_| corrupt("poisoned"))?;
            let mut tx = SqliteTx::<WriteMode>::begin(lock)?;
            tx.load_collection(&dump)?;
            crate::WriteTx::commit(Box::new(tx))?;
        }
        conn = guard.into_inner().map_err(|_| corrupt("poisoned"))?;
    }
    crate::migrate::migrate_with(&mut conn, migrations)?;
    let guard = std::sync::Mutex::new(conn);
    let lock = guard.lock().map_err(|_| corrupt("poisoned"))?;
    let tx = SqliteTx::<WriteMode>::begin(lock)?;
    tx.dump_collection()
}
