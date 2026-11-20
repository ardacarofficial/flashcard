//! Recovery offer, integrity check, status, crash report cleanup.

use super::crash::{self, ReportFile};
use super::{logging, MaintenanceServices, APP_VERSION, CRASH_DIR};
use crate::error::{AppError, AppResult};
use crate::handler::{CommandCtx, CommandOutcome, QueryCtx};
use crate::registry::Registry;
use crate::App;
use fc_api::v1::common::Empty;
use fc_api::v1::maintenance::*;
use fc_api::ErrorCode;
use fc_core::maintenance::recovery::{integrity_problems, reason};
use fc_core::settings::SettingScope;
use fc_core::Timestamp;
use fc_storage::Reads;
use std::sync::atomic::Ordering;
use std::sync::Arc;

/// Problem rows asked from SQLite.
const MAX_CHECK_ROWS: u32 = 100;
/// Problems kept in the verdict.
const MAX_PROBLEMS: usize = 20;

fn services(registry: &Registry) -> AppResult<Arc<MaintenanceServices>> {
    registry
        .extension::<MaintenanceServices>()
        .ok_or_else(|| AppError::api(ErrorCode::Internal, "maintenance services missing"))
}

pub(crate) fn crash_dto(r: ReportFile) -> CrashReportDto {
    CrashReportDto {
        file: r.file,
        version: r.header.version,
        time: r.header.time,
        thread: r.header.thread,
        location: r.header.location,
        message: r.header.message,
    }
}

fn stored(app: &App, key: &str) -> Option<serde_json::Value> {
    let tx = app.store().read().ok()?;
    crate::settings::resolve_stored(&*tx, &SettingScope::App, key)
        .ok()
        .flatten()
        .map(|(v, _)| v)
}

pub(crate) fn setting_bool(app: &App, key: &str, default: bool) -> bool {
    stored(app, key).and_then(|v| v.as_bool()).unwrap_or(default)
}

pub(crate) fn setting_u64(app: &App, key: &str) -> Option<u64> {
    stored(app, key).and_then(|v| v.as_u64())
}

pub(crate) fn setting_str(app: &App, key: &str) -> Option<String> {
    stored(app, key).and_then(|v| v.as_str().map(str::to_owned))
}

fn check(reads: &dyn Reads, quick: bool, now: Timestamp) -> AppResult<IntegrityDto> {
    let started = std::time::Instant::now();
    let record = reads.integrity_check(quick, MAX_CHECK_ROWS)?;
    let problems = integrity_problems(&record.rows, record.foreign_key_violations, MAX_PROBLEMS);
    Ok(IntegrityDto {
        ok: problems.is_empty(),
        quick,
        problems,
        checked_at: now,
        duration_ms: u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX),
    })
}

/// Runs the integrity check outside a handler (start) and records the verdict.
pub(crate) fn run_integrity_check(app: &App, quick: bool) -> Result<IntegrityDto, fc_api::ApiError> {
    let tx = app.store().read().map_err(|e| AppError::from(e).into_api())?;
    let dto = check(&*tx, quick, app.now()).map_err(AppError::into_api)?;
    if let Some(s) = app.maintenance() {
        s.set_integrity(dto.clone());
    }
    Ok(dto)
}

pub(super) fn integrity_check(req: CheckIntegrity, ctx: &QueryCtx<'_>) -> AppResult<IntegrityDto> {
    let quick = req.quick.unwrap_or(false);
    let dto = check(ctx.tx, quick, ctx.now)?;
    services(ctx.registry)?.set_integrity(dto.clone());
    if !dto.ok {
        log::error!(target: "fc_app::maintenance", "integrity check found problems: {:?}", dto.problems);
    }
    Ok(dto)
}

fn offer(reads: &dyn Reads, svc: &MaintenanceServices) -> AppResult<RecoveryDto> {
    let state = svc.read();
    let previous_run = svc.previous_run();
    let unclean = previous_run.as_ref().is_some_and(|p| !p.clean);
    let why = if svc.dismissed.load(Ordering::SeqCst) {
        None
    } else {
        reason(unclean, state.crashes.len())
    };
    if why.is_none() {
        return Ok(RecoveryDto {
            reason: None,
            previous_run,
            crash: None,
            integrity: None,
            session: None,
            drafts: Vec::new(),
        });
    }
    let session = reads
        .latest_open_session()?
        .map(|s| RecoverableSession {
            id: s.id,
            kind: s.kind,
            started_at: s.started_at,
        });
    let drafts = reads
        .list_drafts()?
        .into_iter()
        .map(super::drafts::dto)
        .collect();
    Ok(RecoveryDto {
        reason: why,
        previous_run,
        crash: state.crashes.first().cloned(),
        integrity: state.integrity.clone(),
        session,
        drafts,
    })
}

pub(super) fn recovery(_: Empty, ctx: &QueryCtx<'_>) -> AppResult<RecoveryDto> {
    offer(ctx.tx, &*services(ctx.registry)?)
}

pub(super) fn dismiss(_: Empty, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<RecoveryDto>> {
    let svc = services(ctx.registry)?;
    svc.dismissed.store(true, Ordering::SeqCst);
    log::info!(target: "fc_app::maintenance", "recovery offer dismissed");
    let dto = offer(ctx.reads(), &svc)?;
    Ok(super::done::<fc_api::v1::methods::MaintenanceRecoveryDismiss, _>(dto))
}

pub(super) fn status(_: Empty, ctx: &QueryCtx<'_>) -> AppResult<MaintenanceStatus> {
    let svc = services(ctx.registry)?;
    let state = svc.read();
    let crash_reports = state
        .data_dir
        .as_ref()
        .map(|d| {
            crash::list_reports(&d.join(CRASH_DIR))
                .into_iter()
                .map(crash_dto)
                .collect()
        })
        .unwrap_or_default();
    let logs = state
        .log
        .as_ref()
        .map(|l| {
            l.files()
                .into_iter()
                .map(|(name, bytes)| LogFileDto { name, bytes })
                .collect()
        })
        .unwrap_or_default();
    Ok(MaintenanceStatus {
        version: APP_VERSION.to_owned(),
        data_dir: state.data_dir.as_ref().map(|d| d.display().to_string()),
        log_dir: state.log.as_ref().map(|l| l.dir().display().to_string()),
        logs,
        crash_reports,
        previous_run: svc.previous_run(),
        integrity: state.integrity.clone(),
    })
}

pub(super) fn clear_crash_reports(
    _: Empty,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<ClearedCount>> {
    let removed = services(ctx.registry)?
        .crash_dir()
        .map_or(0, |dir| crash::prune(&dir, 0));
    log::info!(target: "fc_app::maintenance", "{removed} crash report(s) removed");
    Ok(super::done::<fc_api::v1::methods::MaintenanceCrashReportsClear, _>(ClearedCount { removed }))
}

/// Re-exported for the shell (log level names).
pub use logging::parse_level;
