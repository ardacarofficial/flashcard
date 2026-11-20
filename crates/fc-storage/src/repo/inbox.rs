//! Inbox items (v0.9; table `inbox` from `0001`, extended by `0800_inbox_local_api`).

use super::*;
use fc_core::{ApiClientId, InboxItemId};

/// One `inbox` row. `source` and `payload` are the app's JSON (`InboxSource`,
/// `AddInboxItem`); storage keeps them verbatim.
#[derive(Debug, Clone, PartialEq)]
pub struct InboxRecord {
    /// Id.
    pub id: InboxItemId,
    /// Origin (JSON).
    pub source: Value,
    /// Proposal (JSON).
    pub payload: Value,
    /// `pending`, `accepted` or `rejected`.
    pub status: String,
    /// Notes created on accept.
    pub note_ids: Vec<NoteId>,
    /// Local API client that sent it.
    pub client_id: Option<ApiClientId>,
    /// That client's idempotency key (unique per client).
    pub client_ref: Option<String>,
    /// Received.
    pub created_at: Timestamp,
    /// Last state change.
    pub updated_at: Timestamp,
}

/// Inbox reads.
pub trait InboxRead {
    /// Items in `status`, newest first (`created_at`, then id). Cursor `<created_at>:<id>`.
    fn list_inbox(&self, status: &str, page: &PageRequest) -> StorageResult<Page<InboxRecord>>;
    /// One item.
    fn get_inbox_item(&self, id: InboxItemId) -> StorageResult<Option<InboxRecord>>;
    /// Number of items in `status`.
    fn count_inbox(&self, status: &str) -> StorageResult<u32>;
    /// The item a client sent with `client_ref`, if any.
    fn inbox_item_by_client_ref(
        &self,
        client: ApiClientId,
        client_ref: &str,
    ) -> StorageResult<Option<InboxRecord>>;
    /// How much a client has waiting in `status`: item count and payload bytes (JSON text).
    fn inbox_client_usage(&self, client: ApiClientId, status: &str) -> StorageResult<(u32, u64)>;
    /// How many items (any status) a client sent at or after `since` (its daily limit).
    fn inbox_client_added_since(&self, client: ApiClientId, since: Timestamp) -> StorageResult<u32>;
}

/// Inbox writes.
pub trait InboxWrite: InboxRead {
    /// Inserts or replaces a row (all columns). A second row with the same
    /// `(client_id, client_ref)` → `Constraint { "inbox_client_ref" }`.
    fn upsert_inbox_item(&mut self, item: &InboxRecord) -> StorageResult<()>;
    /// Deletes a row. Returns whether it existed.
    fn delete_inbox_item(&mut self, id: InboxItemId) -> StorageResult<bool>;
}
