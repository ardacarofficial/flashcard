//! Operation log and undo/redo DTOs (docs/03 §5).

use super::common::I18nMessage;
use crate::envelope::InvokeRequest;
use fc_core::paging::PageRequest;
use fc_core::{BatchId, OplogId, Timestamp};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

/// Who issued a command.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum Actor {
    /// The user through the UI.
    User,
    /// The CLI.
    Cli,
    /// A plugin (plugin id).
    Plugin(String),
    /// An automation rule (rule id).
    Automation(String),
    /// Local HTTP API client (client name).
    LocalApi(String),
    /// An MCP client through the MCP server (client name from `initialize`, e.g.
    /// `claude-ai`); v0.8.
    Mcp(String),
    /// The app itself (migrations, seeding, maintenance).
    System,
}

/// What an oplog batch did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum OplogKind {
    /// A normal command.
    Do,
    /// Reverted an earlier batch (`reverts_batch_id`).
    Undo,
    /// Re-applied a batch that was undone (`reverts_batch_id` = the undo batch).
    Redo,
}

/// One oplog row.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct OplogEntryDto {
    /// Id.
    pub id: OplogId,
    /// Batch (one undo step).
    pub batch_id: BatchId,
    /// Qualified method name.
    pub command: String,
    /// Request params as executed.
    #[ts(type = "unknown")]
    pub payload: Value,
    /// Calls that revert this entry, in execution order.
    pub inverse: Vec<InvokeRequest>,
    /// Issuer.
    pub actor: Actor,
    /// Do/undo/redo.
    pub kind: OplogKind,
    /// For undo/redo: the batch reverted.
    pub reverts_batch_id: Option<BatchId>,
    /// Human summary ("50 cards moved").
    pub summary: I18nMessage,
    /// Time.
    pub created_at: Timestamp,
}

/// `oplog.list`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ListOplog {
    /// Paging (newest first).
    #[serde(default)]
    pub page: PageRequest,
}

/// Response of `oplog.list`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct OplogPage {
    /// Entries, newest first.
    pub items: Vec<OplogEntryDto>,
    /// Next cursor.
    pub next_cursor: Option<String>,
}

/// Response of `oplog.undo` / `oplog.redo`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct UndoResult {
    /// New batch written by the undo/redo.
    pub batch_id: BatchId,
    /// Batch that was reverted.
    pub reverted_batch_id: BatchId,
    /// What was undone/redone ("Undone: 50 cards moved").
    pub summary: I18nMessage,
}

/// Response of `oplog.status`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct UndoStatus {
    /// Something can be undone.
    pub can_undo: bool,
    /// Something can be redone.
    pub can_redo: bool,
    /// Summary of the next undo.
    pub undo_summary: Option<I18nMessage>,
    /// Summary of the next redo.
    pub redo_summary: Option<I18nMessage>,
}

/// `batch.execute`: runs commands in one transaction and one undo step; all or nothing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ExecuteBatch {
    /// Commands (queries are rejected).
    pub calls: Vec<InvokeRequest>,
}

/// Response of `batch.execute`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct BatchResult {
    /// Batch id (undo step).
    pub batch_id: BatchId,
    /// Response of each call, in order.
    #[ts(type = "Array<unknown>")]
    pub results: Vec<Value>,
}
