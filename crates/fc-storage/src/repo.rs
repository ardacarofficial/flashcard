//! The repository interface: what `fc-app` may ask of storage, per area.
//!
//! * `*Read` traits are available in read and write transactions; `*Write` traits only in write
//!   transactions. [`Reads`] / [`Writes`] bundle them (blanket-implemented).
//! * All traits are object-safe; the app works with `&dyn Writes` / `Box<dyn WriteTx>`.
//! * Methods do **no business logic**: validation, inverse computation, cascades and events
//!   are the app's job. Storage only persists/loads rows faithfully.
//! * Soft delete: `set_*_deleted(ids, Some(t))` trashes, `(ids, None)` restores. Readers return
//!   deleted rows only when asked (`include_deleted`) or when fetching by id.

use crate::error::StorageResult;
use fc_core::attribute::{AttributeDef, AttributeValue, EntityRef};
use fc_core::card::Card;
use fc_core::deck::Deck;
use fc_core::note::Note;
use fc_core::notetype::NoteType;
use fc_core::paging::{Page, PageRequest};
use fc_core::preset::Preset;
use fc_core::settings::SettingScope;
use fc_core::tag::{Tag, TagName};
use fc_core::{
    AttributeDefId, AttributeOptionId, BatchId, CardId, DeckId, JobId, NoteId, NoteTypeId, OplogId,
    PresetId, TagId, Timestamp,
};
use serde_json::Value;

// ------------------------------------------------------------------ records

/// Per-deck card counts (own cards only; the app aggregates sub-decks).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CardCounts {
    /// New, not suspended/buried.
    pub new: u32,
    /// Learning/relearning with `due <= now`.
    pub learning: u32,
    /// Review with `due <= now`.
    pub review_due: u32,
    /// All live cards.
    pub total: u32,
}

/// Filter for note listing.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NoteFilter {
    /// Notes with at least one live card in any of these decks (`None` = all notes). The app
    /// expands sub-decks.
    pub deck_ids: Option<Vec<DeckId>>,
    /// Include trashed notes.
    pub include_deleted: bool,
}

/// A note row in a listing.
#[derive(Debug, Clone, PartialEq)]
pub struct NoteListRow {
    /// The note (with tags).
    pub note: Note,
    /// Number of live cards.
    pub card_count: u32,
}

/// Filter for card listing (all set conditions must hold).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CardFilter {
    /// Cards in these decks.
    pub deck_ids: Option<Vec<DeckId>>,
    /// Cards of these notes.
    pub note_ids: Option<Vec<NoteId>>,
    /// Include trashed cards.
    pub include_deleted: bool,
}

/// Oplog entry kind (`oplog.kind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OplogRecordKind {
    /// Normal command.
    Do,
    /// Undo of `reverts_batch_id`.
    Undo,
    /// Redo of `reverts_batch_id` (an undo batch).
    Redo,
}

/// One `oplog` row. JSON columns are opaque to storage (the app owns their shape:
/// `inverse` = `[InvokeRequest]`, `actor` = `Actor`, `summary` = `I18nMessage`).
#[derive(Debug, Clone, PartialEq)]
pub struct OplogRecord {
    /// Id.
    pub id: OplogId,
    /// Batch.
    pub batch_id: BatchId,
    /// Order within the batch (0-based).
    pub seq: u32,
    /// Qualified method name.
    pub command: String,
    /// Params.
    pub payload: Value,
    /// Inverse calls.
    pub inverse: Value,
    /// Actor JSON.
    pub actor: Value,
    /// Kind.
    pub kind: OplogRecordKind,
    /// For undo/redo.
    pub reverts_batch_id: Option<BatchId>,
    /// Summary JSON.
    pub summary: Value,
    /// Time.
    pub created_at: Timestamp,
}

/// One batch, as seen by the undo machinery.
#[derive(Debug, Clone, PartialEq)]
pub struct OplogBatch {
    /// Batch id.
    pub batch_id: BatchId,
    /// Kind of its entries.
    pub kind: OplogRecordKind,
    /// For undo/redo.
    pub reverts_batch_id: Option<BatchId>,
    /// Summary of the first entry.
    pub summary: Value,
    /// Time of the first entry.
    pub created_at: Timestamp,
}

/// One `jobs` row.
#[derive(Debug, Clone, PartialEq)]
pub struct JobRecord {
    /// Id.
    pub id: JobId,
    /// Kind.
    pub kind: String,
    /// `queued|running|succeeded|failed|cancelled|interrupted`.
    pub status: String,
    /// 0.0–1.0.
    pub progress: Option<f64>,
    /// Input.
    pub payload: Value,
    /// Output.
    pub result: Option<Value>,
    /// `ApiError` JSON.
    pub error: Option<Value>,
    /// Created.
    pub created_at: Timestamp,
    /// Updated.
    pub updated_at: Timestamp,
}

// ------------------------------------------------------------------ decks & presets

/// Deck reads.
pub trait DeckRead {
    /// Deck by id (also if trashed).
    fn get_deck(&self, id: DeckId) -> StorageResult<Option<Deck>>;
    /// All decks.
    fn list_decks(&self, include_deleted: bool) -> StorageResult<Vec<Deck>>;
    /// Live child of `parent` named `name` (Unicode case-insensitive).
    fn find_child_deck(&self, parent: Option<DeckId>, name: &str) -> StorageResult<Option<Deck>>;
    /// Own card counts per deck at `now` (decks without cards may be omitted). Cards of
    /// trashed notes are not counted.
    fn card_counts_by_deck(&self, now: Timestamp) -> StorageResult<Vec<(DeckId, CardCounts)>>;
    /// Ancestry of `id`: root first, `id` itself last (trashed decks included). Fails with
    /// `NotFound` for an unknown deck and `Corrupt` on a parent cycle.
    fn deck_ancestry(&self, id: DeckId) -> StorageResult<Vec<DeckId>>;
}

/// Deck writes.
pub trait DeckWrite: DeckRead {
    /// Inserts a new deck.
    fn insert_deck(&mut self, deck: &Deck) -> StorageResult<()>;
    /// Replaces all columns of an existing deck (by id).
    fn update_deck(&mut self, deck: &Deck) -> StorageResult<()>;
    /// Trash (`Some`) or restore (`None`). Returns rows changed.
    fn set_decks_deleted(
        &mut self,
        ids: &[DeckId],
        deleted_at: Option<Timestamp>,
    ) -> StorageResult<u32>;
}

/// Preset reads.
pub trait PresetRead {
    /// Preset by id.
    fn get_preset(&self, id: PresetId) -> StorageResult<Option<Preset>>;
    /// All presets.
    fn list_presets(&self) -> StorageResult<Vec<Preset>>;
}

/// Preset writes.
pub trait PresetWrite: PresetRead {
    /// Inserts or replaces.
    fn upsert_preset(&mut self, preset: &Preset) -> StorageResult<()>;
}

// ------------------------------------------------------------------ note types, notes, cards

/// Note type reads.
pub trait NoteTypeRead {
    /// Note type by id.
    fn get_notetype(&self, id: NoteTypeId) -> StorageResult<Option<NoteType>>;
    /// All note types.
    fn list_notetypes(&self, include_deleted: bool) -> StorageResult<Vec<NoteType>>;
}

/// Note type writes.
pub trait NoteTypeWrite: NoteTypeRead {
    /// Inserts or replaces (fields/templates stored as JSON).
    fn upsert_notetype(&mut self, nt: &NoteType) -> StorageResult<()>;
}

/// Note reads. Returned notes always carry their tags (from `note_tags`, in stored order).
pub trait NoteRead {
    /// Note by id.
    fn get_note(&self, id: NoteId) -> StorageResult<Option<Note>>;
    /// Paged listing, newest `updated_at` first.
    fn list_notes(
        &self,
        filter: &NoteFilter,
        page: &PageRequest,
    ) -> StorageResult<Page<NoteListRow>>;
    /// Live notes of `notetype` with this checksum (duplicate detection).
    fn find_notes_by_checksum(
        &self,
        notetype: NoteTypeId,
        checksum: &str,
    ) -> StorageResult<Vec<NoteId>>;
    /// Live notes carrying `tag` or any tag below it.
    fn note_ids_with_tag(&self, tag: &TagName) -> StorageResult<Vec<NoteId>>;
}

/// Note writes.
pub trait NoteWrite: NoteRead {
    /// Inserts a note, its `note_tags` (creating missing tags with `note.created_at`) and its
    /// `notes_fts` row with `search_text`.
    fn insert_note(&mut self, note: &Note, search_text: &str) -> StorageResult<()>;
    /// Replaces a note's columns, tags and FTS row.
    fn update_note(&mut self, note: &Note, search_text: &str) -> StorageResult<()>;
    /// Trash/restore. Returns rows changed.
    fn set_notes_deleted(
        &mut self,
        ids: &[NoteId],
        deleted_at: Option<Timestamp>,
    ) -> StorageResult<u32>;
}

/// Card reads.
pub trait CardRead {
    /// Card by id.
    fn get_card(&self, id: CardId) -> StorageResult<Option<Card>>;
    /// Cards matching `filter`.
    fn list_cards(&self, filter: &CardFilter) -> StorageResult<Vec<Card>>;
}

/// Card writes.
pub trait CardWrite: CardRead {
    /// Inserts a card.
    fn insert_card(&mut self, card: &Card) -> StorageResult<()>;
    /// Replaces a card's columns.
    fn update_card(&mut self, card: &Card) -> StorageResult<()>;
    /// Trash/restore. Returns rows changed.
    fn set_cards_deleted(
        &mut self,
        ids: &[CardId],
        deleted_at: Option<Timestamp>,
    ) -> StorageResult<u32>;
}

// ------------------------------------------------------------------ tags

/// Tag reads.
pub trait TagRead {
    /// All tags with the number of live notes carrying each.
    fn list_tags(&self) -> StorageResult<Vec<(Tag, u32)>>;
    /// Tag by name (case-insensitive).
    fn get_tag_by_name(&self, name: &TagName) -> StorageResult<Option<Tag>>;
}

/// Tag writes. Tag assignment happens through [`NoteWrite`].
pub trait TagWrite: TagRead {
    /// Inserts or replaces a tag row.
    fn upsert_tag(&mut self, tag: &Tag) -> StorageResult<()>;
    /// Removes tags no note references (also not trashed notes). Returns rows deleted.
    fn delete_unused_tags(&mut self) -> StorageResult<u32>;
    /// Moves every `note_tags` reference from tag `from` to tag `into` (a note carrying both
    /// keeps `into` at its own position), then deletes `from`. Returns the number of notes
    /// that carried `from`. Note rows (`updated_at`) are not touched. Hierarchical renames
    /// are `upsert_tag` (same id, new name) per tag, or this when the new name exists.
    fn merge_tag(&mut self, from: TagId, into: TagId) -> StorageResult<u32>;
}

// ------------------------------------------------------------------ attributes

/// Attribute reads.
pub trait AttributeRead {
    /// Definition by id.
    fn get_attribute_def(&self, id: AttributeDefId) -> StorageResult<Option<AttributeDef>>;
    /// All definitions.
    fn list_attribute_defs(&self, include_deleted: bool) -> StorageResult<Vec<AttributeDef>>;
    /// Values on one entity (definitions resolved for decoding; trashed definitions skipped).
    fn get_attribute_values(
        &self,
        entity: EntityRef,
    ) -> StorageResult<Vec<(AttributeDefId, AttributeValue)>>;
    /// Entities whose value of `attr` includes `option`.
    fn entities_with_option(
        &self,
        attr: AttributeDefId,
        option: AttributeOptionId,
    ) -> StorageResult<Vec<EntityRef>>;
    /// Definitions that apply to `deck` (`None` = collection only), resolved with
    /// `fc_core::attribute::effective_attribute_defs` over the deck's ancestry: live
    /// definitions only, deepest `key` wins. Each comes with its depth (`0` = collection,
    /// `ancestry.len()` = defined on `deck` itself), in the resolver's order.
    fn effective_attribute_defs_for_deck(
        &self,
        deck: Option<DeckId>,
    ) -> StorageResult<Vec<(AttributeDef, usize)>>;
}

/// Attribute writes.
pub trait AttributeWrite: AttributeRead {
    /// Inserts a definition.
    fn insert_attribute_def(&mut self, def: &AttributeDef) -> StorageResult<()>;
    /// Replaces a definition's columns (options JSON included).
    fn update_attribute_def(&mut self, def: &AttributeDef) -> StorageResult<()>;
    /// Trash/restore a definition. Values are kept.
    fn set_attribute_def_deleted(
        &mut self,
        id: AttributeDefId,
        deleted_at: Option<Timestamp>,
    ) -> StorageResult<()>;
    /// Sets values on one entity: `Some` replaces that attribute's rows, `None` removes them.
    /// Attributes not listed are untouched. Rows use `AttributeValue::to_stored()`.
    fn set_attribute_values(
        &mut self,
        entity: EntityRef,
        values: &[(AttributeDefId, Option<AttributeValue>)],
    ) -> StorageResult<()>;
}

// ------------------------------------------------------------------ oplog

/// Oplog reads.
pub trait OplogRead {
    /// Entries, newest first (cursor = opaque).
    fn list_oplog(&self, page: &PageRequest) -> StorageResult<Page<OplogRecord>>;
    /// Batches newest first, starting before `before` (exclusive) if given.
    fn list_oplog_batches(
        &self,
        before: Option<BatchId>,
        limit: u32,
    ) -> StorageResult<Vec<OplogBatch>>;
    /// Entries of a batch in `seq` order.
    fn oplog_batch_entries(&self, batch: BatchId) -> StorageResult<Vec<OplogRecord>>;
}

/// Oplog writes (append-only).
pub trait OplogWrite: OplogRead {
    /// Appends an entry.
    fn append_oplog(&mut self, record: &OplogRecord) -> StorageResult<()>;
}

// ------------------------------------------------------------------ settings & modules

/// Settings reads.
pub trait SettingsRead {
    /// Stored value at exactly `scope`.
    fn get_setting(&self, scope: &SettingScope, key: &str) -> StorageResult<Option<Value>>;
    /// All stored values at `scope`.
    fn list_settings(&self, scope: &SettingScope) -> StorageResult<Vec<(String, Value)>>;
}

/// Settings writes.
pub trait SettingsWrite: SettingsRead {
    /// Upserts a value.
    fn set_setting(
        &mut self,
        scope: &SettingScope,
        key: &str,
        value: &Value,
        now: Timestamp,
    ) -> StorageResult<()>;
    /// Removes an override. Returns whether a row existed.
    fn delete_setting(&mut self, scope: &SettingScope, key: &str) -> StorageResult<bool>;
}

/// Module switch reads.
pub trait ModuleStateRead {
    /// Stored state of every module that has one (absent = module default).
    fn list_module_states(&self) -> StorageResult<Vec<(String, bool)>>;
}

/// Module switch writes.
pub trait ModuleStateWrite: ModuleStateRead {
    /// Upserts a module switch.
    fn set_module_enabled(&mut self, id: &str, enabled: bool, now: Timestamp) -> StorageResult<()>;
}

// ------------------------------------------------------------------ jobs

/// Job reads.
pub trait JobRead {
    /// Job by id.
    fn get_job(&self, id: JobId) -> StorageResult<Option<JobRecord>>;
    /// Newest first.
    fn list_jobs(&self, limit: u32) -> StorageResult<Vec<JobRecord>>;
}

/// Job writes.
pub trait JobWrite: JobRead {
    /// Inserts or replaces.
    fn upsert_job(&mut self, job: &JobRecord) -> StorageResult<()>;
    /// At startup: `running`/`queued` → `interrupted`. Returns rows changed.
    fn mark_unfinished_jobs_interrupted(&mut self, now: Timestamp) -> StorageResult<u32>;
    /// Claims the oldest `queued` job: sets it `running` with `updated_at = now` and returns
    /// it (`None` if the queue is empty).
    fn claim_next_job(&mut self, now: Timestamp) -> StorageResult<Option<JobRecord>>;
}

// ------------------------------------------------------------------ bundles

/// Everything readable. Blanket-implemented.
pub trait Reads:
    DeckRead
    + PresetRead
    + NoteTypeRead
    + NoteRead
    + CardRead
    + TagRead
    + AttributeRead
    + OplogRead
    + SettingsRead
    + ModuleStateRead
    + JobRead
{
}

impl<T> Reads for T where
    T: DeckRead
        + PresetRead
        + NoteTypeRead
        + NoteRead
        + CardRead
        + TagRead
        + AttributeRead
        + OplogRead
        + SettingsRead
        + ModuleStateRead
        + JobRead
{
}

/// Everything writable. Blanket-implemented.
pub trait Writes:
    Reads
    + DeckWrite
    + PresetWrite
    + NoteTypeWrite
    + NoteWrite
    + CardWrite
    + TagWrite
    + AttributeWrite
    + OplogWrite
    + SettingsWrite
    + ModuleStateWrite
    + JobWrite
{
}

impl<T> Writes for T where
    T: Reads
        + DeckWrite
        + PresetWrite
        + NoteTypeWrite
        + NoteWrite
        + CardWrite
        + TagWrite
        + AttributeWrite
        + OplogWrite
        + SettingsWrite
        + ModuleStateWrite
        + JobWrite
{
}
