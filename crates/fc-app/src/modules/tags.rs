//! `tags` module: hierarchical tags (docs/02 §8).

use crate::module::ModuleDescriptor;
use crate::registry::Registry;
use fc_api::v1::methods::*;

/// Module id.
pub const ID: &str = "tags";

/// Descriptor.
pub fn descriptor() -> ModuleDescriptor {
    ModuleDescriptor {
        id: ID,
        name_key: "modules.tags.name",
        description_key: "modules.tags.description",
        enabled_by_default: true,
        can_disable: true,
        settings: vec![],
        ui: vec![],
    }
}

/// Registers the module and its methods.
pub fn register(reg: &mut Registry) {
    reg.module(descriptor());
    reg.stub::<TagsList>();
    reg.stub::<TagsRename>();
    reg.stub::<TagsMerge>();
    reg.stub::<TagsSetColor>();
}
