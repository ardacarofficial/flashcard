//! [`MemTx`] repository traits: Jobs.

use super::*;

impl JobRead for MemTx<'_> {
    fn get_job(&self, id: JobId) -> StorageResult<Option<JobRecord>> {
        Ok(self.s().jobs.get(&id).cloned())
    }

    fn list_jobs(&self, limit: u32) -> StorageResult<Vec<JobRecord>> {
        let mut v: Vec<JobRecord> = self.s().jobs.values().cloned().collect();
        v.sort_by(|a, b| b.created_at.cmp(&a.created_at).then(b.id.cmp(&a.id)));
        v.truncate(limit as usize);
        Ok(v)
    }

    fn list_unfinished_batches(&self) -> StorageResult<Vec<UnfinishedBatchRecord>> {
        let mut v: Vec<UnfinishedBatchRecord> =
            self.s().unfinished_batches.values().cloned().collect();
        v.sort_by(|a, b| a.created_at.cmp(&b.created_at).then(a.batch_id.cmp(&b.batch_id)));
        Ok(v)
    }

    fn unfinished_batch_parts(&self, batch: BatchId) -> StorageResult<Vec<Value>> {
        Ok(self
            .s()
            .unfinished_batch_parts
            .range((batch, 0)..=(batch, u32::MAX))
            .map(|(_, v)| v.clone())
            .collect())
    }
}

impl JobWrite for MemTx<'_> {
    fn upsert_unfinished_batch(&mut self, batch: &UnfinishedBatchRecord) -> StorageResult<()> {
        let rows = &mut self.m().unfinished_batches;
        let mut b = batch.clone();
        if let Some(old) = rows.get(&b.batch_id) {
            b.created_at = old.created_at;
        }
        rows.insert(b.batch_id, b);
        Ok(())
    }

    fn add_unfinished_batch_part(
        &mut self,
        batch: BatchId,
        part: u32,
        inverse: &Value,
    ) -> StorageResult<()> {
        if !self.s().unfinished_batches.contains_key(&batch) {
            return Err(StorageError::NotFound {
                entity: "unfinished_batch",
                id: batch.to_string(),
            });
        }
        self.m()
            .unfinished_batch_parts
            .insert((batch, part), inverse.clone());
        Ok(())
    }

    fn delete_unfinished_batch(&mut self, batch: BatchId) -> StorageResult<bool> {
        self.m()
            .unfinished_batch_parts
            .retain(|(b, _), _| *b != batch);
        Ok(self.m().unfinished_batches.remove(&batch).is_some())
    }

    fn upsert_job(&mut self, job: &JobRecord) -> StorageResult<()> {
        self.m().jobs.insert(job.id, job.clone());
        Ok(())
    }

    fn mark_unfinished_jobs_interrupted(&mut self, now: Timestamp) -> StorageResult<u32> {
        let mut n = 0;
        for j in self.m().jobs.values_mut() {
            if j.status == "running" || j.status == "queued" {
                j.status = "interrupted".into();
                j.updated_at = now;
                n += 1;
            }
        }
        Ok(n)
    }

    fn claim_next_job(&mut self, now: Timestamp) -> StorageResult<Option<JobRecord>> {
        self.claim_next_job_in(now, &[], false)
    }

    fn claim_next_job_in(
        &mut self,
        now: Timestamp,
        kinds: &[&str],
        only: bool,
    ) -> StorageResult<Option<JobRecord>> {
        let next = self
            .s()
            .jobs
            .values()
            .filter(|j| j.status == "queued" && kinds.contains(&j.kind.as_str()) == only)
            .min_by(|a, b| a.created_at.cmp(&b.created_at).then(a.id.cmp(&b.id)))
            .map(|j| j.id);
        let Some(id) = next else {
            return Ok(None);
        };
        let job = self
            .m()
            .jobs
            .get_mut(&id)
            .ok_or_else(|| not_found("job", id))?;
        job.status = "running".into();
        job.updated_at = now;
        Ok(Some(job.clone()))
    }
}
