//! Linear undo/redo over the append-only oplog.
//!
//! The history is never rewritten; undo/redo append batches that revert another batch
//! (`reverts_batch_id`). Walking the batches newest → oldest:
//!
//! * a batch reverted by a later, still-effective undo/redo batch is *cancelled* and ignored
//!   (including the cancellations it made itself);
//! * the **undo target** is the newest non-cancelled `do` or `redo` batch;
//! * the **redo target** is the newest non-cancelled `undo` batch, provided no non-cancelled
//!   `do` batch comes after it (a new action clears the redo stack).
//! * a **history barrier** batch (a `do` batch holding an [`OPLOG_BARRIER_COMMAND`] entry,
//!   written by a restore from backup or a trash purge) ends the undo walk: if it is reached
//!   before an undo target, undo fails with `CONFLICT_HISTORY_BARRIER`. Older batches stay
//!   listed (`oplog.list`) but are never undone. Like any `do` batch it also clears redo.

use super::from_json;
use crate::error::{AppError, AppResult};
use crate::handler::{CommandCtx, PendingOplogEntry};
use fc_api::v1::common::I18nMessage;
use fc_api::{ApiError, ErrorCode};
use fc_core::BatchId;
use fc_storage::repo::{OplogBatch, OplogRecordKind, OPLOG_BARRIER_COMMAND};
use fc_storage::Reads;
use std::collections::HashSet;

const PAGE: u32 = 256;

/// What undo and redo would act on now.
#[derive(Debug, Default)]
pub(crate) struct History {
    /// Batch `oplog.undo` reverts.
    pub undo: Option<OplogBatch>,
    /// Batch `oplog.redo` reverts (an undo batch).
    pub redo: Option<OplogBatch>,
    /// The history barrier that stopped the undo walk (then `undo` is `None`).
    pub barrier: Option<OplogBatch>,
}

impl History {
    /// The undo target, or the error explaining why there is none.
    pub(crate) fn undo_target(self) -> AppResult<OplogBatch> {
        if let Some(b) = self.undo {
            return Ok(b);
        }
        match self.barrier {
            Some(barrier) => {
                let reason = barrier_reason(&barrier);
                Err(AppError::Api(
                    ApiError::new(
                        ErrorCode::ConflictHistoryBarrier,
                        "undo cannot go past this point in history",
                    )
                    .with_detail("reason", reason)
                    .with_detail("batch_id", barrier.batch_id.to_string()),
                ))
            }
            None => Err(AppError::api(
                ErrorCode::ConflictNothingToUndo,
                "nothing to undo",
            )),
        }
    }
}

/// Reason of a barrier batch (`restore` / `purge`), from its summary key.
fn barrier_reason(b: &OplogBatch) -> String {
    b.summary
        .get("key")
        .and_then(|k| k.as_str())
        .and_then(|k| k.strip_prefix(BARRIER_SUMMARY_PREFIX))
        .unwrap_or("unknown")
        .to_owned()
}

const BARRIER_SUMMARY_PREFIX: &str = "oplog.summary.oplog.barrier.";

/// Why a history barrier is written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BarrierReason {
    /// The collection was replaced from a backup (the oplog starts afresh).
    Restore,
    /// Trashed items were deleted for good (`count` of them).
    Purge {
        /// Purged items.
        count: u32,
    },
}

/// Records a history barrier in this dispatch's batch (see the module docs).
pub(crate) fn push_barrier(ctx: &mut CommandCtx<'_>, reason: BarrierReason) {
    let (name, summary) = match reason {
        BarrierReason::Restore => ("restore", None),
        BarrierReason::Purge { count } => ("purge", Some(count)),
    };
    let mut message = I18nMessage::key(format!("{BARRIER_SUMMARY_PREFIX}{name}"));
    if let Some(count) = summary {
        message = message.param("count", count);
    }
    ctx.oplog.push(Some(PendingOplogEntry {
        command: OPLOG_BARRIER_COMMAND.to_owned(),
        payload: serde_json::json!({ "reason": name }),
        inverse: Vec::new(),
        summary: message,
    }));
}

/// Scans the oplog (see module docs).
pub(crate) fn scan(reads: &dyn Reads) -> AppResult<History> {
    let mut out = History::default();
    let mut cancelled: HashSet<BatchId> = HashSet::new();
    let mut redo_blocked = false;
    let mut undo_done = false;
    let mut before: Option<BatchId> = None;
    loop {
        let page = reads.list_oplog_batches(before, PAGE)?;
        let Some(last) = page.last() else {
            break;
        };
        before = Some(last.batch_id);
        let exhausted = page.len() < PAGE as usize;
        for b in page {
            if cancelled.contains(&b.batch_id) {
                continue;
            }
            match b.kind {
                OplogRecordKind::Do => redo_blocked = true,
                OplogRecordKind::Undo if !redo_blocked && out.redo.is_none() => {
                    out.redo = Some(b.clone());
                }
                _ => {}
            }
            if matches!(b.kind, OplogRecordKind::Undo | OplogRecordKind::Redo) {
                if let Some(r) = b.reverts_batch_id {
                    cancelled.insert(r);
                }
            }
            if !undo_done && matches!(b.kind, OplogRecordKind::Do | OplogRecordKind::Redo) {
                undo_done = true;
                if b.barrier {
                    out.barrier = Some(b);
                } else {
                    out.undo = Some(b);
                }
            }
            if undo_done && (out.redo.is_some() || redo_blocked) {
                return Ok(out);
            }
        }
        if exhausted {
            break;
        }
    }
    Ok(out)
}

/// User-facing summary of a batch: for undo/redo batches, the summary of the original action
/// they (transitively) revert.
pub(crate) fn display_summary(reads: &dyn Reads, batch: &OplogBatch) -> AppResult<I18nMessage> {
    let mut summary = batch.summary.clone();
    let mut next = match batch.kind {
        OplogRecordKind::Do => None,
        _ => batch.reverts_batch_id,
    };
    let mut hops = 0;
    while let Some(id) = next {
        hops += 1;
        let entries = reads.oplog_batch_entries(id)?;
        let Some(first) = entries.first() else {
            break;
        };
        summary = first.summary.clone();
        next = match first.kind {
            OplogRecordKind::Do => None,
            _ => first.reverts_batch_id,
        };
        if hops > 64 {
            break;
        }
    }
    from_json(&summary)
}
