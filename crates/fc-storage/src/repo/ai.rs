//! AI providers, per-feature routes, the price table and usage rows (v0.8, migration 0700).
//! API keys are never stored: they live in the OS keychain.

use super::*;
use fc_core::AiProviderId;

/// One `ai_providers` row.
#[derive(Debug, Clone, PartialEq)]
pub struct AiProviderRecord {
    /// Id.
    pub id: AiProviderId,
    /// `anthropic | openai | openai_compatible | ollama`.
    pub kind: String,
    /// Display name.
    pub name: String,
    /// Endpoint base URL.
    pub base_url: String,
    /// Model used when a feature names none (`""` = none chosen).
    pub default_model: String,
    /// Known model ids (fetched from the provider or typed in).
    pub models: Vec<String>,
    /// The default provider.
    pub is_default: bool,
    /// Display order.
    pub position: i32,
    /// Created.
    pub created_at: Timestamp,
    /// Updated.
    pub updated_at: Timestamp,
    /// Trashed.
    pub deleted_at: Option<Timestamp>,
}

/// One `ai_routes` row: which provider/model a feature uses.
#[derive(Debug, Clone, PartialEq)]
pub struct AiRouteRecord {
    /// Feature id (`generate_cards`, …).
    pub feature: String,
    /// Provider (`None` = default provider).
    pub provider_id: Option<AiProviderId>,
    /// Model (`None` = the provider's default model).
    pub model: Option<String>,
    /// Updated.
    pub updated_at: Timestamp,
}

/// One `ai_prices` row (USD per million tokens).
#[derive(Debug, Clone, PartialEq)]
pub struct AiPriceRecord {
    /// Model id (exact, or a prefix of dated model ids).
    pub model: String,
    /// Input price.
    pub input_per_mtok: f64,
    /// Output price.
    pub output_per_mtok: f64,
    /// Updated.
    pub updated_at: Timestamp,
}

/// One `ai_usage` row.
#[derive(Debug, Clone, PartialEq)]
pub struct AiUsageRecord {
    /// Id (UUIDv7 text).
    pub id: String,
    /// Provider id.
    pub provider: String,
    /// Model that answered.
    pub model: String,
    /// Feature id.
    pub feature: String,
    /// Input tokens.
    pub input_tokens: u32,
    /// Output tokens.
    pub output_tokens: u32,
    /// Estimated cost in USD (`None` = no price known).
    pub cost_estimate: Option<f64>,
    /// When.
    pub created_at: Timestamp,
}

/// AI reads.
pub trait AiRead {
    /// Provider by id (trashed too).
    fn get_ai_provider(&self, id: AiProviderId) -> StorageResult<Option<AiProviderRecord>>;
    /// Providers by `position`, then creation.
    fn list_ai_providers(&self, include_deleted: bool) -> StorageResult<Vec<AiProviderRecord>>;
    /// Every route, by feature.
    fn list_ai_routes(&self) -> StorageResult<Vec<AiRouteRecord>>;
    /// The price table, by model.
    fn list_ai_prices(&self) -> StorageResult<Vec<AiPriceRecord>>;
    /// Usage rows with `created_at >= since`, newest first.
    fn list_ai_usage(&self, since: Timestamp) -> StorageResult<Vec<AiUsageRecord>>;
}

/// AI writes.
pub trait AiWrite: AiRead {
    /// Inserts or replaces a provider row (all columns, `deleted_at` included).
    fn upsert_ai_provider(&mut self, provider: &AiProviderRecord) -> StorageResult<()>;
    /// Inserts or replaces a route.
    fn upsert_ai_route(&mut self, route: &AiRouteRecord) -> StorageResult<()>;
    /// Removes a route. Returns whether it existed.
    fn delete_ai_route(&mut self, feature: &str) -> StorageResult<bool>;
    /// Inserts or replaces a price.
    fn upsert_ai_price(&mut self, price: &AiPriceRecord) -> StorageResult<()>;
    /// Removes a price. Returns whether it existed.
    fn delete_ai_price(&mut self, model: &str) -> StorageResult<bool>;
    /// Appends a usage row.
    fn insert_ai_usage(&mut self, usage: &AiUsageRecord) -> StorageResult<()>;
}
