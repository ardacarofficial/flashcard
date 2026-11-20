//! [`MemTx`] repository traits: Operation log.

use super::*;

impl OplogRead for MemTx<'_> {
    fn list_oplog(&self, page: &PageRequest) -> StorageResult<Page<OplogRecord>> {
        let items: Vec<OplogRecord> = self.s().oplog.iter().rev().cloned().collect();
        offset_page(items, page)
    }

    fn list_oplog_batches(
        &self,
        before: Option<BatchId>,
        limit: u32,
    ) -> StorageResult<Vec<OplogBatch>> {
        let mut batches: Vec<OplogBatch> = Vec::new();
        for r in &self.s().oplog {
            if !batches.iter().any(|b| b.batch_id == r.batch_id) {
                batches.push(OplogBatch {
                    batch_id: r.batch_id,
                    kind: r.kind,
                    reverts_batch_id: r.reverts_batch_id,
                    summary: r.summary.clone(),
                    created_at: r.created_at,
                });
            }
        }
        batches.reverse();
        let start = match before {
            None => 0,
            Some(b) => batches
                .iter()
                .position(|x| x.batch_id == b)
                .map_or(batches.len(), |p| p + 1),
        };
        Ok(batches
            .into_iter()
            .skip(start)
            .take(limit as usize)
            .collect())
    }

    fn oplog_watermark(&self) -> StorageResult<i64> {
        Ok(i64::try_from(self.s().oplog.len()).unwrap_or(i64::MAX))
    }

    fn oplog_actors_after(&self, after: i64) -> StorageResult<Vec<serde_json::Value>> {
        let skip = usize::try_from(after).unwrap_or(0);
        let mut out: Vec<serde_json::Value> = Vec::new();
        for r in self.s().oplog.iter().skip(skip) {
            if !out.contains(&r.actor) {
                out.push(r.actor.clone());
            }
        }
        Ok(out)
    }

    fn oplog_batch_entries(&self, batch: BatchId) -> StorageResult<Vec<OplogRecord>> {
        let mut v: Vec<OplogRecord> = self
            .s()
            .oplog
            .iter()
            .filter(|r| r.batch_id == batch)
            .cloned()
            .collect();
        v.sort_by_key(|r| r.seq);
        Ok(v)
    }
}

impl OplogWrite for MemTx<'_> {
    fn append_oplog(&mut self, record: &OplogRecord) -> StorageResult<()> {
        self.m().oplog.push(record.clone());
        Ok(())
    }
}
