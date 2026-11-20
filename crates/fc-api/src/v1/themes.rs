//! Themes (v1.0, docs/02 §31): colour scheme, preset and custom accent. The backend owns the
//! choice and the colour tokens (`fc_core::theme`); the frontend only applies them as CSS
//! variables.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

/// Which colour scheme the app uses (the `app.theme` setting).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ThemeMode {
    /// Follow the operating system.
    #[default]
    System,
    /// Always light.
    Light,
    /// Always dark.
    Dark,
}

/// Response of `themes.current` and `themes.set`: the effective theme.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ThemeDto {
    /// Colour scheme.
    pub mode: ThemeMode,
    /// Preset id (`themes.presets`).
    pub preset: String,
    /// Custom accent `#rrggbb` (`null` = the preset's).
    pub accent: Option<String>,
    /// Tokens of the light scheme: token name (`bg`, `accent-text`…) → `#rrggbb`; the
    /// frontend sets them as `--fc-<token>`.
    pub light: BTreeMap<String, String>,
    /// Tokens of the dark scheme.
    pub dark: BTreeMap<String, String>,
}

/// A built-in preset, with its tokens for previews (swatches).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ThemePresetDto {
    /// Id.
    pub id: String,
    /// i18n key of the name.
    pub name_key: String,
    /// Light scheme tokens.
    pub light: BTreeMap<String, String>,
    /// Dark scheme tokens.
    pub dark: BTreeMap<String, String>,
}

/// Response of `themes.presets`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ThemePresetList {
    /// Presets in menu order.
    pub presets: Vec<ThemePresetDto>,
}

/// `themes.set`: changes what is given (one undo step). `accent: ""` goes back to the
/// preset's accent; an accent must be `#rgb`/`#rrggbb` (`VALIDATION_SETTING_VALUE`).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct SetTheme {
    /// New colour scheme.
    #[serde(default)]
    pub mode: Option<ThemeMode>,
    /// New preset id.
    #[serde(default)]
    pub preset: Option<String>,
    /// New accent (`""` = none).
    #[serde(default)]
    pub accent: Option<String>,
}
