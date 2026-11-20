//! `focus` module: focus mode (docs/02 §6.2) — the study screen full screen with everything
//! that distracts hidden. Pure presentation: the module only owns its switch and settings;
//! the frontend reads them from `modules.list` / `settings.*`.

use crate::module::{bool_setting, ModuleDescriptor};
use crate::registry::Registry;
use fc_core::settings::SettingScopeKind;

/// Module id.
pub const ID: &str = "focus";
/// Enter the operating system's full screen too (not only hide the app's chrome).
pub const FULLSCREEN: &str = "focus.fullscreen";
/// Hide the remaining-card counters and progress.
pub const HIDE_COUNTS: &str = "focus.hide_counts";
/// Start every study session in focus mode.
pub const AUTO: &str = "focus.auto";

/// Descriptor.
pub fn descriptor() -> ModuleDescriptor {
    let app = [SettingScopeKind::App];
    ModuleDescriptor {
        id: ID,
        name_key: "modules.focus.name",
        description_key: "modules.focus.description",
        enabled_by_default: true,
        can_disable: true,
        settings: vec![
            bool_setting(FULLSCREEN, true, &app),
            bool_setting(HIDE_COUNTS, true, &app),
            bool_setting(AUTO, false, &app),
        ],
        ui: vec![],
    }
}

/// Registers the module.
pub fn register(reg: &mut Registry) {
    reg.module(descriptor());
}
