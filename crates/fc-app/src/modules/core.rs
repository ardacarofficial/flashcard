//! `core` module: undo/redo, batches, module registry, settings, jobs. Cannot be disabled.

use crate::module::{choice_setting, settings_page, ModuleDescriptor};
use crate::registry::Registry;
use fc_api::v1::methods::*;

/// Module id.
pub const ID: &str = "core";

/// Descriptor.
pub fn descriptor() -> ModuleDescriptor {
    ModuleDescriptor {
        id: ID,
        name_key: "modules.core.name",
        description_key: "modules.core.description",
        enabled_by_default: true,
        can_disable: false,
        settings: vec![
            choice_setting("app.locale", &["tr", "en"], "tr"),
            choice_setting("app.theme", &["system", "light", "dark"], "system"),
        ],
        ui: vec![settings_page(
            "core.general",
            "settings.pages.general",
            "/settings/general",
            0,
        )],
    }
}

/// Registers the module and its methods.
pub fn register(reg: &mut Registry) {
    reg.module(descriptor());
    reg.stub::<OplogUndo>();
    reg.stub::<OplogRedo>();
    reg.stub::<OplogStatus>();
    reg.stub::<OplogList>();
    reg.stub::<BatchExecute>();
    reg.stub::<ModulesList>();
    reg.stub::<ModulesSetEnabled>();
    reg.stub::<CommandsList>();
    reg.stub::<SettingsSchemaGet>();
    reg.stub::<SettingsGet>();
    reg.stub::<SettingsSet>();
    reg.stub::<SettingsReset>();
    reg.stub::<JobsList>();
    reg.stub::<JobsGet>();
    reg.stub::<JobsCancel>();
}
