//! The repository interface: what `fc-app` may ask of storage, per area.
//!
//! * `*Read` traits are available in read and write transactions; `*Write` traits only in write
//!   transactions. [`Reads`] / [`Writes`] bundle them (blanket-implemented).
//! * All traits are object-safe; the app works with `&dyn Writes` / `Box<dyn WriteTx>`.
//! * Methods do **no business logic**: validation, inverse computation, cascades and events
//!   are the app's job. Storage only persists/loads rows faithfully.
//! * Soft delete: `set_*_deleted(ids, Some(t))` trashes, `(ids, None)` restores. Readers return
//!   deleted rows only when asked (`include_deleted`) or when fetching by id.
//!
//! One file per domain (records + `<Domain>Read`/`<Domain>Write`). A new domain adds
//! `repo/<domain>.rs` and **one line** to the [`domains!`] list below, which declares the
//! file, re-exports it and adds its traits to [`Reads`]/[`Writes`]. (It then implements the
//! traits for `SqliteTx` in `sqlite/<domain>.rs` and for `fc_app::testing::MemTx`.)

// Shared imports of the domain files (they `use super::*`).
pub(crate) use crate::error::StorageResult;
pub(crate) use fc_core::attribute::{AttributeDef, AttributeOption, AttributeValue, EntityRef};
pub(crate) use fc_core::card::Card;
pub(crate) use fc_core::deck::Deck;
pub(crate) use fc_core::note::Note;
pub(crate) use fc_core::notetype::NoteType;
pub(crate) use fc_core::paging::{Page, PageRequest};
pub(crate) use fc_core::preset::Preset;
pub(crate) use fc_core::settings::SettingScope;
pub(crate) use fc_core::tag::{Tag, TagName};
pub(crate) use fc_core::{
    AttributeDefId, AttributeOptionId, BatchId, CardId, DeckId, JobId, NoteId, NoteTypeId, OplogId,
    PresetId, TagId, Timestamp,
};
pub(crate) use serde_json::Value;

/// Declares the domain files and the [`Reads`]/[`Writes`] bundles over their traits.
macro_rules! domains {
    (
        $first:ident: $first_read:ident, $first_write:ident;
        $($domain:ident: $read:ident, $write:ident;)*
    ) => {
        mod $first;
        pub use self::$first::*;
        $(
            mod $domain;
            pub use self::$domain::*;
        )*

        /// Everything readable. Blanket-implemented.
        pub trait Reads: $first_read $(+ $read)* {}

        impl<T> Reads for T where T: $first_read $(+ $read)* {}

        /// Everything writable. Blanket-implemented.
        pub trait Writes: Reads + $first_write $(+ $write)* {}

        impl<T> Writes for T where T: Reads + $first_write $(+ $write)* {}
    };
}

// One line per domain. Phase-2 tracks append theirs here (and nothing else in this crate's
// shared files, apart from `mod <domain>;` in `sqlite/mod.rs`).
domains! {
    decks: DeckRead, DeckWrite;
    presets: PresetRead, PresetWrite;
    notetypes: NoteTypeRead, NoteTypeWrite;
    notes: NoteRead, NoteWrite;
    cards: CardRead, CardWrite;
    tags: TagRead, TagWrite;
    attributes: AttributeRead, AttributeWrite;
    oplog: OplogRead, OplogWrite;
    settings: SettingsRead, SettingsWrite;
    modules: ModuleStateRead, ModuleStateWrite;
    jobs: JobRead, JobWrite;
    study: StudyRead, StudyWrite;
    content: ContentRead, ContentWrite;
    interop: InteropRead, InteropWrite;
    search: SearchRead, SearchWrite;
    mcp: McpRead, McpWrite;
}
