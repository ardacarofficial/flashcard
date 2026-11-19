//! `notes` module: note CRUD and listing (docs/02 §2).

use crate::module::{main_menu, ModuleDescriptor};
use crate::registry::Registry;
use fc_api::v1::methods::*;

/// Module id.
pub const ID: &str = "notes";

/// Descriptor.
pub fn descriptor() -> ModuleDescriptor {
    ModuleDescriptor {
        id: ID,
        name_key: "modules.notes.name",
        description_key: "modules.notes.description",
        enabled_by_default: true,
        can_disable: false,
        settings: vec![],
        ui: vec![
            main_menu("notes.add", "nav.add_note", "plus", "/notes/new", 20),
            main_menu("notes.list", "nav.notes", "list", "/notes", 30),
        ],
    }
}

/// Registers the module and its methods.
pub fn register(reg: &mut Registry) {
    reg.module(descriptor());
    reg.stub::<NotesCreate>();
    reg.stub::<NotesUpdate>();
    reg.stub::<NotesDelete>();
    reg.stub::<NotesRestore>();
    reg.stub::<NotesGet>();
    reg.stub::<NotesList>();
}
