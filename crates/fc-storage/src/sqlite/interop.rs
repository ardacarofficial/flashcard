//! Import/export support: bulk review log rows, whole-collection dump/load.

use super::study::{review_row, REVIEW_COLS};
use super::{all, count, enum_str, exec, json_list};
use super::{Mode, SqliteTx, WriteMode};
use crate::error::{StorageError, StorageResult};
use crate::repo::{
    CollectionDump, InteropRead, InteropWrite, PurgeRows, ReviewLogRecord, SqlValue, TableDump,
};
use fc_core::CardId;
use rusqlite::types::{Value, ValueRef};
use rusqlite::{params, params_from_iter, Connection};
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
];

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
        let mut versions: Vec<u32> = Vec::new();
        {
            let mut st = conn.prepare("SELECT version FROM schema_migrations ORDER BY version")?;
            for v in st.query_map([], |r| r.get::<_, u32>(0))? {
                versions.push(v?);
            }
        }
        let mut tables = Vec::new();
        for t in user_tables(conn)? {
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
            let sql = format!(
                "SELECT {} FROM {} ORDER BY {order}",
                select.join(", "),
                quote(&t.name)
            );
            let mut st = conn.prepare(&sql)?;
            let n = columns.len();
            let rows: Vec<Vec<SqlValue>> = st
                .query_map([], |r| {
                    (0..n)
                        .map(|i| r.get_ref(i).map(to_sql_value))
                        .collect::<Result<Vec<_>, _>>()
                })?
                .collect::<Result<_, _>>()?;
            tables.push(TableDump {
                name: t.name,
                columns,
                rows,
            });
        }
        Ok(CollectionDump {
            schema_versions: versions,
            tables,
        })
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
        let known: BTreeMap<&str, &TableInfo> =
            current.iter().map(|t| (t.name.as_str(), t)).collect();
        // Validate before deleting anything.
        for t in &dump.tables {
            let info = known
                .get(t.name.as_str())
                .ok_or_else(|| corrupt(format!("unknown table {}", t.name)))?;
            let cols: HashSet<&str> = info.columns.iter().map(String::as_str).collect();
            for c in &t.columns {
                let rowid_ok = c == "rowid" && info.with_rowid;
                if !rowid_ok && !cols.contains(c.as_str()) {
                    return Err(corrupt(format!("unknown column {}.{c}", t.name)));
                }
            }
            if t.rows.iter().any(|r| r.len() != t.columns.len()) {
                return Err(corrupt(format!("ragged rows in {}", t.name)));
            }
        }
        for t in &current {
            conn.execute_batch(&format!("DELETE FROM {}", quote(&t.name)))?;
        }
        for t in &dump.tables {
            if t.rows.is_empty() {
                continue;
            }
            let cols: Vec<String> = t.columns.iter().map(|c| quote(c)).collect();
            let marks = vec!["?"; cols.len()].join(", ");
            let sql = format!(
                "INSERT INTO {} ({}) VALUES ({marks})",
                quote(&t.name),
                cols.join(", ")
            );
            let mut st = conn.prepare(&sql)?;
            for row in &t.rows {
                let values = row
                    .iter()
                    .map(from_sql_value)
                    .collect::<StorageResult<Vec<_>>>()?;
                st.execute(params_from_iter(values))?;
            }
        }
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
