//! v0.3 card types & content: note type editor, rendering/preview, media, links, flags, card
//! notes. Modules: `cardtypes`, `media`, `links`, `flags`, `card_notes` (+ frontend-only
//! `math`, `code_highlight`).

use crate::v1::cardtypes::*;
use crate::v1::common::*;
use crate::v1::notetypes::{NoteTypeDto, NoteTypeRef};

methods! {
    // ---- note type editor ----
    /// Creates a note type (definition, copy, or minimal front/back).
    NotetypesCreate = "notetypes.create", Command, "cardtypes", "notetypes:write", Public, undo: true, CreateNoteType => NoteTypeDto;
    /// Replaces a note type's schema (fields, templates, CSS).
    NotetypesUpdate = "notetypes.update", Command, "cardtypes", "notetypes:write", Public, undo: true, UpdateNoteType => NoteTypeDto;
    /// Moves an unused note type to the trash.
    NotetypesDelete = "notetypes.delete", Command, "cardtypes", "notetypes:write", Public, undo: true, NoteTypeRef => NoteTypeDto;
    /// Restores a note type from the trash.
    NotetypesRestore = "notetypes.restore", Command, "cardtypes", "notetypes:write", Public, undo: true, NoteTypeRef => NoteTypeDto;
    /// Inverse of `notetypes.update`: restores a note type and its notes' field values.
    NotetypesRestoreVersion = "notetypes.restore_version", Command, "cardtypes", "notetypes:write", Internal, undo: true, RestoreNoteTypeVersion => NoteTypeDto;
    /// How many notes/cards use a note type.
    NotetypesUsage = "notetypes.usage", Query, "cardtypes", "notetypes:read", Public, undo: false, NoteTypeRef => NoteTypeUsage;
    /// Registered card type engines.
    CardtypesList = "cardtypes.list", Query, "cardtypes", "notetypes:read", Public, undo: false, Empty => CardTypeList;
    /// Inverse of card generation: trashes generated cards.
    CardsTrashGenerated = "cards.trash_generated", Command, "cardtypes", "cards:write", Internal, undo: true, CardIdList => AffectedCount;
    /// Inverse of `cards.trash_generated`.
    CardsRestoreGenerated = "cards.restore_generated", Command, "cardtypes", "cards:write", Internal, undo: true, CardIdList => AffectedCount;

    // ---- rendering ----
    /// Renders a card (sanitized HTML).
    CardsRender = "cards.render", Query, "cardtypes", "cards:read", Public, undo: false, CardRef => RenderedCardDto;
    /// Renders the cards a note would have, without saving.
    CardsPreview = "cards.preview", Query, "cardtypes", "cards:read", Public, undo: false, PreviewCards => CardPreviewList;

    // ---- media ----
    /// Stores a media file (deduplicated by content).
    MediaAdd = "media.add", Command, "media", "media:write", Public, undo: true, AddMedia => MediaDto;
    /// Inverse of `media.add`: drops an unreferenced media entry.
    MediaForget = "media.forget", Command, "media", "media:write", Internal, undo: true, MediaRef => AffectedCount;
    /// Inverse of `media.forget`.
    MediaReinstate = "media.reinstate", Command, "media", "media:write", Internal, undo: true, MediaEntry => AffectedCount;
    /// One media entry.
    MediaGet = "media.get", Query, "media", "media:read", Public, undo: false, MediaRef => MediaDto;
    /// All media entries.
    MediaList = "media.list", Query, "media", "media:read", Public, undo: false, Empty => MediaItems;
    /// File content (transports without a media protocol).
    MediaRead = "media.read", Query, "media", "media:read", Public, undo: false, MediaRef => MediaContent;

    // ---- links ----
    /// Notes linking to a note.
    LinksBacklinks = "links.backlinks", Query, "links", "notes:read", Public, undo: false, BacklinksOf => LinkedNoteList;
    /// Link completion.
    LinksSuggest = "links.suggest", Query, "links", "notes:read", Public, undo: false, SuggestLinks => LinkedNoteList;

    // ---- flags ----
    /// The seven card flags.
    FlagsList = "flags.list", Query, "flags", "cards:read", Public, undo: false, Empty => FlagList;
    /// Renames/recolors a flag.
    FlagsUpdate = "flags.update", Command, "flags", "cards:write", Public, undo: true, UpdateFlag => FlagDto;
    /// Turns a flag on/off on cards.
    CardsSetFlag = "cards.set_flag", Command, "flags", "cards:write", Public, undo: true, SetCardFlag => CardFlagsList;
    /// Sets exact flag bits.
    CardsSetFlags = "cards.set_flags", Command, "flags", "cards:write", Public, undo: true, SetCardFlags => CardFlagsList;

    // ---- card notes ----
    /// Sets the private note on a card.
    CardsSetUserNote = "cards.set_user_note", Command, "card_notes", "cards:write", Public, undo: true, SetCardUserNote => CardMetaDto;
    /// Per-card details of a note (flags, card note).
    CardsMeta = "cards.meta", Query, "cardtypes", "cards:read", Public, undo: false, CardsOfNote => CardMetaList;
}
