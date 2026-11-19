//! Settings.

use super::*;

/// Settings reads.
pub trait SettingsRead {
    /// Stored value at exactly `scope`.
    fn get_setting(&self, scope: &SettingScope, key: &str) -> StorageResult<Option<Value>>;
    /// All stored values at `scope`.
    fn list_settings(&self, scope: &SettingScope) -> StorageResult<Vec<(String, Value)>>;
}

/// Settings writes.
pub trait SettingsWrite: SettingsRead {
    /// Upserts a value.
    fn set_setting(
        &mut self,
        scope: &SettingScope,
        key: &str,
        value: &Value,
        now: Timestamp,
    ) -> StorageResult<()>;
    /// Removes an override. Returns whether a row existed.
    fn delete_setting(&mut self, scope: &SettingScope, key: &str) -> StorageResult<bool>;
}
