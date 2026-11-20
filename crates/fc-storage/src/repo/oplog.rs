//! Operation log (undo/redo).

use super::*;

/// Oplog entry kind (`oplog.kind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OplogRecordKind {
    /// Normal command.
    Do,
    /// Undo of `reverts_batch_id`.
    Undo,
    /// Redo of `reverts_batch_id` (an undo batch).
    Redo,
}

/// `oplog.command` of a **history barrier** entry: a `do` batch that undo cannot cross
/// (written when the collection is replaced from a backup or trashed items are purged for
/// good — older inverses may refer to rows that no longer exist). It has no inverse and is
/// not a method of the catalog.
pub const OPLOG_BARRIER_COMMAND: &str = "v1.oplog.barrier";

/// One `oplog` row. JSON columns are opaque to storage (the app owns their shape:
/// `inverse` = `[InvokeRequest]`, `actor` = `Actor`, `summary` = `I18nMessage`).
#[derive(Debug, Clone, PartialEq)]
pub struct OplogRecord {
    /// Id.
    pub id: OplogId,
    /// Batch.
    pub batch_id: BatchId,
    /// Order within the batch (0-based).
    pub seq: u32,
    /// Qualified method name.
    pub command: String,
    /// Params.
    pub payload: Value,
    /// Inverse calls.
    pub inverse: Value,
    /// Actor JSON.
    pub actor: Value,
    /// Kind.
    pub kind: OplogRecordKind,
    /// For undo/redo.
    pub reverts_batch_id: Option<BatchId>,
    /// Summary JSON.
    pub summary: Value,
    /// Time.
    pub created_at: Timestamp,
}

/// One batch, as seen by the undo machinery.
#[derive(Debug, Clone, PartialEq)]
pub struct OplogBatch {
    /// Batch id.
    pub batch_id: BatchId,
    /// Kind of its entries.
    pub kind: OplogRecordKind,
    /// For undo/redo.
    pub reverts_batch_id: Option<BatchId>,
    /// Summary of the first entry.
    pub summary: Value,
    /// The batch contains a history barrier entry ([`OPLOG_BARRIER_COMMAND`]).
    pub barrier: bool,
    /// Time of the first entry.
    pub created_at: Timestamp,
}

/// Oplog reads.
pub trait OplogRead {
    /// Entries, newest first (cursor = opaque).
    fn list_oplog(&self, page: &PageRequest) -> StorageResult<Page<OplogRecord>>;
    /// Batches newest first, starting before `before` (exclusive) if given.
    fn list_oplog_batches(
        &self,
        before: Option<BatchId>,
        limit: u32,
    ) -> StorageResult<Vec<OplogBatch>>;
    /// Entries of a batch in `seq` order.
    fn oplog_batch_entries(&self, batch: BatchId) -> StorageResult<Vec<OplogRecord>>;
}

/// Oplog writes (append-only).
pub trait OplogWrite: OplogRead {
    /// Appends an entry.
    fn append_oplog(&mut self, record: &OplogRecord) -> StorageResult<()>;
    /// Deletes every entry (a restored collection starts a fresh history).
    fn clear_oplog(&mut self) -> StorageResult<()>;
}
