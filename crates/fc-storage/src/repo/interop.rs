//! Import/export support (v0.5): bulk review history rows and the whole-collection dump/load
//! behind `.fcbackup`. Media go through v0.3's file store ([`ContentRead`]/[`ContentWrite`]);
//! review rows are v0.2's [`ReviewLogRecord`].

use super::*;
use serde::{Deserialize, Serialize};

/// A column value in a dump. Reals travel as their exact decimal text, blobs as hex, so a
/// dump → JSON → load cycle is bit-exact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", content = "v", rename_all = "snake_case")]
pub enum SqlValue {
    /// NULL.
    Null,
    /// INTEGER.
    Integer(i64),
    /// REAL (shortest round-trip decimal text).
    Real(String),
    /// TEXT.
    Text(String),
    /// BLOB (lowercase hex).
    Blob(String),
}

/// All rows of one table.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableDump {
    /// Table name.
    pub name: String,
    /// Column names; `rowid` first when the table has one (insertion order matters, e.g.
    /// the oplog).
    pub columns: Vec<String>,
    /// Rows in rowid/primary key order.
    pub rows: Vec<Vec<SqlValue>>,
}

/// Every user table of the collection (not `schema_migrations`, FTS shadow tables or in-flight
/// job state). Media files are not part of it (see `MediaStore` in the `io` module).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectionDump {
    /// Applied migration versions.
    pub schema_versions: Vec<u32>,
    /// Tables, sorted by name.
    pub tables: Vec<TableDump>,
}

/// Rows an unfinished import wrote, hard-deleted when it is discarded (cancel/failure between
/// its chunks; see `io.import.discard`). Unlike trashing, nothing is left behind.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PurgeRows {
    /// Decks.
    pub decks: Vec<DeckId>,
    /// Note types.
    pub notetypes: Vec<NoteTypeId>,
    /// Notes (with their cards, tags links, search rows and classification values).
    pub notes: Vec<NoteId>,
    /// Cards (with their review history and classification values).
    pub cards: Vec<CardId>,
    /// Classification definitions (with their values).
    pub attribute_defs: Vec<AttributeDefId>,
    /// Tags carried by the purged notes that were created at or after this time are deleted
    /// too once no note references them (`None` = keep tags).
    pub tags_created_since: Option<Timestamp>,
}

/// Import/export reads.
pub trait InteropRead {
    /// Review history of `cards`, oldest first.
    fn review_logs_for_cards(&self, cards: &[CardId]) -> StorageResult<Vec<ReviewLogRecord>>;
    /// Dumps every table (see [`CollectionDump`]). SQLite only.
    fn dump_collection(&self) -> StorageResult<CollectionDump>;
    /// Rows per table as [`Self::dump_collection`] would dump them, without reading them
    /// (v1.4 Z1: backup verification counts what it restored; SQLite counts in place).
    fn dump_row_counts(&self) -> StorageResult<std::collections::BTreeMap<String, u64>> {
        Ok(self
            .dump_collection()?
            .tables
            .iter()
            .map(|t| (t.name.clone(), t.rows.len() as u64))
            .collect())
    }
}

/// Import/export writes.
pub trait InteropWrite: InteropRead {
    /// Inserts review log rows; rows whose id exists are skipped. Returns rows inserted.
    fn insert_review_logs(&mut self, rows: &[ReviewLogRecord]) -> StorageResult<u32>;
    /// Replaces the whole collection with `dump` (tables missing from the dump are emptied,
    /// columns missing from it take their defaults; media files are left alone — the caller
    /// re-adds the backup's files, which are content-addressed). Foreign keys are checked at commit. SQLite only.
    fn load_collection(&mut self, dump: &CollectionDump) -> StorageResult<()>;
    /// Hard-deletes the rows of a discarded import (see [`PurgeRows`]).
    fn purge_rows(&mut self, rows: &PurgeRows) -> StorageResult<()>;
}
