//! [`MemTx`] repository traits: Settings and module switches.

use super::*;

impl SettingsRead for MemTx<'_> {
    fn get_setting(&self, scope: &SettingScope, key: &str) -> StorageResult<Option<Value>> {
        Ok(self
            .s()
            .settings
            .get(&(scope.storage_key(), key.to_owned()))
            .cloned())
    }

    fn list_settings(&self, scope: &SettingScope) -> StorageResult<Vec<(String, Value)>> {
        let sk = scope.storage_key();
        Ok(self
            .s()
            .settings
            .iter()
            .filter(|((s, _), _)| *s == sk)
            .map(|((_, k), v)| (k.clone(), v.clone()))
            .collect())
    }
}

impl SettingsWrite for MemTx<'_> {
    fn set_setting(
        &mut self,
        scope: &SettingScope,
        key: &str,
        value: &Value,
        _now: Timestamp,
    ) -> StorageResult<()> {
        self.m()
            .settings
            .insert((scope.storage_key(), key.to_owned()), value.clone());
        Ok(())
    }

    fn delete_setting(&mut self, scope: &SettingScope, key: &str) -> StorageResult<bool> {
        Ok(self
            .m()
            .settings
            .remove(&(scope.storage_key(), key.to_owned()))
            .is_some())
    }
}

impl ModuleStateRead for MemTx<'_> {
    fn list_module_states(&self) -> StorageResult<Vec<(String, bool)>> {
        Ok(self
            .s()
            .modules
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect())
    }
}

impl ModuleStateWrite for MemTx<'_> {
    fn set_module_enabled(
        &mut self,
        id: &str,
        enabled: bool,
        _now: Timestamp,
    ) -> StorageResult<()> {
        self.m().modules.insert(id.to_owned(), enabled);
        Ok(())
    }
}
