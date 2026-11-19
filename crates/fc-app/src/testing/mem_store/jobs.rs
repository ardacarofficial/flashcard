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
}

impl JobWrite for MemTx<'_> {
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
        let next = self
            .s()
            .jobs
            .values()
            .filter(|j| j.status == "queued")
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
