//! Settings schema registry and value resolution (docs/03 §14). Definitions come from module
//! descriptors (and later providers/plugins); values live in the `settings` table.

use crate::module::ModuleDescriptor;
use fc_core::settings::{SettingDef, SettingScope};
use serde_json::Value;

/// All setting definitions with their owning module.
#[derive(Debug, Clone, Default)]
pub struct SettingsSchema {
    entries: Vec<(String, SettingDef)>,
}

impl SettingsSchema {
    /// Collects definitions from modules.
    pub fn from_modules(modules: &[ModuleDescriptor]) -> Self {
        let entries = modules
            .iter()
            .flat_map(|m| m.settings.iter().map(|d| (m.id.to_owned(), d.clone())))
            .collect();
        Self { entries }
    }

    /// `(module, def)` pairs in module order.
    pub fn entries(&self) -> &[(String, SettingDef)] {
        &self.entries
    }

    /// Definition by key.
    pub fn find(&self, key: &str) -> Option<&SettingDef> {
        self.entries
            .iter()
            .find(|(_, d)| d.key == key)
            .map(|(_, d)| d)
    }
}

/// Resolves a value: first stored value along `chain` (most specific first), else the
/// definition default. Returns the value and the scope it came from (`None` = default).
pub fn resolve_setting(
    def: &SettingDef,
    chain: &[(SettingScope, Option<Value>)],
) -> (Value, Option<SettingScope>) {
    chain
        .iter()
        .find_map(|(scope, v)| v.clone().map(|v| (v, Some(scope.clone()))))
        .unwrap_or_else(|| (def.default.clone(), None))
}
