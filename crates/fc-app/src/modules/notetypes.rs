//! `notetypes` module: note type schemas (read-only in v0.1; editor in v0.3).

use crate::module::ModuleDescriptor;
use crate::registry::Registry;
use fc_api::v1::methods::*;

/// Module id.
pub const ID: &str = "notetypes";

/// Descriptor.
pub fn descriptor() -> ModuleDescriptor {
    ModuleDescriptor {
        id: ID,
        name_key: "modules.notetypes.name",
        description_key: "modules.notetypes.description",
        enabled_by_default: true,
        can_disable: false,
        settings: vec![],
        ui: vec![],
    }
}

/// Registers the module and its methods.
pub fn register(reg: &mut Registry) {
    reg.module(descriptor());
    reg.stub::<NotetypesList>();
    reg.stub::<NotetypesGet>();
}
