//! [`MemTx`] repository traits: Plugins (v0.7).

use super::*;

/// Plugin tables.
#[derive(Debug, Clone, Default)]
pub struct PluginState {
    plugins: BTreeMap<String, PluginRecord>,
    packages: BTreeMap<(String, String), PluginPackageRecord>,
    kv: BTreeMap<(String, String), Value>,
}

impl PluginRead for MemTx<'_> {
    fn list_plugins(&self, include_deleted: bool) -> StorageResult<Vec<PluginRecord>> {
        Ok(self
            .s()
            .domains
            .plugins
            .plugins
            .values()
            .filter(|p| include_deleted || p.deleted_at.is_none())
            .cloned()
            .collect())
    }

    fn get_plugin(&self, id: &str) -> StorageResult<Option<PluginRecord>> {
        Ok(self.s().domains.plugins.plugins.get(id).cloned())
    }

    fn get_plugin_package(
        &self,
        plugin_id: &str,
        sha256: &str,
        with_wasm: bool,
    ) -> StorageResult<Option<PluginPackageRecord>> {
        Ok(self
            .s()
            .domains
            .plugins
            .packages
            .get(&(plugin_id.to_owned(), sha256.to_owned()))
            .map(|p| {
                let mut p = p.clone();
                if !with_wasm {
                    p.wasm.clear();
                }
                p
            }))
    }

    fn get_plugin_kv(&self, plugin_id: &str, key: &str) -> StorageResult<Option<Value>> {
        Ok(self
            .s()
            .domains
            .plugins
            .kv
            .get(&(plugin_id.to_owned(), key.to_owned()))
            .cloned())
    }

    fn list_plugin_kv(&self, plugin_id: &str, prefix: &str) -> StorageResult<Vec<(String, Value)>> {
        Ok(self
            .s()
            .domains
            .plugins
            .kv
            .iter()
            .filter(|((p, k), _)| p == plugin_id && k.starts_with(prefix))
            .map(|((_, k), v)| (k.clone(), v.clone()))
            .collect())
    }
}

impl PluginWrite for MemTx<'_> {
    fn upsert_plugin(&mut self, record: &PluginRecord) -> StorageResult<()> {
        self.m()
            .domains
            .plugins
            .plugins
            .insert(record.id.clone(), record.clone());
        Ok(())
    }

    fn delete_plugin(&mut self, id: &str) -> StorageResult<()> {
        let state = &mut self.m().domains.plugins;
        state.plugins.remove(id);
        state.kv.retain(|(p, _), _| p != id);
        Ok(())
    }

    fn insert_plugin_package(&mut self, record: &PluginPackageRecord) -> StorageResult<()> {
        self.m()
            .domains
            .plugins
            .packages
            .entry((record.plugin_id.clone(), record.sha256.clone()))
            .or_insert_with(|| record.clone());
        Ok(())
    }

    fn set_plugin_kv(
        &mut self,
        plugin_id: &str,
        key: &str,
        value: Option<&Value>,
        _now: Timestamp,
    ) -> StorageResult<()> {
        let state = &mut self.m().domains.plugins;
        if !state.plugins.contains_key(plugin_id) {
            return Err(not_found("plugin", plugin_id));
        }
        let k = (plugin_id.to_owned(), key.to_owned());
        match value {
            Some(v) => {
                state.kv.insert(k, v.clone());
            }
            None => {
                state.kv.remove(&k);
            }
        }
        Ok(())
    }
}
