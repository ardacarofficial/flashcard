//! Module descriptors (docs/03 §11). Every feature is a module that can be switched off in
//! settings; the frontend draws its menus from `modules.list`.

use fc_api::v1::modules::UiContribution;
use fc_core::settings::SettingDef;

/// Static description of a module.
#[derive(Debug, Clone, PartialEq)]
pub struct ModuleDescriptor {
    /// Id (`decks`, `attributes`, …). Equals `ApiMethod::MODULE` of its methods.
    pub id: &'static str,
    /// i18n key of the name (`modules.<id>.name`).
    pub name_key: &'static str,
    /// i18n key of the description (`modules.<id>.description`).
    pub description_key: &'static str,
    /// Enabled on a fresh install.
    pub enabled_by_default: bool,
    /// `false` for the backbone (core, decks, notes, notetypes).
    pub can_disable: bool,
    /// Settings owned by the module (keys prefixed with the module id).
    pub settings: Vec<SettingDef>,
    /// UI contributions.
    pub ui: Vec<UiContribution>,
}

/// Main-menu entry pointing at a frontend route.
pub fn main_menu(id: &str, label_key: &str, icon: &str, route: &str, order: i32) -> UiContribution {
    UiContribution {
        slot: fc_api::v1::modules::UiSlot::MainMenu,
        id: id.to_owned(),
        label_key: label_key.to_owned(),
        icon: Some(icon.to_owned()),
        route: Some(route.to_owned()),
        method: None,
        order,
    }
}

/// Settings page entry.
pub fn settings_page(id: &str, label_key: &str, route: &str, order: i32) -> UiContribution {
    UiContribution {
        slot: fc_api::v1::modules::UiSlot::SettingsPage,
        id: id.to_owned(),
        label_key: label_key.to_owned(),
        icon: None,
        route: Some(route.to_owned()),
        method: None,
        order,
    }
}

/// A boolean setting available at the given scopes.
pub fn bool_setting(
    key: &str,
    default: bool,
    scopes: &[fc_core::settings::SettingScopeKind],
) -> SettingDef {
    SettingDef {
        key: key.to_owned(),
        kind: fc_core::settings::SettingKind::Bool,
        default: serde_json::Value::Bool(default),
        label_key: format!("settings.{key}.label"),
        description_key: Some(format!("settings.{key}.description")),
        advanced: false,
        scopes: scopes.to_vec(),
    }
}

/// A choice setting.
pub fn choice_setting(key: &str, options: &[&str], default: &str) -> SettingDef {
    SettingDef {
        key: key.to_owned(),
        kind: fc_core::settings::SettingKind::Choice {
            options: options.iter().map(|o| (*o).to_owned()).collect(),
        },
        default: serde_json::Value::String(default.to_owned()),
        label_key: format!("settings.{key}.label"),
        description_key: Some(format!("settings.{key}.description")),
        advanced: false,
        scopes: vec![fc_core::settings::SettingScopeKind::App],
    }
}
