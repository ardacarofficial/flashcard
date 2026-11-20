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

/// Permission to change a setting that names a **program the app runs** (an executable path,
/// e.g. `reading.ffmpeg_path`). Like `mcp:write`, only the user, the CLI and the app itself
/// hold it (`Registry::permits`, `registry::USER_ONLY_PERMISSIONS`): a plugin, automation, local-API or MCP client that
/// could point it elsewhere could make the app run any program.
pub const EXEC_PERMISSION: &str = "settings:exec";

/// Settings that name a program the app runs ([`EXEC_PERMISSION`]). They are validated with
/// [`validate_executable`] when set **and** when used, and dropped from restored backups.
pub const EXEC_SETTINGS: &[&str] = &[crate::modules::reading::FFMPEG_PATH];

/// Whether `key` is one of the [`EXEC_SETTINGS`].
pub fn is_exec_setting(key: &str) -> bool {
    EXEC_SETTINGS.contains(&key)
}

/// Why `path` is not an acceptable executable: it must be an absolute path to an existing
/// regular file that is executable (Unix: an `x` bit; Windows: an `.exe`).
pub fn executable_problem(path: &str) -> Option<&'static str> {
    let p = std::path::Path::new(path);
    if path.trim() != path || !p.is_absolute() {
        return Some("the program must be given by its absolute path");
    }
    let Ok(meta) = std::fs::metadata(p) else {
        return Some("the program does not exist");
    };
    if !meta.is_file() {
        return Some("the program is not a file");
    }
    if cfg!(windows) {
        let exe = p
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("exe"));
        if !exe {
            return Some("the program must be an .exe file");
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        if meta.permissions().mode() & 0o111 == 0 {
            return Some("the program is not executable");
        }
    }
    None
}

/// Validates the value of an [`EXEC_SETTINGS`] entry: an empty text (= the default program
/// looked up on `PATH`) or an [`executable_problem`]-free path.
pub fn validate_executable(key: &str, value: &Value) -> Result<(), fc_core::error::CoreError> {
    let bad = |reason: &str| fc_core::error::CoreError::InvalidSettingValue {
        key: key.to_owned(),
        reason: reason.to_owned(),
    };
    let Some(text) = value.as_str() else {
        return Err(bad("expected text"));
    };
    if text.is_empty() {
        return Ok(());
    }
    match executable_problem(text) {
        None => Ok(()),
        Some(reason) => Err(bad(reason)),
    }
}
