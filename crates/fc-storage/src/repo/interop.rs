//! Import/export support (v0.5): media bytes, review history rows, and the whole-collection
//! dump/load behind `.fcbackup`.
//!
//! Media bytes live in `media_blobs` (migration 0400) next to the `media` metadata row until a
//! file-based media store exists; keeping them in the database also makes backups complete.

use super::*;
use fc_core::card::{CardState, Rating};
use fc_core::{ReviewLogId, SessionId};
use serde::{Deserialize, Serialize};

/// One `media` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaRecord {
    /// SHA-256 hex of the content.
    pub hash: String,
    /// MIME type.
    pub mime: String,
    /// Size in bytes.
    pub size: u64,
    /// Original file name.
    pub original_name: Option<String>,
    /// First stored.
    pub created_at: Timestamp,
}

/// One `review_log` row.
#[derive(Debug, Clone, PartialEq)]
pub struct ReviewLogRecord {
    /// Id.
    pub id: ReviewLogId,
    /// Card.
    pub card_id: CardId,
    /// Session.
    pub session_id: Option<SessionId>,
    /// Answer.
    pub rating: Rating,
    /// State before.
    pub state_before: CardState,
    /// State after.
    pub state_after: CardState,
    /// Scheduled interval (days).
    pub scheduled_days: i64,
    /// Days since the previous review.
    pub elapsed_days: i64,
    /// Time on card (ms).
    pub duration_ms: i64,
    /// Time to answer (ms).
    pub answer_duration_ms: Option<i64>,
    /// Hints used.
    pub hints_used: u32,
    /// Mode (`normal`, …).
    pub mode: String,
    /// FSRS stability after.
    pub stability_after: Option<f64>,
    /// FSRS difficulty after.
    pub difficulty_after: Option<f64>,
    /// When.
    pub created_at: Timestamp,
}

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

/// Every user table of the collection (not `schema_migrations`, FTS shadow tables or media
/// bytes).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CollectionDump {
    /// Applied migration versions.
    pub schema_versions: Vec<u32>,
    /// Tables, sorted by name.
    pub tables: Vec<TableDump>,
}

/// Import/export reads.
pub trait InteropRead {
    /// Media metadata.
    fn get_media(&self, hash: &str) -> StorageResult<Option<MediaRecord>>;
    /// Media content.
    fn media_bytes(&self, hash: &str) -> StorageResult<Option<Vec<u8>>>;
    /// All media with stored content, by hash.
    fn list_media(&self) -> StorageResult<Vec<MediaRecord>>;
    /// Review history of `cards`, oldest first.
    fn review_logs_for_cards(&self, cards: &[CardId]) -> StorageResult<Vec<ReviewLogRecord>>;
    /// Dumps every table (see [`CollectionDump`]). SQLite only.
    fn dump_collection(&self) -> StorageResult<CollectionDump>;
}

/// Import/export writes.
pub trait InteropWrite: InteropRead {
    /// Stores media (content-addressed: an existing hash is left as is). Returns whether it
    /// was new.
    fn put_media(&mut self, record: &MediaRecord, bytes: &[u8]) -> StorageResult<bool>;
    /// Inserts review log rows; rows whose id exists are skipped. Returns rows inserted.
    fn insert_review_logs(&mut self, rows: &[ReviewLogRecord]) -> StorageResult<u32>;
    /// Replaces the whole collection with `dump` (tables missing from the dump are emptied,
    /// columns missing from it take their defaults; media bytes are removed and re-added by
    /// the caller). Foreign keys are checked at commit. SQLite only.
    fn load_collection(&mut self, dump: &CollectionDump) -> StorageResult<()>;
}
