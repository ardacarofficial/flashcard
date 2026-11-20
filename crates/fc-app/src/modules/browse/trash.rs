//! Trash retention (docs/02 §10: trashed items are kept for `browse.trash_retention_days`,
//! default 30, then deleted for good).
//!
//! `browse.purge_trash` is the one place that deletes permanently. It runs at startup
//! ([`purge_on_startup`]) and is the body of the `browse.purge_trash` job ([`PurgeTrashJob`])
//! for the worker pool (v0.5) to schedule periodically.

use crate::error::AppResult;
use crate::handler::{CommandCtx, CommandOutcome};
use crate::jobs::{JobContext, JobHandler};
use crate::support::summary;
use fc_api::v1::browse::{PurgeResult, PurgeTrash};
use fc_api::v1::methods::BrowsePurgeTrash;
use fc_api::v1::oplog::Actor;
use fc_api::{ApiMethod, InvokeRequest};
use fc_core::settings::{SettingDef, SettingKind, SettingScope, SettingScopeKind};
use fc_core::time::MILLIS_PER_DAY;
use fc_core::Timestamp;
use serde_json::Value;

/// Setting key: days trashed items are kept.
pub const RETENTION_KEY: &str = "browse.trash_retention_days";
/// Default retention (days).
pub const DEFAULT_RETENTION_DAYS: u32 = 30;
/// Longest retention (10 years).
pub const MAX_RETENTION_DAYS: u32 = 3650;

/// The retention setting (app scope; integer days, 1–3650).
pub fn retention_setting() -> SettingDef {
    SettingDef {
        key: RETENTION_KEY.to_owned(),
        kind: SettingKind::Integer {
            min: Some(1),
            max: Some(i64::from(MAX_RETENTION_DAYS)),
        },
        default: Value::from(DEFAULT_RETENTION_DAYS),
        label_key: format!("settings.{RETENTION_KEY}.label"),
        description_key: Some(format!("settings.{RETENTION_KEY}.description")),
        advanced: false,
        scopes: vec![SettingScopeKind::App],
    }
}

fn retention_days(reads: &dyn fc_storage::Reads) -> AppResult<u32> {
    let stored = crate::settings::resolve_stored(reads, &SettingScope::App, RETENTION_KEY)?;
    Ok(stored
        .and_then(|(v, _)| v.as_u64())
        .and_then(|d| u32::try_from(d).ok())
        .unwrap_or(DEFAULT_RETENTION_DAYS)
        .min(MAX_RETENTION_DAYS))
}

pub(super) fn purge(req: PurgeTrash, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<PurgeResult>> {
    let days = match req.older_than_days {
        Some(d) => d.min(MAX_RETENTION_DAYS),
        None => retention_days(ctx.reads())?,
    };
    let before = Timestamp(ctx.now.0 - i64::from(days) * MILLIS_PER_DAY);
    // `0` days empties the trash, including what was trashed in this very millisecond.
    let cutoff = if days == 0 { Timestamp(before.0 + 1) } else { before };
    let c = ctx.tx.purge_trash(cutoff)?;
    let total = c.notes + c.cards + c.decks;
    Ok(CommandOutcome {
        response: PurgeResult {
            notes: c.notes,
            cards: c.cards,
            decks: c.decks,
            before,
        },
        inverse: Vec::new(),
        summary: summary::<BrowsePurgeTrash>().param("count", total),
        events: Vec::new(),
    })
}

fn request(payload: Value) -> InvokeRequest {
    InvokeRequest {
        method: format!("v1.{}", BrowsePurgeTrash::NAME),
        params: if payload.is_null() {
            Value::Object(serde_json::Map::new())
        } else {
            payload
        },
    }
}

/// Startup purge with the configured retention (skipped while the module is off).
pub fn purge_on_startup(app: &crate::App) -> AppResult<()> {
    if !app.is_module_enabled(super::ID) {
        return Ok(());
    }
    let r = request(Value::Null);
    app.invoke(&r.method, r.params, &Actor::System)
        .map(|_| ())
        .map_err(crate::error::AppError::Api)
}

/// Job kind `browse.purge_trash`: payload = [`PurgeTrash`] (`{}` = configured retention).
#[derive(Debug, Clone, Copy, Default)]
pub struct PurgeTrashJob;

impl JobHandler for PurgeTrashJob {
    fn kind(&self) -> &'static str {
        BrowsePurgeTrash::NAME
    }

    fn run(&self, payload: Value, ctx: &JobContext<'_>) -> AppResult<Value> {
        let req: PurgeTrash = if payload.is_null() {
            PurgeTrash::default()
        } else {
            serde_json::from_value(payload).map_err(|e| {
                crate::error::AppError::api(fc_api::ErrorCode::ValidationInvalidParams, e.to_string())
            })?
        };
        let result = ctx.invoke::<BrowsePurgeTrash>(&req)?;
        serde_json::to_value(result)
            .map_err(|e| crate::error::AppError::api(fc_api::ErrorCode::Internal, e.to_string()))
    }
}
