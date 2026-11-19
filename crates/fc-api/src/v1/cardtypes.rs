//! v0.3 DTOs: note type editor, card rendering/preview, media, links, flags, card notes.

use fc_core::content::media::MediaKind;
use fc_core::notetype::{CardTemplate, FieldDef, FieldKey, NoteTypeKind};
use fc_core::{CardId, DeckId, NoteId, NoteTypeId, Timestamp};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

// ------------------------------------------------------------------ note type editor

/// A note type definition as edited. Fields and templates with an empty `key` are new: the
/// backend derives a key from their name. Existing keys are matched to existing fields, so
/// renaming a field (its `name`) never touches notes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct NoteTypeInput {
    /// Display name.
    pub name: String,
    /// Standard or cloze.
    pub kind: NoteTypeKind,
    /// Fields in display order.
    pub fields: Vec<FieldDef>,
    /// Card templates in order.
    pub templates: Vec<CardTemplate>,
    /// Shared CSS (sanitized on save).
    pub css: String,
    /// Sort field key (empty = the first field).
    pub sort_field: FieldKey,
}

/// `notetypes.create`: a new note type from a definition, a copy of an existing one, or (with
/// neither) a minimal front/back type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct CreateNoteType {
    /// Name (overrides the definition's/copied name when set).
    pub name: Option<String>,
    /// Copy this note type.
    pub copy_from: Option<NoteTypeId>,
    /// Full definition.
    pub definition: Option<NoteTypeInput>,
    /// Id to use (redo only; normally generated).
    pub id: Option<NoteTypeId>,
}

/// `notetypes.update`: replaces the schema. Removed fields drop their values from every note
/// of the type (undo brings them back); new fields start empty (or at their default); a
/// template that still has cards cannot be removed (`CONFLICT_TEMPLATE_IN_USE`); new
/// templates generate their cards for existing notes. Card history is never touched.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct UpdateNoteType {
    /// Note type.
    pub id: NoteTypeId,
    /// New definition.
    pub definition: NoteTypeInput,
}

/// Field values of one note, as restored by undo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct NoteFieldsSnapshot {
    /// Note.
    pub id: NoteId,
    /// Field values.
    pub fields: BTreeMap<String, String>,
}

/// Every column of a note type (undo of schema changes restores it exactly).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct NoteTypeSnapshot {
    /// Id.
    pub id: NoteTypeId,
    /// Name.
    pub name: String,
    /// Kind.
    pub kind: NoteTypeKind,
    /// Fields.
    pub fields: Vec<FieldDef>,
    /// Templates.
    pub templates: Vec<CardTemplate>,
    /// CSS.
    pub css: String,
    /// Sort field.
    pub sort_field: FieldKey,
    /// Schema version.
    pub version: u32,
    /// Built-in marker.
    pub builtin: Option<String>,
    /// Creation time.
    pub created_at: Timestamp,
    /// Last change.
    pub updated_at: Timestamp,
    /// Trash time.
    pub deleted_at: Option<Timestamp>,
}

/// Internal `notetypes.restore_version`: puts a note type and the field values of its notes
/// back exactly as they were (inverse of `notetypes.update`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct RestoreNoteTypeVersion {
    /// Note type columns.
    pub notetype: NoteTypeSnapshot,
    /// Notes whose field values to restore.
    pub notes: Vec<NoteFieldsSnapshot>,
}

/// Response of `notetypes.usage`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct NoteTypeUsage {
    /// Live notes.
    pub notes: u32,
    /// Live and trashed cards per template key (`c1`… / `m1`… for cloze / occlusion).
    pub cards_by_template: BTreeMap<String, u32>,
}

/// A card type engine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CardTypeDto {
    /// Id (`CardTemplate::card_type`).
    pub id: String,
    /// i18n key of the name.
    pub name_key: String,
}

/// Response of `cardtypes.list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CardTypeList {
    /// Engines, built-ins first.
    pub card_types: Vec<CardTypeDto>,
}

/// Card ids (internal `cards.trash_generated` / `cards.restore_generated`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct CardIdList {
    /// Cards.
    pub ids: Vec<CardId>,
}

// ------------------------------------------------------------------ rendering

/// How a card is answered.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CardInteraction {
    /// Reveal and grade yourself.
    SelfGrade,
    /// Type the answer.
    Type {
        /// Expected answer (plain text).
        expected: String,
    },
    /// Pick one option.
    Choice {
        /// Sanitized option HTML, shuffled.
        options: Vec<String>,
        /// Index of the right option.
        correct: u32,
    },
}

/// Text to read aloud.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct TtsItem {
    /// BCP-47 language (empty = unspecified).
    pub lang: String,
    /// Text.
    pub text: String,
}

/// A rendered side pair. HTML is sanitized; media appear as `src="media:<sha256>"`, note
/// links as `href="note:<id>"`, math as `span.math`, code as `code.language-*` (the frontend
/// maps media URLs and typesets math/code; it never needs to sanitize).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct RenderedSides {
    /// Template key of the card (`forward`, `c2`, `m1`…).
    pub template_key: String,
    /// Template display name.
    pub template_name: String,
    /// Card type engine id.
    pub card_type: String,
    /// Question HTML.
    pub question: String,
    /// Answer HTML.
    pub answer: String,
    /// Note type CSS (sanitized; scope it to the card container).
    pub css: String,
    /// Answer mode.
    pub interaction: CardInteraction,
    /// TTS requests.
    pub tts: Vec<TtsItem>,
}

/// `cards.render`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct CardRef {
    /// Card id.
    pub id: CardId,
}

/// Response of `cards.render`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct RenderedCardDto {
    /// Card.
    pub card_id: CardId,
    /// Note.
    pub note_id: NoteId,
    /// Rendered content.
    pub sides: RenderedSides,
    /// Flag bits.
    pub flags: u32,
    /// Card note.
    pub user_note: Option<String>,
}

/// `cards.preview`: renders the cards a note **would** have, without saving. Either an
/// existing note type (`notetype_id`) or an unsaved definition (note type editor).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct PreviewCards {
    /// Existing note type.
    pub notetype_id: Option<NoteTypeId>,
    /// Unsaved definition (wins over `notetype_id`).
    pub definition: Option<NoteTypeInput>,
    /// Field values (missing = empty).
    #[serde(default)]
    pub fields: BTreeMap<String, String>,
    /// Tags.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Deck (for `{{Deck}}` and automatic distractors).
    pub deck_id: Option<DeckId>,
}

/// Response of `cards.preview`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct CardPreviewList {
    /// One entry per card that would be generated (empty when the note yields none).
    pub cards: Vec<RenderedSides>,
}

// ------------------------------------------------------------------ media

/// `media.add`: stores a file (content-addressed; adding the same bytes again returns the
/// existing entry with `created: false`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct AddMedia {
    /// Original file name (for type detection and display).
    pub name: Option<String>,
    /// File content, standard base64.
    pub data_base64: String,
}

/// A stored media file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct MediaDto {
    /// SHA-256 hex.
    pub hash: String,
    /// Reference to put into fields: `media:<hash>`.
    pub reference: String,
    /// MIME type.
    pub mime: String,
    /// Category.
    pub kind: MediaKind,
    /// Size in bytes.
    pub size: u64,
    /// Name it was first added with.
    pub original_name: Option<String>,
    /// Notes referencing it.
    pub ref_count: u32,
    /// Whether this call stored it (false: it already existed).
    pub created: bool,
}

/// `media.get` / internal `media.forget`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct MediaRef {
    /// SHA-256 hex.
    pub hash: String,
}

/// Response of `media.list`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct MediaItems {
    /// Newest first.
    pub items: Vec<MediaDto>,
}

/// Internal `media.reinstate` (inverse of `media.forget`): re-registers a media entry whose
/// file is still stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct MediaEntry {
    /// SHA-256 hex.
    pub hash: String,
    /// MIME type.
    pub mime: String,
    /// Size in bytes.
    pub size: u64,
    /// Original name.
    pub original_name: Option<String>,
}

/// Response of `media.read`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct MediaContent {
    /// MIME type.
    pub mime: String,
    /// Content, standard base64.
    pub data_base64: String,
}

// ------------------------------------------------------------------ links

/// `links.backlinks`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct BacklinksOf {
    /// Target note.
    pub note_id: NoteId,
}

/// `links.suggest`: notes whose title starts with `prefix` (for `[[` completion).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct SuggestLinks {
    /// Typed text after `[[`.
    pub prefix: String,
    /// Maximum results (default 10, at most 50).
    pub limit: Option<u32>,
}

/// A linked/linkable note.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct LinkedNote {
    /// Note.
    pub id: NoteId,
    /// Its title (sort field text).
    pub title: String,
}

/// Response of `links.backlinks` / `links.suggest`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct LinkedNoteList {
    /// Notes.
    pub items: Vec<LinkedNote>,
}

// ------------------------------------------------------------------ flags & card notes

/// A card flag.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct FlagDto {
    /// 1–7 (bit `flag - 1` of the card's flags).
    pub flag: u8,
    /// Custom name (`null` = localized default `flags.default.<flag>`).
    pub name: Option<String>,
    /// Color token.
    pub color: String,
}

/// Response of `flags.list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct FlagList {
    /// The seven flags.
    pub flags: Vec<FlagDto>,
}

/// `flags.update`: renames/recolors a flag (`name: null` = back to the default name).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct UpdateFlag {
    /// 1–7.
    pub flag: u8,
    /// New name.
    pub name: Option<String>,
    /// New color (unchanged when `null`).
    pub color: Option<String>,
}

/// `cards.set_flag`: turns one flag on/off on cards.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SetCardFlag {
    /// Cards.
    pub ids: Vec<CardId>,
    /// 1–7.
    pub flag: u8,
    /// On or off.
    pub on: bool,
}

/// Flag bits of one card.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct CardFlags {
    /// Card.
    pub id: CardId,
    /// Bitmask.
    pub flags: u32,
}

/// `cards.set_flags`: sets exact flag bits (also the inverse of `cards.set_flag`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SetCardFlags {
    /// Cards and their new bits.
    pub items: Vec<CardFlags>,
}

/// Response of the flag commands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CardFlagsList {
    /// Cards and their bits after the change.
    pub items: Vec<CardFlags>,
}

/// `cards.set_user_note`: the private note on a card (`null`/blank clears it).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct SetCardUserNote {
    /// Card.
    pub id: CardId,
    /// Text.
    pub text: Option<String>,
}

/// `cards.meta`: per-card details of a note.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct CardsOfNote {
    /// Note.
    pub note_id: NoteId,
}

/// Per-card details.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CardMetaDto {
    /// Card.
    pub id: CardId,
    /// Template key.
    pub template_key: String,
    /// Template display name (or the key for cloze/occlusion cards).
    pub template_name: String,
    /// Engine id.
    pub card_type: String,
    /// Deck.
    pub deck_id: DeckId,
    /// Flag bits.
    pub flags: u32,
    /// Card note.
    pub user_note: Option<String>,
}

/// Response of `cards.meta`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CardMetaList {
    /// Live cards of the note.
    pub items: Vec<CardMetaDto>,
}
