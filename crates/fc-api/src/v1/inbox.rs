//! Inbox (v0.9, docs/06 §4): proposed cards from outside the app — the local HTTP API, the
//! browser extension, `flashcard://add` links, the quick-add window, the clipboard watcher —
//! wait here until the user accepts them as notes (one undo step) or rejects them.
//!
//! Also the small DTOs of the `deep_links` and `clipboard` modules, which feed the inbox.

use fc_core::paging::PageRequest;
use fc_core::{ApiClientId, DeckId, InboxItemId, NoteId, NoteTypeId, Timestamp};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use ts_rs::TS;

/// Where a proposed card came from (the source page goes in [`InboxContext`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InboxSource {
    /// Added by hand in the app.
    User,
    /// A clipboard suggestion the user accepted.
    Clipboard,
    /// A `flashcard://add` link.
    DeepLink,
    /// A local HTTP API client (e.g. the browser extension).
    LocalApi {
        /// Client.
        client_id: ApiClientId,
        /// Client name at the time.
        client_name: String,
    },
    /// Another caller (MCP client, automation, plugin, CLI): the actor's kind and id.
    Other {
        /// Actor kind (`mcp`, `automation`, `plugin`, `cli`, `system`).
        actor: String,
        /// Actor id.
        id: Option<String>,
    },
}

/// How the app itself says it got the item (`AddInboxItem.via`; only the app may set it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum InboxVia {
    /// Added by hand.
    User,
    /// Clipboard suggestion.
    Clipboard,
    /// A `flashcard://add` link (`deep_links.open`).
    DeepLink,
}

/// Where the text was found.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct InboxContext {
    /// Page address.
    pub url: Option<String>,
    /// Page title.
    pub title: Option<String>,
    /// The sentence the selection was in.
    pub sentence: Option<String>,
}

/// `inbox.add`: proposes a card. See docs/06 §3 for the wire contract.
///
/// `front`/`back` are the Classic shorthand; other note types use `fields`. Classification
/// values (`attributes`) are keyed by the classification's key, name or id and hold option
/// labels/ids (a list for multi-choice), numbers, booleans, text or `YYYY-MM-DD`; they are
/// checked for shape here and resolved against the deck when the item is accepted.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct AddInboxItem {
    /// Front (Classic).
    #[serde(default)]
    pub front: Option<String>,
    /// Back (Classic).
    #[serde(default)]
    pub back: Option<String>,
    /// Field values by field key (other note types).
    #[serde(default)]
    pub fields: BTreeMap<String, String>,
    /// Target deck (may be chosen when accepting).
    #[serde(default)]
    pub deck_id: Option<DeckId>,
    /// Note type (`null` = Classic).
    #[serde(default)]
    pub notetype_id: Option<NoteTypeId>,
    /// Tags.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Classification values: key/name/id → label(s)/value.
    #[serde(default)]
    #[ts(type = "Record<string, string | Array<string> | number | boolean>")]
    pub attributes: BTreeMap<String, Value>,
    /// Source page.
    #[serde(default)]
    pub context: Option<InboxContext>,
    /// Client-chosen idempotency key (1–100 chars): a resend with the same key from the same
    /// client returns the existing item instead of adding another.
    #[serde(default)]
    pub client_ref: Option<String>,
    /// How the app got it (clipboard, deep link). Only the app itself may set this.
    #[serde(default)]
    pub via: Option<InboxVia>,
}

/// Item state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum InboxStatus {
    /// Waiting for review.
    Pending,
    /// Became notes.
    Accepted,
    /// Rejected.
    Rejected,
}

/// One inbox item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct InboxItemDto {
    /// Id.
    pub id: InboxItemId,
    /// Origin.
    pub source: InboxSource,
    /// State.
    pub status: InboxStatus,
    /// The proposal as received (`via` cleared).
    pub item: AddInboxItem,
    /// Notes created when accepted.
    pub note_ids: Vec<NoteId>,
    /// Received.
    pub created_at: Timestamp,
    /// Last state change.
    pub updated_at: Timestamp,
}

/// `inbox.list`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct ListInbox {
    /// State filter (default `pending`).
    #[serde(default)]
    pub status: Option<InboxStatus>,
    /// Paging (newest first).
    #[serde(default)]
    pub page: PageRequest,
}

/// Response of `inbox.list`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct InboxPage {
    /// Items, newest first.
    pub items: Vec<InboxItemDto>,
    /// Cursor for the next page (`null` = end).
    pub next_cursor: Option<String>,
    /// Pending items in total (menu badge).
    pub pending: u32,
}

/// One item of `inbox.accept`, with optional edits made during review.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct AcceptInboxItem {
    /// Item.
    pub id: InboxItemId,
    /// Deck (`null` = the item's).
    #[serde(default)]
    pub deck_id: Option<DeckId>,
    /// Note type (`null` = the item's, else Classic).
    #[serde(default)]
    pub notetype_id: Option<NoteTypeId>,
    /// Replaces the item's fields (`front`/`back` included) when set.
    #[serde(default)]
    pub fields: Option<BTreeMap<String, String>>,
    /// Replaces the item's tags when set.
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    /// Replaces the item's classification values when set.
    #[serde(default)]
    #[ts(type = "Record<string, string | Array<string> | number | boolean> | null")]
    pub attributes: Option<BTreeMap<String, Value>>,
}

/// `inbox.accept`: turns pending items into notes — all or nothing, one undo step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct AcceptInbox {
    /// Items (1–500).
    pub items: Vec<AcceptInboxItem>,
    /// Create notes even if duplicates exist (default `false` → `CONFLICT_NOTE_DUPLICATE`
    /// with `details.item_index`).
    #[serde(default)]
    pub allow_duplicates: Option<bool>,
}

/// One accepted item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AcceptedInboxItem {
    /// Item.
    pub id: InboxItemId,
    /// Note created.
    pub note_id: NoteId,
}

/// Response of `inbox.accept`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AcceptInboxResult {
    /// In request order.
    pub accepted: Vec<AcceptedInboxItem>,
}

/// `inbox.reject` (and internal `inbox.remove`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct InboxItemIds {
    /// Items (1–500).
    pub ids: Vec<InboxItemId>,
}

/// Response of `inbox.reject`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct InboxChangeCount {
    /// Items changed.
    pub changed: u32,
}

/// Internal `inbox.restore`: puts rows back exactly (inverse of remove/accept/reject).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct RestoreInboxItems {
    /// Rows as they were.
    pub items: Vec<InboxItemDto>,
}

/// Response of `inbox.count`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct InboxCount {
    /// Pending items.
    pub pending: u32,
}

// ------------------------------------------------------------------ deep links

/// `deep_links.open`: what the OS handed the app.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct OpenDeepLink {
    /// The `flashcard://…` link.
    pub url: String,
}

/// Where a navigation link goes (the shell shows the screen).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DeepLinkTarget {
    /// A deck (checked to exist).
    Deck {
        /// Deck.
        deck_id: DeckId,
    },
    /// A note (checked to exist).
    Note {
        /// Note.
        note_id: NoteId,
    },
    /// Study a deck or a query (the query is checked to parse).
    Study {
        /// Deck.
        deck_id: Option<DeckId>,
        /// Query.
        query: Option<String>,
    },
}

/// Response of `deep_links.open`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DeepLinkOutcome {
    /// Show a screen; nothing was written.
    Navigate {
        /// Target.
        target: DeepLinkTarget,
    },
    /// A writing link (`add`) became an inbox item for the user to review.
    Inbox {
        /// The new item.
        item: Box<InboxItemDto>,
    },
}

// ------------------------------------------------------------------ clipboard

/// `clipboard.check`: should copied text be suggested as a card?
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct CheckClipboard {
    /// Copied text.
    pub text: String,
}

/// Response of `clipboard.check`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ClipboardVerdict {
    /// Show the suggestion.
    pub suggest: bool,
    /// Trimmed text to propose (empty when not suggested).
    pub text: String,
    /// Why not (`empty`, `too_short`, `too_long`, `multiline`, `secret`, `not_text`).
    pub reason: Option<String>,
}
