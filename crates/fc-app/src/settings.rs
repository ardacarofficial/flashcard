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

/// Scopes consulted when resolving a value as seen from `scope`, most specific first
/// (`Deck → its preset → Profile → App`; plugin scopes are isolated).
pub(crate) fn chain_for(
    reads: &dyn fc_storage::Reads,
    scope: &SettingScope,
) -> crate::error::AppResult<Vec<SettingScope>> {
    Ok(match scope {
        SettingScope::Deck(d) => {
            let deck = reads.get_deck(*d)?.ok_or_else(|| {
                crate::support::err_id(
                    fc_api::ErrorCode::NotFoundDeck,
                    format!("deck {d} not found"),
                    d,
                )
            })?;
            fc_core::settings::resolution_chain(Some(*d), deck.preset_id)
        }
        SettingScope::Preset(p) => fc_core::settings::resolution_chain(None, Some(*p)),
        SettingScope::Profile => vec![SettingScope::Profile, SettingScope::App],
        SettingScope::App => vec![SettingScope::App],
        SettingScope::Plugin(_) => vec![scope.clone()],
    })
}

/// Stored values of `key` along the chain of `scope` (input for [`resolve_setting`]).
pub(crate) fn stored_chain(
    reads: &dyn fc_storage::Reads,
    scope: &SettingScope,
    key: &str,
) -> crate::error::AppResult<Vec<(SettingScope, Option<Value>)>> {
    chain_for(reads, scope)?
        .into_iter()
        .map(|s| {
            let v = reads.get_setting(&s, key)?;
            Ok((s, v))
        })
        .collect()
}

/// First stored value of `key` along the chain of `scope` (`None` = schema default applies).
pub(crate) fn resolve_stored(
    reads: &dyn fc_storage::Reads,
    scope: &SettingScope,
    key: &str,
) -> crate::error::AppResult<Option<(Value, SettingScope)>> {
    Ok(stored_chain(reads, scope, key)?
        .into_iter()
        .find_map(|(s, v)| v.map(|v| (v, s))))
}
