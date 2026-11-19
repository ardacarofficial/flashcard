//! Feature modules. **One file per module**, each exposing `descriptor()` and `register()`.
//! A lane adding a module adds its file and one line to [`register_all`].
//!
//! v0.1 registers every catalog method; handlers not implemented yet are `Registry::stub`s
//! (fail with `INTERNAL_NOT_IMPLEMENTED`). L3 replaces stubs with real handlers.

pub mod attributes;
pub mod core;
pub mod decks;
pub mod notes;
pub mod notetypes;
pub mod tags;

use crate::registry::Registry;

/// Registers every module, in menu order.
pub fn register_all(reg: &mut Registry) {
    core::register(reg);
    decks::register(reg);
    notetypes::register(reg);
    notes::register(reg);
    tags::register(reg);
    attributes::register(reg);
}
