//! The v1 method catalog. **Single source of truth** for method names, kinds, owning module,
//! permission and request/response types. `fc-app` registers one handler per entry; a test
//! there asserts the registry covers the whole catalog.
//!
//! One file per domain (`core.rs`, `decks.rs`, …), each a `methods! { … }` block. The
//! [`domains!`] list at the bottom is the only shared line: it declares the files, re-exports
//! their method types and concatenates their descriptors into [`catalog`] (in list order).
//!
//! Adding methods: a new domain adds `methods/<domain>.rs` + one name in `domains!`; then run
//! `cargo run -p fc-api --bin gen-ts` and implement the handlers in the owning `fc-app` module.

use crate::method::MethodDescriptor;

/// Declares methods: a zero-sized type implementing [`crate::ApiMethod`] per line, plus the
/// domain's `descriptors()` in declaration order.
///
/// ```text
/// /// Doc comment (required).
/// DecksCreate = "decks.create", Command, "decks", "decks:write", Public, undo: true,
///     CreateDeck => DeckDto;
/// ```
macro_rules! methods {
    ($(
        $(#[$m:meta])*
        $ty:ident = $name:literal, $kind:ident, $module:literal, $perm:literal, $vis:ident,
            undo: $undo:literal,
            $req:ty => $res:ty;
    )*) => {
        $(
            $(#[$m])*
            #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
            pub struct $ty;

            impl $crate::method::ApiMethod for $ty {
                const NAME: &'static str = $name;
                const KIND: $crate::method::MethodKind = $crate::method::MethodKind::$kind;
                const MODULE: &'static str = $module;
                const PERMISSION: &'static str = $perm;
                const VISIBILITY: $crate::method::Visibility = $crate::method::Visibility::$vis;
                const UNDOABLE: bool = $undo;
                type Request = $req;
                type Response = $res;
            }
        )*

        /// Descriptors of this domain's methods, in declaration order.
        pub(super) fn descriptors() -> Vec<$crate::method::MethodDescriptor> {
            vec![ $( $crate::method::MethodDescriptor::of::<$ty>(), )* ]
        }
    };
}

/// Declares the domain files and builds [`catalog`] from them, in this order.
macro_rules! domains {
    ($($domain:ident),* $(,)?) => {
        $(
            mod $domain;
            pub use self::$domain::*;
        )*

        /// Descriptors of every v1 method, in catalog order (domain list order, then
        /// declaration order inside each file).
        pub fn catalog() -> Vec<MethodDescriptor> {
            let mut all = Vec::new();
            $( all.extend(self::$domain::descriptors()); )*
            all
        }
    };
}

// One name per domain file. Phase-2 tracks append their domain here (and nothing else).
domains! {
    core,
    decks,
    notetypes,
    notes,
    tags,
    attributes,
    study,
    cardtypes,
    io,
    search,
    bulk,
    presets,
    browse,
    stats,
    planning,
}
