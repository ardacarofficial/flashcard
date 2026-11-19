//! Settings DTOs (schema-driven, docs/03 §14).

use fc_core::settings::{SettingDef, SettingScope};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

/// A setting definition with its owning module.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct SettingSchemaEntry {
    /// Module id that declares the setting.
    pub module: String,
    /// Definition.
    pub def: SettingDef,
}

/// Response of `settings.schema`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct SettingsSchema {
    /// All settings of enabled modules.
    pub settings: Vec<SettingSchemaEntry>,
}

/// `settings.get`: resolves values as seen from `scope` (e.g. a deck scope falls back to
/// its preset, then profile, then app, then the schema default).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct GetSettings {
    /// Scope to resolve from.
    pub scope: SettingScope,
    /// Keys to resolve (`null` = all).
    pub keys: Option<Vec<String>>,
}

/// A resolved setting value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct SettingValueDto {
    /// Key.
    pub key: String,
    /// Effective value.
    #[ts(type = "unknown")]
    pub value: Value,
    /// Scope the value came from (`null` = schema default).
    pub source: Option<SettingScope>,
}

/// Response of `settings.get`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct SettingValues {
    /// Values.
    pub values: Vec<SettingValueDto>,
}

/// `settings.set`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SetSetting {
    /// Scope to write.
    pub scope: SettingScope,
    /// Key.
    pub key: String,
    /// Value (validated against the schema).
    #[ts(type = "unknown")]
    pub value: Value,
}

/// `settings.reset`: removes the override at `scope`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ResetSetting {
    /// Scope.
    pub scope: SettingScope,
    /// Key.
    pub key: String,
}
