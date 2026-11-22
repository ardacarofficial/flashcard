//! SQLite implementation of the repository traits.
//!
//! [`SqliteTx`] wraps a locked connection with an open transaction. Every `*Read` trait is
//! implemented for both modes, `*Write` traits only for [`WriteMode`]. Per-area SQL lives in
//! the sibling modules; this one holds the transaction type and column helpers.

mod ai;
mod attributes;
mod capture_log;
mod cards;
mod content;
mod decks;
mod interop;
pub use interop::{fit_dump_to_schema, upgrade_dump, upgrade_dump_with};
mod automation;
mod inbox;
mod jobs;
mod local_api;
mod maintenance;
mod mcp;
mod meanings;
mod notes;
mod notetypes;
mod oplog;
mod plugins;
mod reading;
pub(crate) mod search;
mod settings;
mod stats;
mod study;
mod subtitle;
mod tags;
mod ux;
mod word_data;
mod words;

use crate::error::{in_table, StorageError, StorageResult};
use crate::store::{ReadTx, WriteTx};
use fc_core::Timestamp;
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ValueRef};
use rusqlite::{Connection, OptionalExtension, Params, Row};
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::fmt::Display;
use std::marker::PhantomData;
use std::str::FromStr;
use std::sync::MutexGuard;

/// Marker: read snapshot.
pub(crate) enum ReadMode {}
/// Marker: write transaction.
pub(crate) enum WriteMode {}

/// Transaction mode: how it begins.
pub(crate) trait Mode {
    const BEGIN: &'static str;
}
impl Mode for ReadMode {
    const BEGIN: &'static str = "BEGIN DEFERRED";
}
impl Mode for WriteMode {
    const BEGIN: &'static str = "BEGIN IMMEDIATE";
}

/// An open transaction on a locked connection. Dropping it without commit rolls back.
pub(crate) struct SqliteTx<'a, M: Mode> {
    conn: MutexGuard<'a, Connection>,
    open: bool,
    _mode: PhantomData<M>,
}

impl<'a, M: Mode> SqliteTx<'a, M> {
    /// Begins a transaction. A transaction left open by a panicking holder is rolled back
    /// first.
    pub(crate) fn begin(conn: MutexGuard<'a, Connection>) -> StorageResult<Self> {
        if !conn.is_autocommit() {
            conn.execute_batch("ROLLBACK")?;
        }
        conn.execute_batch(M::BEGIN)?;
        Ok(Self {
            conn,
            open: true,
            _mode: PhantomData,
        })
    }

    pub(crate) fn conn(&self) -> &Connection {
        &self.conn
    }
}

impl<M: Mode> Drop for SqliteTx<'_, M> {
    fn drop(&mut self) {
        if self.open {
            let _ = self.conn.execute_batch("ROLLBACK");
        }
    }
}

impl ReadTx for SqliteTx<'_, ReadMode> {}

impl WriteTx for SqliteTx<'_, WriteMode> {
    fn commit(mut self: Box<Self>) -> StorageResult<()> {
        self.conn.execute_batch("COMMIT")?;
        self.open = false;
        Ok(())
    }
}

// ------------------------------------------------------------------ query helpers

/// All rows of a query.
pub(crate) fn all<T, P: Params>(
    conn: &Connection,
    table: &'static str,
    sql: &str,
    params: P,
    f: impl FnMut(&Row<'_>) -> rusqlite::Result<T>,
) -> StorageResult<Vec<T>> {
    let mut stmt = conn.prepare_cached(sql).map_err(|e| in_table(e, table))?;
    let rows = stmt.query_map(params, f).map_err(|e| in_table(e, table))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| in_table(e, table))
}

/// At most one row.
pub(crate) fn one<T, P: Params>(
    conn: &Connection,
    table: &'static str,
    sql: &str,
    params: P,
    f: impl FnOnce(&Row<'_>) -> rusqlite::Result<T>,
) -> StorageResult<Option<T>> {
    let mut stmt = conn.prepare_cached(sql).map_err(|e| in_table(e, table))?;
    stmt.query_row(params, f)
        .optional()
        .map_err(|e| in_table(e, table))
}

/// Executes a statement; returns changed rows.
pub(crate) fn exec<P: Params>(
    conn: &Connection,
    table: &'static str,
    sql: &str,
    params: P,
) -> StorageResult<usize> {
    let mut stmt = conn.prepare_cached(sql).map_err(|e| in_table(e, table))?;
    stmt.execute(params).map_err(|e| in_table(e, table))
}

/// `exec` that must change exactly one row, else `NotFound`.
pub(crate) fn exec_one<P: Params>(
    conn: &Connection,
    table: &'static str,
    entity: &'static str,
    id: &dyn Display,
    sql: &str,
    params: P,
) -> StorageResult<()> {
    if exec(conn, table, sql, params)? == 0 {
        return Err(StorageError::NotFound {
            entity,
            id: id.to_string(),
        });
    }
    Ok(())
}

/// Row count as `u32` (saturating).
pub(crate) fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// Serializes a value to JSON text.
pub(crate) fn to_json<T: Serialize + ?Sized>(table: &'static str, v: &T) -> StorageResult<String> {
    serde_json::to_string(v).map_err(|e| StorageError::Corrupt {
        table,
        message: e.to_string(),
    })
}

/// JSON array of display strings, for `IN (SELECT value FROM json_each(?))`.
pub(crate) fn json_list<T: Display>(items: &[T]) -> String {
    let v: Vec<String> = items.iter().map(ToString::to_string).collect();
    serde_json::Value::from(v).to_string()
}

/// Storage string of a serde unit enum (`"standard"`, `"note"`, …).
pub(crate) fn enum_str<T: Serialize>(table: &'static str, v: &T) -> StorageResult<String> {
    match serde_json::to_value(v) {
        Ok(serde_json::Value::String(s)) => Ok(s),
        other => Err(StorageError::Corrupt {
            table,
            message: format!("not a string enum: {other:?}"),
        }),
    }
}

/// Optional timestamp column value.
pub(crate) fn opt_ms(t: Option<Timestamp>) -> Option<i64> {
    t.map(|t| t.0)
}

fn conversion<E: Display>(e: E) -> FromSqlError {
    FromSqlError::Other(e.to_string().into())
}

/// Column decoded with `FromStr` (ids).
pub(crate) struct Parsed<T>(pub T);

impl<T> FromSql for Parsed<T>
where
    T: FromStr,
    T::Err: Display,
{
    fn column_result(v: ValueRef<'_>) -> FromSqlResult<Self> {
        v.as_str()?.parse().map(Parsed).map_err(conversion)
    }
}

/// Column holding JSON text.
pub(crate) struct Json<T>(pub T);

impl<T: DeserializeOwned> FromSql for Json<T> {
    fn column_result(v: ValueRef<'_>) -> FromSqlResult<Self> {
        serde_json::from_str(v.as_str()?)
            .map(Json)
            .map_err(conversion)
    }
}

/// Column holding a serde unit-enum storage string.
pub(crate) struct EnumStr<T>(pub T);

impl<T: DeserializeOwned> FromSql for EnumStr<T> {
    fn column_result(v: ValueRef<'_>) -> FromSqlResult<Self> {
        // Straight from the borrowed text (v1.0: no `String` + JSON value per column; review
        // and card rows carry several of these, millions of times in statistics).
        T::deserialize(
            serde::de::value::BorrowedStrDeserializer::<serde_json::Error>::new(v.as_str()?),
        )
        .map(EnumStr)
        .map_err(conversion)
    }
}

/// Reads an id column.
pub(crate) fn id<T>(r: &Row<'_>, i: usize) -> rusqlite::Result<T>
where
    T: FromStr,
    T::Err: Display,
{
    Ok(r.get::<_, Parsed<T>>(i)?.0)
}

/// Reads a nullable id column.
pub(crate) fn opt_id<T>(r: &Row<'_>, i: usize) -> rusqlite::Result<Option<T>>
where
    T: FromStr,
    T::Err: Display,
{
    Ok(r.get::<_, Option<Parsed<T>>>(i)?.map(|p| p.0))
}

/// Reads a timestamp column.
pub(crate) fn ts(r: &Row<'_>, i: usize) -> rusqlite::Result<Timestamp> {
    Ok(Timestamp(r.get(i)?))
}

/// Reads a nullable timestamp column.
pub(crate) fn opt_ts(r: &Row<'_>, i: usize) -> rusqlite::Result<Option<Timestamp>> {
    Ok(r.get::<_, Option<i64>>(i)?.map(Timestamp))
}

/// Reads a JSON column.
pub(crate) fn json<T: DeserializeOwned>(r: &Row<'_>, i: usize) -> rusqlite::Result<T> {
    Ok(r.get::<_, Json<T>>(i)?.0)
}

/// Reads an enum-string column.
pub(crate) fn enum_col<T: DeserializeOwned>(r: &Row<'_>, i: usize) -> rusqlite::Result<T> {
    Ok(r.get::<_, EnumStr<T>>(i)?.0)
}
