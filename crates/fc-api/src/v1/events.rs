//! Domain events pushed to subscribers after a command commits (docs/03 §8).

use super::attributes::AttributeDefDto;
use super::common::{AffectedCount, EntityRef, I18nMessage};
use super::decks::DeckDto;
use super::jobs::JobDto;
use fc_core::settings::SettingScope;
use fc_core::{AttributeDefId, BatchId, CardId, DeckId, JobId, NoteId, NoteTypeId, Timestamp};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Event as delivered on the event stream.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct EventEnvelope {
    /// Monotonic sequence number within the app run (gap = missed events → refetch).
    pub seq: u64,
    /// Commit time.
    pub at: Timestamp,
    /// Oplog batch that produced the event (`null` for non-command events like job progress).
    pub batch_id: Option<BatchId>,
    /// The event.
    pub event: DomainEvent,
}

/// Domain events. JSON: `{"type": "deck_created", "data": {...}}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum DomainEvent {
    /// A deck was created.
    DeckCreated {
        /// The deck.
        deck: DeckDto,
    },
    /// A deck's name/metadata changed.
    DeckUpdated {
        /// The deck.
        deck: DeckDto,
    },
    /// A deck was re-parented/reordered.
    DeckMoved {
        /// The deck.
        deck: DeckDto,
        /// Previous parent.
        old_parent_id: Option<DeckId>,
    },
    /// A deck (and its contents) went to the trash.
    DeckDeleted {
        /// Deck id.
        id: DeckId,
        /// What was affected.
        affected: AffectedCount,
    },
    /// A deck came back from the trash.
    DeckRestored {
        /// The deck.
        deck: DeckDto,
    },
    /// A note was created.
    NoteCreated {
        /// Note id.
        note_id: NoteId,
        /// Deck of its cards.
        deck_id: DeckId,
    },
    /// A note's content/tags/attributes changed.
    NoteUpdated {
        /// Note id.
        note_id: NoteId,
    },
    /// Notes went to the trash.
    NotesDeleted {
        /// Note ids.
        ids: Vec<NoteId>,
    },
    /// Notes came back from the trash.
    NotesRestored {
        /// Note ids.
        ids: Vec<NoteId>,
    },
    /// Tags were renamed/merged/recolored (refetch `tags.list`).
    TagsChanged,
    /// A note type was created, changed, trashed or restored (v0.3).
    NoteTypeChanged {
        /// Note type id.
        notetype_id: NoteTypeId,
    },
    /// Card metadata (flags, card note) or generated cards changed (v0.3).
    CardsUpdated {
        /// Card ids.
        ids: Vec<CardId>,
    },
    /// Flag names/colors changed (refetch `flags.list`, v0.3).
    FlagsChanged,
    /// An attribute definition was created or restored.
    AttributeDefined {
        /// Definition.
        def: AttributeDefDto,
    },
    /// An attribute definition or its options changed.
    AttributeUpdated {
        /// Definition.
        def: AttributeDefDto,
    },
    /// An attribute definition went to the trash.
    AttributeDeleted {
        /// Definition id.
        id: AttributeDefId,
    },
    /// Attribute values changed on these entities.
    AttributeValuesChanged {
        /// Entities.
        entities: Vec<EntityRef>,
    },
    /// A setting changed.
    SettingChanged {
        /// Scope.
        scope: SettingScope,
        /// Key.
        key: String,
    },
    /// A module was enabled/disabled (refetch `modules.list`).
    ModuleToggled {
        /// Module id.
        id: String,
        /// New state.
        enabled: bool,
    },
    /// A job made progress or changed status.
    JobProgress {
        /// Job snapshot.
        job: JobDto,
    },
    /// An import job committed (refetch decks/notes).
    Imported {
        /// Job.
        job_id: JobId,
        /// Notes added or updated.
        notes: u32,
    },
    /// The whole collection was replaced (backup restored): refetch everything.
    CollectionReplaced,
    /// Another process (the CLI, an MCP client…) changed the collection while the app was
    /// open: refetch whatever is on screen.
    DataChanged,
    /// Scheduling/deck/flag/suspension columns of these cards changed (bulk operations).
    CardsChanged {
        /// Cards.
        ids: Vec<CardId>,
    },
    /// Saved queries changed (refetch `saved_queries.list`).
    SavedQueriesChanged,
    /// Presets or deck preset assignments changed (refetch `presets.list`).
    PresetsChanged,
    /// Exam dates changed (refetch `planning.exams.list`; v0.6).
    ExamsChanged,
    /// Goals changed (refetch `goals.list`; v0.6).
    GoalsChanged,
    /// AI providers, routes or prices changed (refetch `ai.*` lists; v0.8). Never carries a key.
    AiConfigChanged,
    /// Output of a running `ai.request` job so far (streamed explanations and hints, v0.8):
    /// the whole text received until now, so a missed event loses nothing. The job's result
    /// is still the final word.
    AiPartial {
        /// Job.
        job_id: JobId,
        /// Text so far.
        text: String,
    },
    /// A batch was undone.
    Undone {
        /// Batch reverted.
        reverted_batch_id: BatchId,
        /// Summary.
        summary: I18nMessage,
    },
    /// A batch was redone.
    Redone {
        /// Batch re-applied.
        reverted_batch_id: BatchId,
        /// Summary.
        summary: I18nMessage,
    },
    /// Study changed cards' scheduling or visibility (answer, suspend, bury, postpone and
    /// their undo): deck counts are stale.
    StudyCardsChanged {
        /// Cards.
        card_ids: Vec<CardId>,
    },
}
