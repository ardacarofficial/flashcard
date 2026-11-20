//! Feature modules. **One file per module**, each exposing `descriptor()` and `register()`.
//! A track adding a module adds its file and one name to the `modules!` list.
//!
//! Every catalog method must be registered (`registry_covers_the_catalog_exactly`); a method
//! declared ahead of its handler can use `Registry::stub` (`INTERNAL_NOT_IMPLEMENTED`).

use crate::registry::Registry;

/// Declares the module files and [`register_all`], which registers them in list order
/// (= menu order).
macro_rules! modules {
    ($($module:ident),* $(,)?) => {
        $( pub mod $module; )*

        /// Registers every module, in menu order.
        pub fn register_all(reg: &mut Registry) {
            $( $module::register(reg); )*
        }
    };
}

// One name per module file, in menu order. Phase-2 tracks append theirs here.
modules! {
    core,
    decks,
    notetypes,
    notes,
    tags,
    attributes,
    study,
    cardtypes,
    media,
    links,
    flags,
    card_notes,
    rich_content,
    io,
    browse,
    palette,
    presets,
    mcp,
}
