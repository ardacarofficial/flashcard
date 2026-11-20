//! [`MemTx`] repository traits: local HTTP API clients (v0.9).

use super::*;
use fc_core::ApiClientId;
use fc_storage::repo::ApiClientRecord;

/// `api_clients`.
#[derive(Debug, Clone, Default)]
pub struct LocalApiState {
    clients: BTreeMap<ApiClientId, ApiClientRecord>,
}

impl LocalApiRead for MemTx<'_> {
    fn list_api_clients(&self) -> StorageResult<Vec<ApiClientRecord>> {
        let mut v: Vec<ApiClientRecord> =
            self.s().domains.local_api.clients.values().cloned().collect();
        v.sort_by(|a, b| b.created_at.cmp(&a.created_at).then_with(|| b.id.cmp(&a.id)));
        Ok(v)
    }

    fn get_api_client(&self, id: ApiClientId) -> StorageResult<Option<ApiClientRecord>> {
        Ok(self.s().domains.local_api.clients.get(&id).cloned())
    }

    fn api_client_by_token_hash(
        &self,
        token_hash: &str,
    ) -> StorageResult<Option<ApiClientRecord>> {
        Ok(self
            .s()
            .domains
            .local_api
            .clients
            .values()
            .find(|c| c.token_hash == token_hash)
            .cloned())
    }
}

impl LocalApiWrite for MemTx<'_> {
    fn upsert_api_client(&mut self, client: &ApiClientRecord) -> StorageResult<()> {
        if self
            .api_client_by_token_hash(&client.token_hash)?
            .is_some_and(|other| other.id != client.id)
        {
            return Err(StorageError::Constraint {
                constraint: "api_clients_token".into(),
            });
        }
        self.m()
            .domains
            .local_api
            .clients
            .insert(client.id, client.clone());
        Ok(())
    }

    fn delete_api_client(&mut self, id: ApiClientId) -> StorageResult<bool> {
        Ok(self.m().domains.local_api.clients.remove(&id).is_some())
    }
}
