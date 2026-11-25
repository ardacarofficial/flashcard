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

/// One `unfinished_batches` row: an undo group a job writes across several transactions
/// (migration 0401). Present from before its first part until the group reaches the oplog or
/// is discarded; one left at startup is rolled back.
#[derive(Debug, Clone, PartialEq)]
pub struct UnfinishedBatchRecord {
    /// The group's oplog batch id.
    pub batch_id: BatchId,
    /// Job writing it.
    pub job_id: Option<JobId>,
    /// `started` (nothing written yet) | `writing` (parts committed).
    pub state: String,
    /// Internal command call (JSON `InvokeRequest`) that removes what is committed.
    pub rollback: Value,
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
    /// Unfinished undo groups, oldest first.
    fn list_unfinished_batches(&self) -> StorageResult<Vec<UnfinishedBatchRecord>>;
    /// Inverse calls of an unfinished group's committed parts, oldest part first.
    fn unfinished_batch_parts(&self, batch: BatchId) -> StorageResult<Vec<Value>>;
}

/// Job writes.
pub trait JobWrite: JobRead {
    /// Inserts or replaces.
    fn upsert_job(&mut self, job: &JobRecord) -> StorageResult<()>;
    /// At startup: `running`/`queued` → `interrupted`, except the jobs in `keep` (jobs another
    /// live process is running, v1.4.2 K1). Returns rows changed.
    fn mark_unfinished_jobs_interrupted(
        &mut self,
        now: Timestamp,
        keep: &[JobId],
    ) -> StorageResult<u32>;
    /// Claims the oldest `queued` job: sets it `running` with `updated_at = now` and returns
    /// it (`None` if the queue is empty).
    fn claim_next_job(&mut self, now: Timestamp) -> StorageResult<Option<JobRecord>>;
    /// [`JobWrite::claim_next_job`] restricted to a worker lane: the oldest `queued` job whose
    /// kind is in `kinds` (`only = true`) or is not in `kinds` (`only = false`).
    fn claim_next_job_in(
        &mut self,
        now: Timestamp,
        kinds: &[&str],
        only: bool,
    ) -> StorageResult<Option<JobRecord>>;
    /// Inserts or replaces an unfinished undo group.
    fn upsert_unfinished_batch(&mut self, batch: &UnfinishedBatchRecord) -> StorageResult<()>;
    /// Appends part `part` (its inverse calls) to an unfinished group.
    fn add_unfinished_batch_part(
        &mut self,
        batch: BatchId,
        part: u32,
        inverse: &Value,
    ) -> StorageResult<()>;
    /// Removes one with its parts. Returns whether a row existed.
    fn delete_unfinished_batch(&mut self, batch: BatchId) -> StorageResult<bool>;
}
