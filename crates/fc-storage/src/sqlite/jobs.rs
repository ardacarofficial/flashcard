//! Background jobs (skeleton: enqueue/update = upsert, claim, startup recovery).

use super::{all, count, exec, id, json, one, opt_id, to_json, ts};
use super::{Mode, SqliteTx, WriteMode};
use crate::error::StorageResult;
use crate::repo::{JobRead, JobRecord, JobWrite, UnfinishedBatchRecord};
use fc_core::{BatchId, JobId, Timestamp};
use rusqlite::{params, Row};
use serde_json::Value;

const T: &str = "jobs";
const COLS: &str = "id, kind, status, progress, payload, result, error, created_at, updated_at";

const B: &str = "unfinished_batches";
const B_COLS: &str = "batch_id, job_id, state, rollback, created_at, updated_at";

fn batch_row(r: &Row<'_>) -> rusqlite::Result<UnfinishedBatchRecord> {
    Ok(UnfinishedBatchRecord {
        batch_id: id(r, 0)?,
        job_id: opt_id(r, 1)?,
        state: r.get(2)?,
        rollback: json(r, 3)?,
        created_at: ts(r, 4)?,
        updated_at: ts(r, 5)?,
    })
}

fn job_row(r: &Row<'_>) -> rusqlite::Result<JobRecord> {
    Ok(JobRecord {
        id: id(r, 0)?,
        kind: r.get(1)?,
        status: r.get(2)?,
        progress: r.get(3)?,
        payload: json(r, 4)?,
        result: r.get::<_, Option<super::Json<Value>>>(5)?.map(|j| j.0),
        error: r.get::<_, Option<super::Json<Value>>>(6)?.map(|j| j.0),
        created_at: ts(r, 7)?,
        updated_at: ts(r, 8)?,
    })
}

impl<M: Mode> JobRead for SqliteTx<'_, M> {
    fn get_job(&self, job: JobId) -> StorageResult<Option<JobRecord>> {
        one(
            self.conn(),
            T,
            &format!("SELECT {COLS} FROM jobs WHERE id = ?1"),
            [job.to_string()],
            job_row,
        )
    }

    fn list_jobs(&self, limit: u32) -> StorageResult<Vec<JobRecord>> {
        all(
            self.conn(),
            T,
            &format!("SELECT {COLS} FROM jobs ORDER BY created_at DESC, id DESC LIMIT ?1"),
            [limit],
            job_row,
        )
    }

    fn list_unfinished_batches(&self) -> StorageResult<Vec<UnfinishedBatchRecord>> {
        all(
            self.conn(),
            B,
            &format!("SELECT {B_COLS} FROM unfinished_batches ORDER BY created_at, batch_id"),
            [],
            batch_row,
        )
    }

    fn unfinished_batch_parts(&self, batch: BatchId) -> StorageResult<Vec<Value>> {
        all(
            self.conn(),
            "unfinished_batch_parts",
            "SELECT inverse FROM unfinished_batch_parts WHERE batch_id = ?1 ORDER BY part",
            [batch.to_string()],
            |r| json(r, 0),
        )
    }
}

impl JobWrite for SqliteTx<'_, WriteMode> {
    fn upsert_job(&mut self, j: &JobRecord) -> StorageResult<()> {
        exec(
            self.conn(),
            T,
            &format!(
                "INSERT INTO jobs ({COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT(id) DO UPDATE SET kind = excluded.kind, status = excluded.status,
                     progress = excluded.progress, payload = excluded.payload,
                     result = excluded.result, error = excluded.error,
                     created_at = excluded.created_at, updated_at = excluded.updated_at"
            ),
            params![
                j.id.to_string(),
                j.kind,
                j.status,
                j.progress,
                to_json(T, &j.payload)?,
                j.result.as_ref().map(|v| to_json(T, v)).transpose()?,
                j.error.as_ref().map(|v| to_json(T, v)).transpose()?,
                j.created_at.0,
                j.updated_at.0,
            ],
        )?;
        Ok(())
    }

    fn mark_unfinished_jobs_interrupted(&mut self, now: Timestamp) -> StorageResult<u32> {
        let n = exec(
            self.conn(),
            T,
            "UPDATE jobs SET status = 'interrupted', updated_at = ?1
             WHERE status IN ('queued', 'running')",
            [now.0],
        )?;
        Ok(count(n))
    }

    fn claim_next_job(&mut self, now: Timestamp) -> StorageResult<Option<JobRecord>> {
        let next: Option<JobId> = one(
            self.conn(),
            T,
            "SELECT id FROM jobs WHERE status = 'queued' ORDER BY created_at, id LIMIT 1",
            [],
            |r| id(r, 0),
        )?;
        let Some(job) = next else { return Ok(None) };
        exec(
            self.conn(),
            T,
            "UPDATE jobs SET status = 'running', updated_at = ?2 WHERE id = ?1",
            params![job.to_string(), now.0],
        )?;
        self.get_job(job)
    }

    fn claim_next_job_in(
        &mut self,
        now: Timestamp,
        kinds: &[&str],
        only: bool,
    ) -> StorageResult<Option<JobRecord>> {
        let list = serde_json::to_string(kinds).unwrap_or_else(|_| "[]".into());
        let sql = if only {
            "SELECT id FROM jobs WHERE status = 'queued'
               AND kind IN (SELECT value FROM json_each(?1))
             ORDER BY created_at, id LIMIT 1"
        } else {
            "SELECT id FROM jobs WHERE status = 'queued'
               AND kind NOT IN (SELECT value FROM json_each(?1))
             ORDER BY created_at, id LIMIT 1"
        };
        let next: Option<JobId> = one(self.conn(), T, sql, params![list], |r| id(r, 0))?;
        let Some(job) = next else { return Ok(None) };
        exec(
            self.conn(),
            T,
            "UPDATE jobs SET status = 'running', updated_at = ?2 WHERE id = ?1",
            params![job.to_string(), now.0],
        )?;
        self.get_job(job)
    }

    fn upsert_unfinished_batch(&mut self, b: &UnfinishedBatchRecord) -> StorageResult<()> {
        exec(
            self.conn(),
            B,
            &format!(
                "INSERT INTO unfinished_batches ({B_COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(batch_id) DO UPDATE SET job_id = excluded.job_id,
                     state = excluded.state, rollback = excluded.rollback,
                     updated_at = excluded.updated_at"
            ),
            params![
                b.batch_id.to_string(),
                b.job_id.map(|j| j.to_string()),
                b.state,
                to_json(B, &b.rollback)?,
                b.created_at.0,
                b.updated_at.0
            ],
        )?;
        Ok(())
    }

    fn add_unfinished_batch_part(
        &mut self,
        batch: BatchId,
        part: u32,
        inverse: &Value,
    ) -> StorageResult<()> {
        const P: &str = "unfinished_batch_parts";
        exec(
            self.conn(),
            P,
            "INSERT INTO unfinished_batch_parts (batch_id, part, inverse) VALUES (?1, ?2, ?3)",
            params![batch.to_string(), part, to_json(P, inverse)?],
        )?;
        Ok(())
    }

    fn delete_unfinished_batch(&mut self, batch: BatchId) -> StorageResult<bool> {
        exec(
            self.conn(),
            "unfinished_batch_parts",
            "DELETE FROM unfinished_batch_parts WHERE batch_id = ?1",
            [batch.to_string()],
        )?;
        let n = exec(
            self.conn(),
            B,
            "DELETE FROM unfinished_batches WHERE batch_id = ?1",
            [batch.to_string()],
        )?;
        Ok(n > 0)
    }
}
