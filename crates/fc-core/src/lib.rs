//! # fc-core
//!
//! Pure domain core of Flashcard: models, typed ids, validation and other pure functions.
//!
//! **Purity rules** (enforced by review and CI grep): no file system, no network, no
//! `SystemTime::now()`, no global mutable state, no OS randomness. Time comes in through
//! [`time::Clock`], randomness through [`rng::Rng`], ids through [`id::IdGen`].
//!
//! The optional `ts` feature derives `ts_rs::TS` on value types re-used by `fc-api` DTOs.
#![forbid(unsafe_code)]

pub mod attribute;
pub mod automation;
pub mod card;
pub mod content;
pub mod deck;
pub mod error;
pub mod goal;
pub mod i18n;
pub mod id;
pub mod inbox;
pub mod inline_cards;
pub mod maintenance;
pub mod note;
pub mod notetype;
pub mod paging;
pub mod preset;
pub mod query;
pub mod reading;
pub mod rng;
pub mod settings;
pub mod shortcut;
pub mod stats;
pub mod tag;
pub mod time;

pub use error::{CoreError, CoreResult};
pub use id::{
    AttributeDefId, AttributeOptionId, BatchId, CardId, DeckId, ExamId, GoalId, IdGen, JobId,
    McpServerId, NoteId, NoteTypeId, OplogId, PresetId, ReviewLogId, SavedQueryId, SessionId,
    TagId, V7IdGen,
};
// v0.8 (own line so parallel tracks adding ids do not collide).
pub use id::AiProviderId;
// v0.10 reading.
pub use id::ReadingItemId;
// v0.9 automation rules.
pub use id::AutomationRuleId;
// v0.9 local API + inbox (own line).
pub use id::{ApiClientId, InboxItemId};
pub use rng::Rng;
pub use time::{Clock, Timestamp};
