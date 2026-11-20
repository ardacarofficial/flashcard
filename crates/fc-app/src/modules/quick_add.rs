//! `quick_add` module (v0.9, docs/06 §7): a global shortcut opens a small always-on-top
//! window that adds a note (an ordinary `notes.create`) while the app is in the background.
//! The module only owns the settings; the shell registers the shortcut and opens the window.

use crate::module::{bool_setting, ModuleDescriptor};
use crate::registry::Registry;
use fc_core::settings::{SettingDef, SettingKind, SettingScopeKind};
use serde_json::json;

/// Module id.
pub const ID: &str = "quick_add";

/// Setting: the global shortcut (Tauri accelerator syntax).
pub const SHORTCUT: &str = "quick_add.shortcut";
/// Setting: prefill the front with the clipboard text when the window opens (read only when
/// the user presses the shortcut).
pub const PREFILL_CLIPBOARD: &str = "quick_add.prefill_clipboard";
/// Default shortcut.
pub const DEFAULT_SHORTCUT: &str = "CommandOrControl+Alt+N";

/// Descriptor.
pub fn descriptor() -> ModuleDescriptor {
    ModuleDescriptor {
        id: ID,
        name_key: "modules.quick_add.name",
        description_key: "modules.quick_add.description",
        enabled_by_default: true,
        can_disable: true,
        settings: vec![
            SettingDef {
                key: SHORTCUT.to_owned(),
                kind: SettingKind::Text { max_len: Some(60) },
                default: json!(DEFAULT_SHORTCUT),
                label_key: format!("settings.{SHORTCUT}.label"),
                description_key: Some(format!("settings.{SHORTCUT}.description")),
                advanced: false,
                scopes: vec![SettingScopeKind::App],
            },
            bool_setting(PREFILL_CLIPBOARD, true, &[SettingScopeKind::App]),
        ],
        ui: vec![],
    }
}

/// Registers the module.
pub fn register(reg: &mut Registry) {
    reg.module(descriptor());
}
