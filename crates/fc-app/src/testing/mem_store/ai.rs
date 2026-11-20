//! [`MemTx`] repository traits: AI providers, routes, prices, usage (v0.8).

use super::*;
use fc_core::AiProviderId;

/// Default price rows (what migration 0700 seeds).
fn seeded_prices() -> BTreeMap<String, AiPriceRecord> {
    [
        ("claude-opus-5-5", 5.0, 25.0),
        ("claude-sonnet-5-5", 3.0, 15.0),
        ("claude-haiku-5-5", 1.0, 5.0),
    ]
    .into_iter()
    .map(|(m, i, o)| {
        (
            m.to_owned(),
            AiPriceRecord {
                model: m.to_owned(),
                input_per_mtok: i,
                output_per_mtok: o,
                updated_at: Timestamp(0),
            },
        )
    })
    .collect()
}

/// AI tables.
#[derive(Debug, Clone)]
pub struct AiState {
    providers: BTreeMap<AiProviderId, AiProviderRecord>,
    routes: BTreeMap<String, AiRouteRecord>,
    prices: BTreeMap<String, AiPriceRecord>,
    usage: Vec<AiUsageRecord>,
}

impl Default for AiState {
    fn default() -> Self {
        Self {
            providers: BTreeMap::new(),
            routes: BTreeMap::new(),
            prices: seeded_prices(),
            usage: Vec::new(),
        }
    }
}

impl AiRead for MemTx<'_> {
    fn get_ai_provider(&self, id: AiProviderId) -> StorageResult<Option<AiProviderRecord>> {
        Ok(self.s().domains.ai.providers.get(&id).cloned())
    }

    fn list_ai_providers(&self, include_deleted: bool) -> StorageResult<Vec<AiProviderRecord>> {
        let mut v: Vec<AiProviderRecord> = self
            .s()
            .domains
            .ai
            .providers
            .values()
            .filter(|p| include_deleted || p.deleted_at.is_none())
            .cloned()
            .collect();
        v.sort_by(|a, b| {
            (a.position, a.created_at, a.id).cmp(&(b.position, b.created_at, b.id))
        });
        Ok(v)
    }

    fn list_ai_routes(&self) -> StorageResult<Vec<AiRouteRecord>> {
        Ok(self.s().domains.ai.routes.values().cloned().collect())
    }

    fn list_ai_prices(&self) -> StorageResult<Vec<AiPriceRecord>> {
        Ok(self.s().domains.ai.prices.values().cloned().collect())
    }

    fn list_ai_usage(&self, since: Timestamp) -> StorageResult<Vec<AiUsageRecord>> {
        let mut v: Vec<AiUsageRecord> = self
            .s()
            .domains
            .ai
            .usage
            .iter()
            .filter(|u| u.created_at >= since)
            .cloned()
            .collect();
        v.sort_by(|a, b| b.created_at.cmp(&a.created_at).then(b.id.cmp(&a.id)));
        Ok(v)
    }
}

impl AiWrite for MemTx<'_> {
    fn upsert_ai_provider(&mut self, provider: &AiProviderRecord) -> StorageResult<()> {
        self.m()
            .domains
            .ai
            .providers
            .insert(provider.id, provider.clone());
        Ok(())
    }

    fn upsert_ai_route(&mut self, route: &AiRouteRecord) -> StorageResult<()> {
        self.m()
            .domains
            .ai
            .routes
            .insert(route.feature.clone(), route.clone());
        Ok(())
    }

    fn delete_ai_route(&mut self, feature: &str) -> StorageResult<bool> {
        Ok(self.m().domains.ai.routes.remove(feature).is_some())
    }

    fn upsert_ai_price(&mut self, price: &AiPriceRecord) -> StorageResult<()> {
        self.m()
            .domains
            .ai
            .prices
            .insert(price.model.clone(), price.clone());
        Ok(())
    }

    fn delete_ai_price(&mut self, model: &str) -> StorageResult<bool> {
        Ok(self.m().domains.ai.prices.remove(model).is_some())
    }

    fn insert_ai_usage(&mut self, usage: &AiUsageRecord) -> StorageResult<()> {
        self.m().domains.ai.usage.push(usage.clone());
        Ok(())
    }
}
