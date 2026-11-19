//! The v1 method catalog. **Single source of truth** for method names, kinds, owning module,
//! permission and request/response types. `fc-app` registers one handler per entry; a test
//! there asserts the registry covers the whole catalog.
//!
//! Adding a method: add a line here, run `cargo run -p fc-api --bin gen-ts`, implement the
//! handler in the owning module of `fc-app`.

use super::attributes::*;
use super::common::*;
use super::decks::*;
use super::jobs::*;
use super::modules::*;
use super::notes::*;
use super::notetypes::*;
use super::oplog::*;
use super::settings::*;
use super::tags::*;
use crate::method::{ApiMethod, MethodDescriptor, MethodKind, Visibility};

macro_rules! catalog {
    ($(
        $(#[$m:meta])*
        $ty:ident = $name:literal, $kind:ident, $module:literal, $perm:literal, undo: $undo:literal,
            $req:ty => $res:ty;
    )*) => {
        $(
            $(#[$m])*
            #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
            pub struct $ty;

            impl ApiMethod for $ty {
                const NAME: &'static str = $name;
                const KIND: MethodKind = MethodKind::$kind;
                const MODULE: &'static str = $module;
                const PERMISSION: &'static str = $perm;
                const VISIBILITY: Visibility = Visibility::Public;
                const UNDOABLE: bool = $undo;
                type Request = $req;
                type Response = $res;
            }
        )*

        /// Descriptors of every v1 method, in catalog order.
        pub fn catalog() -> Vec<MethodDescriptor> {
            vec![ $( MethodDescriptor::of::<$ty>(), )* ]
        }
    };
}

catalog! {
    // ---- core: undo/redo, batch ----
    /// Reverts the latest undoable batch.
    OplogUndo = "oplog.undo", Command, "core", "oplog:write", undo: false, Empty => UndoResult;
    /// Re-applies the latest undone batch.
    OplogRedo = "oplog.redo", Command, "core", "oplog:write", undo: false, Empty => UndoResult;
    /// What undo/redo would do now.
    OplogStatus = "oplog.status", Query, "core", "oplog:read", undo: false, Empty => UndoStatus;
    /// Operation history (newest first).
    OplogList = "oplog.list", Query, "core", "oplog:read", undo: false, ListOplog => OplogPage;
    /// Runs several commands atomically as one undo step (its sub-calls are what the oplog
    /// records, so the batch itself is `undo: false`).
    BatchExecute = "batch.execute", Command, "core", "batch:write", undo: false, ExecuteBatch => BatchResult;

    // ---- core: modules, method registry, settings, jobs ----
    /// Modules with their UI contributions (frontend menus are drawn from this).
    ModulesList = "modules.list", Query, "core", "modules:read", undo: false, Empty => ModuleList;
    /// Enables/disables a module.
    ModulesSetEnabled = "modules.set_enabled", Command, "core", "modules:write", undo: true, SetModuleEnabled => ModuleDto;
    /// All methods (command palette, MCP tool list, docs).
    CommandsList = "commands.list", Query, "core", "modules:read", undo: false, Empty => MethodList;
    /// Setting schemas of enabled modules.
    SettingsSchemaGet = "settings.schema", Query, "core", "settings:read", undo: false, Empty => SettingsSchema;
    /// Resolved setting values.
    SettingsGet = "settings.get", Query, "core", "settings:read", undo: false, GetSettings => SettingValues;
    /// Sets a setting at a scope.
    SettingsSet = "settings.set", Command, "core", "settings:write", undo: true, SetSetting => SettingValueDto;
    /// Removes a setting override at a scope.
    SettingsReset = "settings.reset", Command, "core", "settings:write", undo: true, ResetSetting => SettingValueDto;
    /// Background jobs.
    JobsList = "jobs.list", Query, "core", "jobs:read", undo: false, Empty => JobList;
    /// One job.
    JobsGet = "jobs.get", Query, "core", "jobs:read", undo: false, JobRef => JobDto;
    /// Requests cooperative cancellation.
    JobsCancel = "jobs.cancel", Command, "core", "jobs:write", undo: false, JobRef => JobDto;

    // ---- decks ----
    /// Deck tree with counts.
    DecksTree = "decks.tree", Query, "decks", "decks:read", undo: false, Empty => DeckTree;
    /// Flat deck list.
    DecksList = "decks.list", Query, "decks", "decks:read", undo: false, Empty => DeckList;
    /// One deck.
    DecksGet = "decks.get", Query, "decks", "decks:read", undo: false, DeckRef => DeckDto;
    /// Creates a deck (and missing parents) from a path.
    DecksCreate = "decks.create", Command, "decks", "decks:write", undo: true, CreateDeck => DeckDto;
    /// Renames a deck (leaf name).
    DecksRename = "decks.rename", Command, "decks", "decks:write", undo: true, RenameDeck => DeckDto;
    /// Re-parents/reorders a deck.
    DecksMove = "decks.move", Command, "decks", "decks:write", undo: true, MoveDeck => DeckDto;
    /// Updates description/icon/color.
    DecksUpdate = "decks.update", Command, "decks", "decks:write", undo: true, UpdateDeck => DeckDto;
    /// Moves a deck, its sub-decks, notes and cards to the trash.
    DecksDelete = "decks.delete", Command, "decks", "decks:write", undo: true, DeckRef => AffectedCount;
    /// Restores a deck deleted by `decks.delete` (with what was deleted along with it).
    DecksRestore = "decks.restore", Command, "decks", "decks:write", undo: true, DeckRef => AffectedCount;

    // ---- note types ----
    /// All note types.
    NotetypesList = "notetypes.list", Query, "notetypes", "notetypes:read", undo: false, Empty => NoteTypeList;
    /// One note type.
    NotetypesGet = "notetypes.get", Query, "notetypes", "notetypes:read", undo: false, NoteTypeRef => NoteTypeDto;

    // ---- notes ----
    /// Creates a note and its cards.
    NotesCreate = "notes.create", Command, "notes", "notes:write", undo: true, CreateNote => NoteDto;
    /// Replaces fields/tags/attributes of a note.
    NotesUpdate = "notes.update", Command, "notes", "notes:write", undo: true, UpdateNote => NoteDto;
    /// Moves notes (and their cards) to the trash.
    NotesDelete = "notes.delete", Command, "notes", "notes:write", undo: true, NoteIds => AffectedCount;
    /// Restores notes from the trash.
    NotesRestore = "notes.restore", Command, "notes", "notes:write", undo: true, NoteIds => AffectedCount;
    /// One note.
    NotesGet = "notes.get", Query, "notes", "notes:read", undo: false, NoteRef => NoteDto;
    /// Paged note list.
    NotesList = "notes.list", Query, "notes", "notes:read", undo: false, ListNotes => NotePage;

    // ---- tags ----
    /// All tags.
    TagsList = "tags.list", Query, "tags", "tags:read", undo: false, Empty => TagList;
    /// Renames a tag subtree on all notes.
    TagsRename = "tags.rename", Command, "tags", "tags:write", undo: true, RenameTag => TagChangeResult;
    /// Merges tags.
    TagsMerge = "tags.merge", Command, "tags", "tags:write", undo: true, MergeTags => TagChangeResult;
    /// Sets a tag color.
    TagsSetColor = "tags.set_color", Command, "tags", "tags:write", undo: true, SetTagColor => TagDto;

    // ---- attributes (classifications) ----
    /// Attribute definitions.
    AttributesList = "attributes.list", Query, "attributes", "attributes:read", undo: false, ListAttributes => AttributeDefList;
    /// Definitions applicable to a deck (inheritance resolved).
    AttributesEffective = "attributes.effective", Query, "attributes", "attributes:read", undo: false, EffectiveAttributes => EffectiveAttributeList;
    /// Built-in templates.
    AttributesTemplates = "attributes.templates", Query, "attributes", "attributes:read", undo: false, ListAttributeTemplates => AttributeTemplateList;
    /// One definition.
    AttributesGet = "attributes.get", Query, "attributes", "attributes:read", undo: false, AttributeRef => AttributeDefDto;
    /// Defines a classification.
    AttributesDefine = "attributes.define", Command, "attributes", "attributes:write", undo: true, DefineAttribute => AttributeDefDto;
    /// Defines a classification from a built-in template.
    AttributesDefineFromTemplate = "attributes.define_from_template", Command, "attributes", "attributes:write", undo: true, DefineAttributeFromTemplate => AttributeDefDto;
    /// Updates name/required/default/position.
    AttributesUpdate = "attributes.update", Command, "attributes", "attributes:write", undo: true, UpdateAttribute => AttributeDefDto;
    /// Moves a definition (and hides its values) to the trash.
    AttributesDelete = "attributes.delete", Command, "attributes", "attributes:write", undo: true, AttributeRef => AttributeDefDto;
    /// Restores a definition from the trash.
    AttributesRestore = "attributes.restore", Command, "attributes", "attributes:write", undo: true, AttributeRef => AttributeDefDto;
    /// Adds an option.
    AttributesOptionAdd = "attributes.option.add", Command, "attributes", "attributes:write", undo: true, AddAttributeOption => AttributeDefDto;
    /// Renames an option (id unchanged).
    AttributesOptionRename = "attributes.option.rename", Command, "attributes", "attributes:write", undo: true, RenameAttributeOption => AttributeDefDto;
    /// Sets an option color.
    AttributesOptionSetColor = "attributes.option.set_color", Command, "attributes", "attributes:write", undo: true, SetAttributeOptionColor => AttributeDefDto;
    /// Reorders options.
    AttributesOptionReorder = "attributes.option.reorder", Command, "attributes", "attributes:write", undo: true, ReorderAttributeOptions => AttributeDefDto;
    /// Merges one option into another (values move).
    AttributesOptionMerge = "attributes.option.merge", Command, "attributes", "attributes:write", undo: true, MergeAttributeOptions => OptionChangeResult;
    /// Deletes an option (values move to a replacement or are removed).
    AttributesOptionDelete = "attributes.option.delete", Command, "attributes", "attributes:write", undo: true, DeleteAttributeOption => OptionChangeResult;
    /// Values on a note/card.
    AttributesValuesGet = "attributes.values.get", Query, "attributes", "attributes:read", undo: false, GetAttributeValues => AttributeValues;
    /// Sets/clears values on a note/card.
    AttributesValuesSet = "attributes.values.set", Command, "attributes", "attributes:write", undo: true, SetAttributeValues => AttributeValues;
}
