//! Local HTTP API clients (v0.9, migration `0800_inbox_local_api`).

use super::*;
use fc_core::ApiClientId;

/// One `api_clients` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiClientRecord {
    /// Id.
    pub id: ApiClientId,
    /// Name it gave.
    pub name: String,
    /// `browser_extension`, `app`, `script`.
    pub kind: String,
    /// Browser origin it paired from.
    pub origin: Option<String>,
    /// Scopes asked for.
    pub requested_scopes: Vec<String>,
    /// Scopes granted.
    pub scopes: Vec<String>,
    /// `pending`, `approved`, `denied`, `revoked`.
    pub status: String,
    /// Hex SHA-256 of its token (unique).
    pub token_hash: String,
    /// Paired.
    pub created_at: Timestamp,
    /// Last state change.
    pub updated_at: Timestamp,
    /// Last request.
    pub last_used_at: Option<Timestamp>,
}

/// Local API client reads.
pub trait LocalApiRead {
    /// Every client, newest first.
    fn list_api_clients(&self) -> StorageResult<Vec<ApiClientRecord>>;
    /// One client.
    fn get_api_client(&self, id: ApiClientId) -> StorageResult<Option<ApiClientRecord>>;
    /// The client whose token hashes to `token_hash`.
    fn api_client_by_token_hash(&self, token_hash: &str)
        -> StorageResult<Option<ApiClientRecord>>;
}

/// Local API client writes.
pub trait LocalApiWrite: LocalApiRead {
    /// Inserts or replaces a row (all columns). A token hash already used by another row →
    /// `Constraint { "api_clients_token" }`.
    fn upsert_api_client(&mut self, client: &ApiClientRecord) -> StorageResult<()>;
    /// Deletes a row. Returns whether it existed.
    fn delete_api_client(&mut self, id: ApiClientId) -> StorageResult<bool>;
}
