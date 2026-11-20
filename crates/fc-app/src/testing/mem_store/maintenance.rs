//! [`MemTx`] repository traits: drafts and the integrity check (v1.0).

use super::*;

/// `drafts`.
#[derive(Debug, Clone, Default)]
pub struct MaintenanceState {
    drafts: BTreeMap<String, DraftRecord>,
}

impl MaintenanceRead for MemTx<'_> {
    fn list_drafts(&self) -> StorageResult<Vec<DraftRecord>> {
        let mut v: Vec<DraftRecord> = self.s().domains.maintenance.drafts.values().cloned().collect();
        v.sort_by(|a, b| b.updated_at.cmp(&a.updated_at).then_with(|| a.key.cmp(&b.key)));
        Ok(v)
    }

    fn get_draft(&self, key: &str) -> StorageResult<Option<DraftRecord>> {
        Ok(self.s().domains.maintenance.drafts.get(key).cloned())
    }

    fn integrity_check(&self, _quick: bool, _max_rows: u32) -> StorageResult<IntegrityRecord> {
        Ok(IntegrityRecord {
            rows: vec!["ok".to_owned()],
            foreign_key_violations: 0,
        })
    }
}

impl MaintenanceWrite for MemTx<'_> {
    fn upsert_draft(&mut self, draft: &DraftRecord) -> StorageResult<()> {
        let drafts = &mut self.m().domains.maintenance.drafts;
        let created_at = drafts.get(&draft.key).map_or(draft.created_at, |d| d.created_at);
        drafts.insert(
            draft.key.clone(),
            DraftRecord {
                created_at,
                ..draft.clone()
            },
        );
        Ok(())
    }

    fn delete_draft(&mut self, key: &str) -> StorageResult<bool> {
        Ok(self.m().domains.maintenance.drafts.remove(key).is_some())
    }
}
