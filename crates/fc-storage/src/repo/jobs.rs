//! Background jobs.

use super::*;

/// One `jobs` row.
#[derive(Debug, Clone, PartialEq)]
pub struct JobRecord {
    /// Id.
    pub id: JobId,
    /// Kind.
    pub kind: String,
    /// `queued|running|succeeded|failed|cancelled|interrupted`.
    pub status: String,
    /// 0.0–1.0.
    pub progress: Option<f64>,
    /// Input.
    pub payload: Value,
    /// Output.
    pub result: Option<Value>,
    /// `ApiError` JSON.
    pub error: Option<Value>,
    /// Created.
    pub created_at: Timestamp,
    /// Updated.
    pub updated_at: Timestamp,
}

/// Job reads.
pub trait JobRead {
    /// Job by id.
    fn get_job(&self, id: JobId) -> StorageResult<Option<JobRecord>>;
    /// Newest first.
    fn list_jobs(&self, limit: u32) -> StorageResult<Vec<JobRecord>>;
}

/// Job writes.
pub trait JobWrite: JobRead {
    /// Inserts or replaces.
    fn upsert_job(&mut self, job: &JobRecord) -> StorageResult<()>;
    /// At startup: `running`/`queued` → `interrupted`. Returns rows changed.
    fn mark_unfinished_jobs_interrupted(&mut self, now: Timestamp) -> StorageResult<u32>;
    /// Claims the oldest `queued` job: sets it `running` with `updated_at = now` and returns
    /// it (`None` if the queue is empty).
    fn claim_next_job(&mut self, now: Timestamp) -> StorageResult<Option<JobRecord>>;
}
