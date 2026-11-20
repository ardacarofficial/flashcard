//! Plugins (v0.7): installed plugins (`plugins`, migration 0600 columns), their packages
//! (`plugin_packages`) and private storage (`plugin_kv`).

use super::*;
use serde::{Deserialize, Serialize};

/// One `plugins` row. Serializable: undo/redo carry it whole (`plugins.restore`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PluginRecord {
    /// Plugin id.
    pub id: String,
    /// Active version.
    pub version: String,
    /// Active manifest (JSON form).
    pub manifest: Value,
    /// Switched on by the user.
    pub enabled: bool,
    /// Granted permissions.
    pub granted: Vec<String>,
    /// Active package (`plugin_packages.sha256`).
    pub package_sha256: String,
    /// Consecutive failures.
    pub failures: u32,
    /// Last failure.
    pub last_error: Option<String>,
    /// Switched off by the app.
    pub auto_disabled: bool,
    /// Installed.
    pub installed_at: Timestamp,
    /// Last change.
    pub updated_at: Timestamp,
    /// Uninstalled (soft).
    pub deleted_at: Option<Timestamp>,
}

/// One `plugin_packages` row.
#[derive(Debug, Clone, PartialEq)]
pub struct PluginPackageRecord {
    /// Plugin id.
    pub plugin_id: String,
    /// SHA-256 of `wasm`.
    pub sha256: String,
    /// Version.
    pub version: String,
    /// Manifest (JSON form).
    pub manifest: Value,
    /// Module bytes (empty when read without them).
    pub wasm: Vec<u8>,
    /// Settings schema (JSON array).
    pub settings: Value,
    /// Translations (JSON object lang → key → text).
    pub locales: Value,
    /// Stored.
    pub created_at: Timestamp,
}

/// Plugin reads.
pub trait PluginRead {
    /// Plugins by id (`include_deleted`: uninstalled ones too).
    fn list_plugins(&self, include_deleted: bool) -> StorageResult<Vec<PluginRecord>>;
    /// One plugin (uninstalled included).
    fn get_plugin(&self, id: &str) -> StorageResult<Option<PluginRecord>>;
    /// A package; `with_wasm = false` leaves `wasm` empty (cheap).
    fn get_plugin_package(
        &self,
        plugin_id: &str,
        sha256: &str,
        with_wasm: bool,
    ) -> StorageResult<Option<PluginPackageRecord>>;
    /// A private storage value.
    fn get_plugin_kv(&self, plugin_id: &str, key: &str) -> StorageResult<Option<Value>>;
    /// Private storage entries whose key starts with `prefix`, by key.
    fn list_plugin_kv(&self, plugin_id: &str, prefix: &str) -> StorageResult<Vec<(String, Value)>>;
}

/// Plugin writes.
pub trait PluginWrite: PluginRead {
    /// Inserts or replaces a plugin row.
    fn upsert_plugin(&mut self, record: &PluginRecord) -> StorageResult<()>;
    /// Removes a plugin row for good (its storage goes with it; packages stay).
    fn delete_plugin(&mut self, id: &str) -> StorageResult<()>;
    /// Stores a package (no-op if that `(plugin_id, sha256)` exists).
    fn insert_plugin_package(&mut self, record: &PluginPackageRecord) -> StorageResult<()>;
    /// Sets (`Some`) or removes (`None`) a private storage value.
    fn set_plugin_kv(
        &mut self,
        plugin_id: &str,
        key: &str,
        value: Option<&Value>,
        now: Timestamp,
    ) -> StorageResult<()>;
}
