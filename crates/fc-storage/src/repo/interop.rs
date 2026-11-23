//! Import/export support (v0.5): bulk review history rows and the whole-collection dump/load
//! behind `.fcbackup`. Media go through v0.3's file store ([`ContentRead`]/[`ContentWrite`]);
//! review rows are v0.2's [`ReviewLogRecord`].
//!
//! The dump also streams both ways (v1.4 Z2): [`InteropRead::dump_collection_json`] writes its
//! JSON row by row and [`InteropWrite::load_collection_json`] loads it chunk by chunk, so a
//! backup, its check and a restore are bounded by one chunk of rows, not by the history.

use super::*;
use crate::error::StorageError;
use serde::{Deserialize, Serialize};

mod dump_json;
pub use dump_json::scan_dump_json;
#[cfg(test)]
pub(crate) use dump_json::CHUNK;
pub(crate) use dump_json::{current_schema, read_dump_json, DumpSink};

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

impl CollectionDump {
    /// Rows per table.
    pub fn row_counts(&self) -> std::collections::BTreeMap<String, u64> {
        self.tables
            .iter()
            .map(|t| (t.name.clone(), t.rows.len() as u64))
            .collect()
    }
}

/// What a streamed dump held: its schema and rows per table (every table it lists, empty ones
/// included; on a load, after the row hook).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DumpSummary {
    /// Applied migration versions.
    pub schema_versions: Vec<u32>,
    /// Rows per table.
    pub counts: std::collections::BTreeMap<String, u64>,
}

impl DumpSummary {
    /// The summary of a dump in memory.
    pub fn of(dump: &CollectionDump) -> Self {
        Self {
            schema_versions: dump.schema_versions.clone(),
            counts: dump.row_counts(),
        }
    }
}

/// A dump read from its JSON ([`InteropWrite::load_collection_json`], [`scan_dump_json`]).
#[derive(Debug)]
pub enum DumpStream {
    /// At this build's schema: streamed (loaded, or only scanned).
    Streamed(DumpSummary),
    /// At another schema: read whole and nothing loaded (bring it to this schema with
    /// `fc_storage::upgrade_dump`, then [`InteropWrite::load_collection`]).
    Whole(CollectionDump),
}

/// Why a streamed dump failed.
#[derive(Debug)]
pub enum DumpStreamError {
    /// The JSON side: not a dump (syntax, shape), or the reader/writer under it failed.
    Json(serde_json::Error),
    /// The storage side (or the row hook) refused it.
    Storage(StorageError),
}

impl From<StorageError> for DumpStreamError {
    fn from(e: StorageError) -> Self {
        Self::Storage(e)
    }
}

impl std::fmt::Display for DumpStreamError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Json(e) => e.fmt(f),
            Self::Storage(e) => e.fmt(f),
        }
    }
}

/// Sees every chunk of rows of a streamed dump before it is used, and may change or drop
/// rows (not columns). An error stops the stream.
pub type RowHook<'a> = &'a mut dyn FnMut(&mut TableDump) -> StorageResult<()>;

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
        Ok(self.dump_collection()?.row_counts())
    }
    /// Writes the JSON of [`Self::dump_collection`] into `out`, byte for byte what
    /// `serde_json::to_writer` makes of it, and returns its schema and row counts (v1.4 Z2).
    /// SQLite streams it row by row; the default builds the dump first.
    fn dump_collection_json(
        &self,
        out: &mut dyn std::io::Write,
    ) -> Result<DumpSummary, DumpStreamError> {
        let dump = self.dump_collection()?;
        serde_json::to_writer(out, &dump).map_err(DumpStreamError::Json)?;
        Ok(DumpSummary::of(&dump))
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
    /// [`Self::load_collection`] from a dump's JSON (v1.4 Z2), with `hook` run on every chunk
    /// of rows first (a restore's sanitizers). A dump at this build's schema is loaded as it
    /// streams in (SQLite: memory bounded by one chunk) and its rows per table (after the
    /// hook) are returned; a dump at another schema is returned whole and nothing is loaded.
    /// Fails like `load_collection` (unknown table or column, ragged rows); a failure may leave
    /// part of the dump written: the caller drops the transaction.
    fn load_collection_json(
        &mut self,
        json: &mut dyn std::io::Read,
        hook: RowHook<'_>,
    ) -> Result<DumpStream, DumpStreamError> {
        let mut dump: CollectionDump =
            serde_json::from_reader(json).map_err(DumpStreamError::Json)?;
        if dump.schema_versions != current_schema() {
            return Ok(DumpStream::Whole(dump));
        }
        for t in &mut dump.tables {
            if t.rows.iter().any(|r| r.len() != t.columns.len()) {
                break; // `load_collection` refuses it
            }
            hook(t)?;
        }
        self.load_collection(&dump)?;
        Ok(DumpStream::Streamed(DumpSummary::of(&dump)))
    }
    /// Hard-deletes the rows of a discarded import (see [`PurgeRows`]).
    fn purge_rows(&mut self, rows: &PurgeRows) -> StorageResult<()>;
}
