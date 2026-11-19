//! `attributes` module: user-defined classifications (docs/02 §8.1). Building blocks live in
//! `fc_core::attribute` (validation, option editing, inheritance, templates, storage form).

use crate::module::{bool_setting, settings_page, ModuleDescriptor};
use crate::registry::Registry;
use fc_api::v1::methods::*;
use fc_core::settings::SettingScopeKind;

/// Module id.
pub const ID: &str = "attributes";

/// Descriptor.
pub fn descriptor() -> ModuleDescriptor {
    ModuleDescriptor {
        id: ID,
        name_key: "modules.attributes.name",
        description_key: "modules.attributes.description",
        enabled_by_default: true,
        can_disable: true,
        settings: vec![bool_setting(
            "attributes.remember_last",
            false,
            &[SettingScopeKind::App, SettingScopeKind::Deck],
        )],
        ui: vec![settings_page(
            "attributes.manage",
            "settings.pages.attributes",
            "/settings/attributes",
            20,
        )],
    }
}

/// Registers the module and its methods.
pub fn register(reg: &mut Registry) {
    reg.module(descriptor());
    reg.stub::<AttributesList>();
    reg.stub::<AttributesEffective>();
    reg.stub::<AttributesTemplates>();
    reg.stub::<AttributesGet>();
    reg.stub::<AttributesDefine>();
    reg.stub::<AttributesDefineFromTemplate>();
    reg.stub::<AttributesUpdate>();
    reg.stub::<AttributesDelete>();
    reg.stub::<AttributesRestore>();
    reg.stub::<AttributesOptionAdd>();
    reg.stub::<AttributesOptionRename>();
    reg.stub::<AttributesOptionSetColor>();
    reg.stub::<AttributesOptionReorder>();
    reg.stub::<AttributesOptionMerge>();
    reg.stub::<AttributesOptionDelete>();
    reg.stub::<AttributesValuesGet>();
    reg.stub::<AttributesValuesSet>();
}
