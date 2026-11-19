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

use super::from_json;
use crate::error::AppResult;
use fc_api::v1::common::I18nMessage;
use fc_core::BatchId;
use fc_storage::repo::{OplogBatch, OplogRecordKind};
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
}

/// Scans the oplog (see module docs).
pub(crate) fn scan(reads: &dyn Reads) -> AppResult<History> {
    let mut out = History::default();
    let mut cancelled: HashSet<BatchId> = HashSet::new();
    let mut redo_blocked = false;
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
            if out.undo.is_none() && matches!(b.kind, OplogRecordKind::Do | OplogRecordKind::Redo) {
                out.undo = Some(b);
            }
            if out.undo.is_some() && (out.redo.is_some() || redo_blocked) {
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
