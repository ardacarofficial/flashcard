//! Settings (`settings`, keyed by `SettingScope::storage_key()`) and module switches
//! (`module_state`).

use super::{all, exec, json, one, to_json};
use super::{Mode, SqliteTx, WriteMode};
use crate::error::StorageResult;
use crate::repo::{ModuleStateRead, ModuleStateWrite, SettingsRead, SettingsWrite};
use fc_core::settings::SettingScope;
use fc_core::Timestamp;
use rusqlite::params;
use serde_json::Value;

const T: &str = "settings";

impl<M: Mode> SettingsRead for SqliteTx<'_, M> {
    fn get_setting(&self, scope: &SettingScope, key: &str) -> StorageResult<Option<Value>> {
        one(
            self.conn(),
            T,
            "SELECT value FROM settings WHERE scope = ?1 AND key = ?2",
            params![scope.storage_key(), key],
            |r| json(r, 0),
        )
    }

    fn list_settings(&self, scope: &SettingScope) -> StorageResult<Vec<(String, Value)>> {
        all(
            self.conn(),
            T,
            "SELECT key, value FROM settings WHERE scope = ?1 ORDER BY key",
            [scope.storage_key()],
            |r| Ok((r.get(0)?, json(r, 1)?)),
        )
    }
}

impl SettingsWrite for SqliteTx<'_, WriteMode> {
    fn set_setting(
        &mut self,
        scope: &SettingScope,
        key: &str,
        value: &Value,
        now: Timestamp,
    ) -> StorageResult<()> {
        exec(
            self.conn(),
            T,
            "INSERT INTO settings (scope, key, value, updated_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(scope, key) DO UPDATE SET value = excluded.value,
                 updated_at = excluded.updated_at",
            params![scope.storage_key(), key, to_json(T, value)?, now.0],
        )?;
        Ok(())
    }

    fn delete_setting(&mut self, scope: &SettingScope, key: &str) -> StorageResult<bool> {
        Ok(exec(
            self.conn(),
            T,
            "DELETE FROM settings WHERE scope = ?1 AND key = ?2",
            params![scope.storage_key(), key],
        )? > 0)
    }
}

impl<M: Mode> ModuleStateRead for SqliteTx<'_, M> {
    fn list_module_states(&self) -> StorageResult<Vec<(String, bool)>> {
        all(
            self.conn(),
            "module_state",
            "SELECT id, enabled FROM module_state ORDER BY id",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
    }
}

impl ModuleStateWrite for SqliteTx<'_, WriteMode> {
    fn set_module_enabled(&mut self, id: &str, enabled: bool, now: Timestamp) -> StorageResult<()> {
        exec(
            self.conn(),
            "module_state",
            "INSERT INTO module_state (id, enabled, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET enabled = excluded.enabled,
                 updated_at = excluded.updated_at",
            params![id, enabled, now.0],
        )?;
        Ok(())
    }
}
