//! Background jobs (skeleton: enqueue/update = upsert, claim, startup recovery).

use super::{all, count, exec, id, json, one, to_json, ts};
use super::{Mode, SqliteTx, WriteMode};
use crate::error::StorageResult;
use crate::repo::{JobRead, JobRecord, JobWrite};
use fc_core::{JobId, Timestamp};
use rusqlite::{params, Row};
use serde_json::Value;

const T: &str = "jobs";
const COLS: &str = "id, kind, status, progress, payload, result, error, created_at, updated_at";

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
}
