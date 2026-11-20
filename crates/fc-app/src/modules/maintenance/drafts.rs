//! Form drafts (crash recovery: "taslaklar kalıcı", docs/02 §34). Forms autosave their content
//! under a key while the user types and discard it when they save or clear the form; after a
//! crash the form (or the recovery offer) brings it back.
//!
//! Drafts are scratch state, not collection data: saving one is not an undo step and is not
//! recorded in the oplog (like `study.start`'s session bookkeeping). The number of drafts is
//! bounded: saving beyond [`MAX_DRAFTS`] drops the oldest.

use super::done;
use crate::error::{AppError, AppResult};
use crate::handler::{CommandCtx, CommandOutcome, QueryCtx};
use fc_api::v1::common::Empty;
use fc_api::v1::maintenance::{DraftDto, DraftList, DraftRef, DraftResult, SaveDraft};
use fc_api::v1::methods::{MaintenanceDraftsDiscard, MaintenanceDraftsSave};
use fc_api::{ApiError, ErrorCode};
use fc_storage::repo::DraftRecord;

/// Longest key.
pub const MAX_KEY: usize = 100;
/// Largest payload (bytes of JSON).
pub const MAX_PAYLOAD_BYTES: usize = 256 * 1024;
/// Drafts kept.
pub const MAX_DRAFTS: usize = 50;

pub(super) fn dto(r: DraftRecord) -> DraftDto {
    DraftDto {
        key: r.key,
        payload: r.payload,
        created_at: r.created_at,
        updated_at: r.updated_at,
    }
}

fn check_key(key: &str) -> AppResult<()> {
    let ok = !key.is_empty()
        && key.len() <= MAX_KEY
        && key
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | ':' | '-'));
    if ok {
        Ok(())
    } else {
        Err(AppError::Api(
            ApiError::new(ErrorCode::ValidationDraftKey, "invalid draft key")
                .with_detail("field", "key"),
        ))
    }
}

pub(super) fn save(req: SaveDraft, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<DraftDto>> {
    check_key(&req.key)?;
    let size = req.payload.to_string().len();
    if size > MAX_PAYLOAD_BYTES {
        return Err(AppError::Api(
            ApiError::new(ErrorCode::ValidationDraftTooLarge, "draft too large")
                .with_detail("max_bytes", MAX_PAYLOAD_BYTES),
        ));
    }
    let created_at = ctx
        .reads()
        .get_draft(&req.key)?
        .map_or(ctx.now, |d| d.created_at);
    let record = DraftRecord {
        key: req.key,
        payload: req.payload,
        created_at,
        updated_at: ctx.now,
    };
    ctx.tx.upsert_draft(&record)?;
    let all = ctx.reads().list_drafts()?;
    for old in all.iter().skip(MAX_DRAFTS) {
        ctx.tx.delete_draft(&old.key)?;
    }
    Ok(done::<MaintenanceDraftsSave, _>(dto(record)))
}

pub(super) fn get(req: DraftRef, ctx: &QueryCtx<'_>) -> AppResult<DraftResult> {
    check_key(&req.key)?;
    Ok(DraftResult {
        draft: ctx.tx.get_draft(&req.key)?.map(dto),
    })
}

pub(super) fn list(_: Empty, ctx: &QueryCtx<'_>) -> AppResult<DraftList> {
    Ok(DraftList {
        items: ctx.tx.list_drafts()?.into_iter().map(dto).collect(),
    })
}

pub(super) fn discard(req: DraftRef, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<DraftResult>> {
    check_key(&req.key)?;
    let draft = ctx.reads().get_draft(&req.key)?;
    if draft.is_some() {
        ctx.tx.delete_draft(&req.key)?;
    }
    Ok(done::<MaintenanceDraftsDiscard, _>(DraftResult {
        draft: draft.map(dto),
    }))
}
