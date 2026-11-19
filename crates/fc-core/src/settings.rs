//! Schema-driven settings (docs/03 §14). Pure data + validation; storage and resolution
//! against stored values live in the app layer.

use crate::error::{CoreError, CoreResult};
use crate::id::{DeckId, PresetId};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Where a setting value lives. Resolution goes from most specific to most general:
/// `Deck → Preset → Profile → App` (plugins have their own isolated scope).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum SettingScope {
    /// Whole application.
    App,
    /// Current profile.
    Profile,
    /// A deck options preset.
    Preset(PresetId),
    /// A single deck.
    Deck(DeckId),
    /// A plugin's private settings.
    Plugin(String),
}

impl SettingScope {
    /// Storage form used in `settings.scope`: `app`, `profile`, `preset:<id>`, `deck:<id>`,
    /// `plugin:<id>`.
    pub fn storage_key(&self) -> String {
        match self {
            Self::App => "app".into(),
            Self::Profile => "profile".into(),
            Self::Preset(id) => format!("preset:{id}"),
            Self::Deck(id) => format!("deck:{id}"),
            Self::Plugin(id) => format!("plugin:{id}"),
        }
    }

    /// Parses [`SettingScope::storage_key`] output.
    pub fn parse_storage_key(s: &str) -> Option<Self> {
        match s {
            "app" => return Some(Self::App),
            "profile" => return Some(Self::Profile),
            _ => {}
        }
        let (kind, rest) = s.split_once(':')?;
        match kind {
            "preset" => rest.parse().ok().map(Self::Preset),
            "deck" => rest.parse().ok().map(Self::Deck),
            "plugin" if !rest.is_empty() => Some(Self::Plugin(rest.to_owned())),
            _ => None,
        }
    }

    /// Kind of this scope, for [`SettingDef::scopes`].
    pub fn kind(&self) -> SettingScopeKind {
        match self {
            Self::App => SettingScopeKind::App,
            Self::Profile => SettingScopeKind::Profile,
            Self::Preset(_) => SettingScopeKind::Preset,
            Self::Deck(_) => SettingScopeKind::Deck,
            Self::Plugin(_) => SettingScopeKind::Plugin,
        }
    }
}

/// Scope kinds a setting may be set at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum SettingScopeKind {
    /// See [`SettingScope::App`].
    App,
    /// See [`SettingScope::Profile`].
    Profile,
    /// See [`SettingScope::Preset`].
    Preset,
    /// See [`SettingScope::Deck`].
    Deck,
    /// See [`SettingScope::Plugin`].
    Plugin,
}

/// Lookup order for a value: most specific first, `App` last.
pub fn resolution_chain(deck: Option<DeckId>, preset: Option<PresetId>) -> Vec<SettingScope> {
    let mut chain = Vec::with_capacity(4);
    if let Some(d) = deck {
        chain.push(SettingScope::Deck(d));
    }
    if let Some(p) = preset {
        chain.push(SettingScope::Preset(p));
    }
    chain.push(SettingScope::Profile);
    chain.push(SettingScope::App);
    chain
}

/// Value type and constraints of a setting.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum SettingKind {
    /// `true`/`false`.
    Bool,
    /// Integer with optional bounds.
    Integer {
        /// Inclusive minimum.
        min: Option<i64>,
        /// Inclusive maximum.
        max: Option<i64>,
    },
    /// Float with optional bounds.
    Float {
        /// Inclusive minimum.
        min: Option<f64>,
        /// Inclusive maximum.
        max: Option<f64>,
    },
    /// Free text.
    Text {
        /// Maximum length in chars.
        max_len: Option<u32>,
    },
    /// One of a fixed set of string values.
    Choice {
        /// Allowed values (labels come from `<label_key>.<value>` i18n keys).
        options: Vec<String>,
    },
}

/// Schema of one setting. Frontend forms and settings search are generated from these.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct SettingDef {
    /// Dotted key, unique app-wide, prefixed by the owning module (`study.show_timer`).
    pub key: String,
    /// Type and constraints.
    pub kind: SettingKind,
    /// Default value (must satisfy `kind`).
    #[cfg_attr(feature = "ts", ts(type = "unknown"))]
    pub default: Value,
    /// i18n key of the label.
    pub label_key: String,
    /// i18n key of the description.
    pub description_key: Option<String>,
    /// Hidden behind "advanced" (progressive disclosure).
    pub advanced: bool,
    /// Scopes at which the value may be overridden.
    pub scopes: Vec<SettingScopeKind>,
}

/// Validates `value` against `def`.
pub fn validate_setting_value(def: &SettingDef, value: &Value) -> CoreResult<()> {
    let fail = |reason: &str| {
        Err(CoreError::InvalidSettingValue {
            key: def.key.clone(),
            reason: reason.to_owned(),
        })
    };
    match &def.kind {
        SettingKind::Bool => {
            if !value.is_boolean() {
                return fail("expected bool");
            }
        }
        SettingKind::Integer { min, max } => {
            let Some(n) = value.as_i64() else {
                return fail("expected integer");
            };
            if min.is_some_and(|m| n < m) || max.is_some_and(|m| n > m) {
                return fail("out of range");
            }
        }
        SettingKind::Float { min, max } => {
            let Some(n) = value.as_f64() else {
                return fail("expected number");
            };
            if !n.is_finite() || min.is_some_and(|m| n < m) || max.is_some_and(|m| n > m) {
                return fail("out of range");
            }
        }
        SettingKind::Text { max_len } => {
            let Some(s) = value.as_str() else {
                return fail("expected string");
            };
            if max_len.is_some_and(|m| s.chars().count() > m as usize) {
                return fail("too long");
            }
        }
        SettingKind::Choice { options } => {
            let Some(s) = value.as_str() else {
                return fail("expected string");
            };
            if !options.iter().any(|o| o == s) {
                return fail("not an option");
            }
        }
    }
    Ok(())
}

/// Validates a definition (its default must satisfy its kind; at least one scope).
pub fn validate_setting_def(def: &SettingDef) -> CoreResult<()> {
    if def.scopes.is_empty() {
        return Err(CoreError::InvalidSettingValue {
            key: def.key.clone(),
            reason: "no scopes".into(),
        });
    }
    validate_setting_value(def, &def.default)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use uuid::Uuid;

    fn def(kind: SettingKind, default: Value) -> SettingDef {
        SettingDef {
            key: "t.k".into(),
            kind,
            default,
            label_key: "settings.t.k".into(),
            description_key: None,
            advanced: false,
            scopes: vec![SettingScopeKind::App],
        }
    }

    #[test]
    fn validates_values() {
        let d = def(
            SettingKind::Integer {
                min: Some(0),
                max: Some(10),
            },
            json!(5),
        );
        validate_setting_def(&d).unwrap();
        assert!(validate_setting_value(&d, &json!(11)).is_err());
        assert!(validate_setting_value(&d, &json!("5")).is_err());
        let c = def(
            SettingKind::Choice {
                options: vec!["a".into(), "b".into()],
            },
            json!("a"),
        );
        assert!(validate_setting_value(&c, &json!("b")).is_ok());
        assert!(validate_setting_value(&c, &json!("z")).is_err());
        assert!(validate_setting_def(&def(SettingKind::Bool, json!(1))).is_err());
        let t = def(SettingKind::Text { max_len: Some(2) }, json!("ab"));
        assert!(validate_setting_value(&t, &json!("abc")).is_err());
        let f = def(
            SettingKind::Float {
                min: Some(0.7),
                max: Some(0.99),
            },
            json!(0.9),
        );
        assert!(validate_setting_value(&f, &json!(0.5)).is_err());
    }

    #[test]
    fn scope_storage_keys_roundtrip() {
        let d = DeckId(Uuid::from_u128(7));
        for s in [
            SettingScope::App,
            SettingScope::Profile,
            SettingScope::Deck(d),
            SettingScope::Preset(PresetId(Uuid::from_u128(8))),
            SettingScope::Plugin("x".into()),
        ] {
            assert_eq!(SettingScope::parse_storage_key(&s.storage_key()), Some(s));
        }
        assert_eq!(SettingScope::parse_storage_key("deck:nope"), None);
        assert_eq!(SettingScope::parse_storage_key("plugin:"), None);
        assert_eq!(
            serde_json::to_value(SettingScope::Deck(d)).unwrap(),
            json!({"kind": "deck", "id": d.to_string()})
        );
        assert_eq!(
            resolution_chain(Some(d), None).first(),
            Some(&SettingScope::Deck(d))
        );
        assert_eq!(
            resolution_chain(None, None),
            vec![SettingScope::Profile, SettingScope::App]
        );
    }
}
