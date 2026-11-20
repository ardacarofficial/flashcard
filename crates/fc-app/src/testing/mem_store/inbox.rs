//! [`MemTx`] repository traits: inbox items (v0.9).

use super::*;
use fc_core::{ApiClientId, InboxItemId};
use fc_storage::repo::InboxRecord;

/// `inbox`.
#[derive(Debug, Clone, Default)]
pub struct InboxState {
    items: BTreeMap<InboxItemId, InboxRecord>,
}

impl InboxRead for MemTx<'_> {
    fn list_inbox(&self, status: &str, page: &PageRequest) -> StorageResult<Page<InboxRecord>> {
        let mut v: Vec<InboxRecord> = self
            .s()
            .domains
            .inbox
            .items
            .values()
            .filter(|i| i.status == status)
            .cloned()
            .collect();
        v.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| b.id.cmp(&a.id)));
        offset_page(v, page)
    }

    fn get_inbox_item(&self, id: InboxItemId) -> StorageResult<Option<InboxRecord>> {
        Ok(self.s().domains.inbox.items.get(&id).cloned())
    }

    fn count_inbox(&self, status: &str) -> StorageResult<u32> {
        let n = self
            .s()
            .domains
            .inbox
            .items
            .values()
            .filter(|i| i.status == status)
            .count();
        Ok(u32::try_from(n).unwrap_or(u32::MAX))
    }

    fn inbox_item_by_client_ref(
        &self,
        client: ApiClientId,
        client_ref: &str,
    ) -> StorageResult<Option<InboxRecord>> {
        Ok(self
            .s()
            .domains
            .inbox
            .items
            .values()
            .find(|i| i.client_id == Some(client) && i.client_ref.as_deref() == Some(client_ref))
            .cloned())
    }
}

impl InboxWrite for MemTx<'_> {
    fn upsert_inbox_item(&mut self, item: &InboxRecord) -> StorageResult<()> {
        if let (Some(c), Some(r)) = (item.client_id, item.client_ref.as_deref()) {
            if self
                .inbox_item_by_client_ref(c, r)?
                .is_some_and(|other| other.id != item.id)
            {
                return Err(StorageError::Constraint {
                    constraint: "inbox_client_ref".into(),
                });
            }
        }
        self.m().domains.inbox.items.insert(item.id, item.clone());
        Ok(())
    }

    fn delete_inbox_item(&mut self, id: InboxItemId) -> StorageResult<bool> {
        Ok(self.m().domains.inbox.items.remove(&id).is_some())
    }
}
