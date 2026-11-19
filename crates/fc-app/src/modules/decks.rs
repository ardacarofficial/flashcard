//! `decks` module: hierarchical decks (docs/02 §1).

use crate::module::{bool_setting, main_menu, ModuleDescriptor};
use crate::registry::Registry;
use fc_api::v1::methods::*;
use fc_core::settings::SettingScopeKind;

/// Module id.
pub const ID: &str = "decks";

/// Descriptor.
pub fn descriptor() -> ModuleDescriptor {
    ModuleDescriptor {
        id: ID,
        name_key: "modules.decks.name",
        description_key: "modules.decks.description",
        enabled_by_default: true,
        can_disable: false,
        settings: vec![bool_setting(
            "decks.include_subdecks",
            true,
            &[
                SettingScopeKind::App,
                SettingScopeKind::Preset,
                SettingScopeKind::Deck,
            ],
        )],
        ui: vec![main_menu("decks.tree", "nav.decks", "decks", "/decks", 10)],
    }
}

/// Registers the module and its methods.
pub fn register(reg: &mut Registry) {
    reg.module(descriptor());
    reg.stub::<DecksTree>();
    reg.stub::<DecksList>();
    reg.stub::<DecksGet>();
    reg.stub::<DecksCreate>();
    reg.stub::<DecksRename>();
    reg.stub::<DecksMove>();
    reg.stub::<DecksUpdate>();
    reg.stub::<DecksDelete>();
    reg.stub::<DecksRestore>();
}
