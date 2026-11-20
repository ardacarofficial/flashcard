//! `plugin.toml` (docs/05 §3).
//!
//! ```toml
//! id = "com.example.autofill"
//! name = "Auto fill"
//! version = "1.0.0"
//! api_version = "1"
//! entry = "plugin.wasm"
//! min_app_version = "0.7.0"
//! permissions = ["hooks:before:notes.create", "notes:read"]
//!
//! [contributes]
//! commands = ["fill-all"]
//! settings = "settings.schema.json"
//! locales = "locales"
//! ```
//!
//! Every contribution needs its permission (`study_modes` → `ui:study_mode`, `card_types` →
//! `card_types:provide`, …): the user approves what the plugin may add, and a revoked
//! permission switches the contribution off.

use crate::abi::SUPPORTED_API_VERSIONS;
use crate::models::ModelSpec;
use crate::permission::Permission;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// A plugin manifest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    /// Reverse-DNS id (`com.example.autofill`).
    pub id: String,
    /// Display name (not translated; contributions use the plugin's locale files).
    pub name: String,
    /// Semantic version (`1.2.0`).
    pub version: String,
    /// Plugin API version the plugin targets (`"1"`).
    pub api_version: String,
    /// Authors.
    #[serde(default)]
    pub authors: Vec<String>,
    /// One-line description.
    #[serde(default)]
    pub description: String,
    /// WASM file inside the package.
    #[serde(default = "default_entry")]
    pub entry: String,
    /// Oldest app version that runs it.
    #[serde(default)]
    pub min_app_version: Option<String>,
    /// Requested permissions.
    #[serde(default)]
    pub permissions: Vec<Permission>,
    /// Contribution points.
    #[serde(default)]
    pub contributes: Contributes,
}

fn default_entry() -> String {
    "plugin.wasm".into()
}

/// Contribution points (docs/05 §6).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Contributes {
    /// Commands (palette, shortcuts, toolbar buttons). Needs `ui:command`.
    #[serde(default)]
    pub commands: Vec<String>,
    /// Study modes (own session flow through the declarative UI). Needs `ui:study_mode`.
    #[serde(default)]
    pub study_modes: Vec<String>,
    /// Full-screen views (declarative UI). Needs `ui:view`.
    #[serde(default)]
    pub views: Vec<String>,
    /// Card types (`CardTypeEngine`). Needs `card_types:provide`.
    #[serde(default)]
    pub card_types: Vec<String>,
    /// TTS providers (`TtsProvider`). Needs `tts:provide`.
    #[serde(default)]
    pub tts: Vec<String>,
    /// Main menu entries opening a view or study mode. Needs `ui:menu`.
    #[serde(default)]
    pub menu_items: Vec<MenuItem>,
    /// Sidebar panels showing a view. Needs `ui:sidebar`.
    #[serde(default)]
    pub sidebar_panels: Vec<PanelItem>,
    /// Buttons under the card running a command. Needs `ui:card_toolbar`.
    #[serde(default)]
    pub card_toolbar: Vec<ToolbarItem>,
    /// Settings schema file (JSON array of setting definitions, keys local to the plugin).
    #[serde(default)]
    pub settings: Option<String>,
    /// Folder with `<lang>.json` translation files (flat `key → text`).
    #[serde(default)]
    pub locales: Option<String>,
    /// Large files the plugin needs (voices, models). Needs `models:manage`.
    #[serde(default)]
    pub models: Vec<ModelSpec>,
}

/// A main menu entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MenuItem {
    /// Id (label: the plugin's `menu.<id>` translation).
    pub id: String,
    /// View or study mode it opens.
    pub view: String,
    /// Icon key.
    #[serde(default)]
    pub icon: Option<String>,
}

/// A sidebar panel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PanelItem {
    /// Id (label: `panels.<id>`).
    pub id: String,
    /// View shown in the panel.
    pub view: String,
}

/// A card toolbar button.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolbarItem {
    /// Id (label: `toolbar.<id>`).
    pub id: String,
    /// Command it runs (receives `{card_id}`).
    pub command: String,
    /// Icon key.
    #[serde(default)]
    pub icon: Option<String>,
}

/// Why a manifest was rejected.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ManifestError {
    /// Not valid TOML / wrong types.
    #[error("manifest does not parse: {0}")]
    Parse(String),
    /// A field breaks a rule.
    #[error("manifest field {field}: {reason}")]
    Invalid {
        /// Field.
        field: String,
        /// Reason.
        reason: String,
    },
    /// `api_version` is not one this host runs.
    #[error("plugin API version {0:?} is not supported")]
    UnsupportedApiVersion(String),
    /// `min_app_version` is newer than the app.
    #[error("plugin needs app {required}, this is {current}")]
    AppTooOld {
        /// Required.
        required: String,
        /// Current.
        current: String,
    },
}

fn invalid(field: &str, reason: impl Into<String>) -> ManifestError {
    ManifestError::Invalid {
        field: field.to_owned(),
        reason: reason.into(),
    }
}

/// Parses `major.minor.patch` (pre-release/build suffix ignored for ordering).
pub fn parse_version(v: &str) -> Option<(u64, u64, u64)> {
    let core = v.split(['-', '+']).next()?;
    let mut parts = core.split('.');
    let mut next = || parts.next().and_then(|p| p.parse::<u64>().ok());
    let version = (next()?, next()?, next()?);
    parts.next().is_none().then_some(version)
}

fn valid_id(id: &str) -> bool {
    let segments: Vec<&str> = id.split('.').collect();
    id.len() <= 64
        && segments.len() >= 2
        && segments.iter().all(|s| {
            !s.is_empty()
                && s.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
        })
}

/// Contribution ids: `[a-z0-9_-]{1,64}`.
pub fn valid_local_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

fn safe_path(p: &str) -> bool {
    !p.is_empty()
        && !p.starts_with('/')
        && !p.contains('\\')
        && !p.contains(':')
        && p.split('/').all(|s| s != ".." && s != ".")
}

impl Manifest {
    /// Parses TOML and validates against `app_version`.
    pub fn parse(toml_text: &str, app_version: &str) -> Result<Self, ManifestError> {
        let m: Manifest =
            toml::from_str(toml_text).map_err(|e| ManifestError::Parse(e.to_string()))?;
        m.validate(app_version)?;
        Ok(m)
    }

    /// Checks every rule (see module docs).
    pub fn validate(&self, app_version: &str) -> Result<(), ManifestError> {
        if !valid_id(&self.id) {
            return Err(invalid(
                "id",
                "expected a reverse-DNS id like com.example.name",
            ));
        }
        if self.name.trim().is_empty() || self.name.chars().count() > 80 {
            return Err(invalid("name", "1–80 characters"));
        }
        if parse_version(&self.version).is_none() {
            return Err(invalid("version", "expected major.minor.patch"));
        }
        if !SUPPORTED_API_VERSIONS.contains(&self.api_version.as_str()) {
            return Err(ManifestError::UnsupportedApiVersion(
                self.api_version.clone(),
            ));
        }
        if let Some(min) = &self.min_app_version {
            let required =
                parse_version(min).ok_or_else(|| invalid("min_app_version", "expected x.y.z"))?;
            let current = parse_version(app_version).unwrap_or((0, 0, 0));
            if required > current {
                return Err(ManifestError::AppTooOld {
                    required: min.clone(),
                    current: app_version.to_owned(),
                });
            }
        }
        if !safe_path(&self.entry) || !self.entry.ends_with(".wasm") {
            return Err(invalid("entry", "a relative .wasm path inside the package"));
        }
        let c = &self.contributes;
        for (field, path) in [
            ("contributes.settings", &c.settings),
            ("contributes.locales", &c.locales),
        ] {
            if let Some(p) = path {
                if !safe_path(p.trim_end_matches('/')) {
                    return Err(invalid(field, "a relative path inside the package"));
                }
            }
        }
        let mut seen = BTreeSet::new();
        for p in &self.permissions {
            if !seen.insert(p.as_str()) {
                return Err(invalid("permissions", format!("{p} listed twice")));
            }
        }
        let ids = |field: &str, list: &[String]| -> Result<BTreeSet<String>, ManifestError> {
            let mut set = BTreeSet::new();
            for id in list {
                if !valid_local_id(id) {
                    return Err(invalid(field, format!("invalid id {id:?}")));
                }
                if !set.insert(id.clone()) {
                    return Err(invalid(field, format!("{id} listed twice")));
                }
            }
            Ok(set)
        };
        let commands = ids("contributes.commands", &c.commands)?;
        let modes = ids("contributes.study_modes", &c.study_modes)?;
        let views = ids("contributes.views", &c.views)?;
        ids("contributes.card_types", &c.card_types)?;
        ids("contributes.tts", &c.tts)?;
        ids(
            "contributes.menu_items",
            &c.menu_items
                .iter()
                .map(|m| m.id.clone())
                .collect::<Vec<_>>(),
        )?;
        ids(
            "contributes.sidebar_panels",
            &c.sidebar_panels
                .iter()
                .map(|m| m.id.clone())
                .collect::<Vec<_>>(),
        )?;
        ids(
            "contributes.card_toolbar",
            &c.card_toolbar
                .iter()
                .map(|m| m.id.clone())
                .collect::<Vec<_>>(),
        )?;
        ids(
            "contributes.models",
            &c.models.iter().map(|m| m.id.clone()).collect::<Vec<_>>(),
        )?;
        for m in &c.menu_items {
            if !views.contains(&m.view) && !modes.contains(&m.view) {
                return Err(invalid(
                    "contributes.menu_items",
                    format!("unknown view {}", m.view),
                ));
            }
        }
        for p in &c.sidebar_panels {
            if !views.contains(&p.view) {
                return Err(invalid(
                    "contributes.sidebar_panels",
                    format!("unknown view {}", p.view),
                ));
            }
        }
        for t in &c.card_toolbar {
            if !commands.contains(&t.command) {
                return Err(invalid(
                    "contributes.card_toolbar",
                    format!("unknown command {}", t.command),
                ));
            }
        }
        for m in &c.models {
            m.validate().map_err(|r| invalid("contributes.models", r))?;
        }
        let needs: [(&str, bool, &str); 9] = [
            ("contributes.commands", !c.commands.is_empty(), "ui:command"),
            (
                "contributes.study_modes",
                !c.study_modes.is_empty(),
                "ui:study_mode",
            ),
            ("contributes.views", !c.views.is_empty(), "ui:view"),
            (
                "contributes.card_types",
                !c.card_types.is_empty(),
                "card_types:provide",
            ),
            ("contributes.tts", !c.tts.is_empty(), "tts:provide"),
            (
                "contributes.menu_items",
                !c.menu_items.is_empty(),
                "ui:menu",
            ),
            (
                "contributes.sidebar_panels",
                !c.sidebar_panels.is_empty(),
                "ui:sidebar",
            ),
            (
                "contributes.card_toolbar",
                !c.card_toolbar.is_empty(),
                "ui:card_toolbar",
            ),
            ("contributes.models", !c.models.is_empty(), "models:manage"),
        ];
        for (field, used, permission) in needs {
            if used && !self.requests(permission) {
                return Err(invalid(field, format!("needs permission {permission}")));
            }
        }
        Ok(())
    }

    /// Whether the manifest requests `permission`.
    pub fn requests(&self, permission: &str) -> bool {
        self.permissions.iter().any(|p| p.as_str() == permission)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OK: &str = r#"
        id = "com.example.demo"
        name = "Demo"
        version = "1.2.0"
        api_version = "1"
        min_app_version = "0.7.0"
        permissions = ["notes:read", "ui:study_mode", "ui:menu", "hooks:before:notes.create"]

        [contributes]
        study_modes = ["drill"]
        menu_items = [{ id = "open", view = "drill" }]
        settings = "settings.schema.json"
        locales = "locales"
    "#;

    #[test]
    fn parses_and_validates() {
        let m = Manifest::parse(OK, "0.7.0").unwrap();
        assert_eq!(m.id, "com.example.demo");
        assert_eq!(m.entry, "plugin.wasm");
        assert!(m.requests("ui:study_mode"));
        assert_eq!(m.contributes.study_modes, vec!["drill".to_owned()]);
    }

    #[test]
    fn rejects_bad_manifests() {
        let bad = |from: &str, to: &str| Manifest::parse(&OK.replace(from, to), "0.7.0");
        assert!(matches!(
            bad("com.example.demo", "demo"),
            Err(ManifestError::Invalid { .. })
        ));
        assert!(matches!(
            bad("\"1.2.0\"", "\"1.2\""),
            Err(ManifestError::Invalid { .. })
        ));
        assert_eq!(
            bad("api_version = \"1\"", "api_version = \"2\""),
            Err(ManifestError::UnsupportedApiVersion("2".into()))
        );
        assert!(matches!(
            bad("\"0.7.0\"", "\"0.9.0\""),
            Err(ManifestError::AppTooOld { .. })
        ));
        // A contribution without its permission.
        assert!(matches!(
            bad("\"ui:study_mode\", ", ""),
            Err(ManifestError::Invalid { field, .. }) if field == "contributes.study_modes"
        ));
        // Menu entry pointing nowhere.
        assert!(matches!(
            bad("view = \"drill\"", "view = \"nope\""),
            Err(ManifestError::Invalid { .. })
        ));
        // Unknown / forbidden permission.
        assert!(matches!(
            bad("\"notes:read\"", "\"plugins:write\""),
            Err(ManifestError::Parse(_))
        ));
        // Path escaping the package.
        assert!(matches!(
            bad("\"locales\"", "\"../x\""),
            Err(ManifestError::Invalid { .. })
        ));
    }

    #[test]
    fn versions() {
        assert_eq!(parse_version("0.7.0"), Some((0, 7, 0)));
        assert_eq!(parse_version("1.2.3-beta.1"), Some((1, 2, 3)));
        assert_eq!(parse_version("1.2"), None);
        assert_eq!(parse_version("1.2.3.4"), None);
    }
}
