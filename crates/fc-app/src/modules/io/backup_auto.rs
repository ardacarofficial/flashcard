//! Automatic backups (v1.4).
//!
//! * Settings: `io.auto_backup` (`off` / `daily` / `weekly`, default `daily`) and
//!   `io.auto_backup_keep` (verified copies kept, default 7). Turning the `io` module off
//!   stops them too.
//! * Where: `<collection folder>/backups/auto/auto-<millis>.fcbackup`. Only a backup that was
//!   restored into a scratch profile and matched gets that name (it is written as `.part`
//!   and renamed after the check), so every file there is a verified copy; a failed run never
//!   touches the existing ones. Rotation runs only after a new verified copy is in place and
//!   removes the oldest beyond `keep`: the newest good copy is never deleted or overwritten.
//! * When: [`tick`] — run at startup and by the job worker while idle — compares the injected
//!   clock with the newest automatic copy and queues the `io.auto_backup` job when one is due
//!   (after a failure, not again for an hour). `io.backup.auto` queues one right away. The
//!   job is cancellable (`jobs.cancel`); a cancelled run leaves nothing behind.
//! * Facts: `<collection folder>/backups/status.json` keeps the last verified backup (any:
//!   automatic, `io.backup.create`, `io.backup.verify`) and the last failure. It lives beside
//!   the files it describes, not in the collection, so a restore does not roll it back.
//!   `io.backup.status` reads it together with the folder.
//! * Mirror (v1.4.1): `io.backup_mirror_dir` — every verified backup is also copied to a
//!   second folder and re-read there ([`super::backup_mirror`]).

use super::backup_file::{write_backup_file, Check, Written};
use super::ID;
use crate::error::{AppError, AppResult};
use crate::handler::{CommandCtx, CommandOutcome};
use crate::jobs::{enqueue, JobContext, JobHandler};
use crate::support::summary;
use fc_api::v1::common::Empty;
use fc_api::v1::io::{
    AutoBackupSchedule, AutoBackupStart, BackupFailure, BackupReport, BackupStale,
    BackupStatus,
    VerifiedBackup,
};
use fc_api::v1::jobs::JobDto;
use fc_api::v1::methods::IoBackupAuto;
use fc_api::v1::oplog::Actor;
use fc_api::ErrorCode;
use fc_core::settings::{SettingDef, SettingKind, SettingScope, SettingScopeKind};
use fc_core::time::MILLIS_PER_DAY;
use fc_core::Timestamp;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};

/// Setting: how often.
pub const SCHEDULE_KEY: &str = "io.auto_backup";
/// Setting: verified automatic copies kept.
pub const KEEP_KEY: &str = "io.auto_backup_keep";
/// Default copies kept.
pub const DEFAULT_KEEP: u32 = 7;
/// Most copies kept.
pub const MAX_KEEP: u32 = 100;
/// Job kind of an automatic backup.
pub const JOB_AUTO_BACKUP: &str = "io.auto_backup";
/// After a failed run, the next one waits this long.
const RETRY_AFTER_MS: i64 = 60 * 60 * 1000;
/// [`tick`] looks at the folder at most this often (the job worker calls it every few
/// seconds while idle).
const TICK_EVERY_MS: i64 = 10 * 60 * 1000;
const PREFIX: &str = "auto-";
const EXT: &str = ".fcbackup";

/// The backup settings.
pub fn settings() -> Vec<SettingDef> {
    vec![
        super::backup_mirror::setting(),
        SettingDef {
            key: SCHEDULE_KEY.to_owned(),
            kind: SettingKind::Choice {
                options: ["off", "daily", "weekly"].map(str::to_owned).to_vec(),
            },
            default: Value::String("daily".into()),
            label_key: format!("settings.{SCHEDULE_KEY}.label"),
            description_key: Some(format!("settings.{SCHEDULE_KEY}.description")),
            advanced: false,
            scopes: vec![SettingScopeKind::App],
        },
        SettingDef {
            key: KEEP_KEY.to_owned(),
            kind: SettingKind::Integer {
                min: Some(1),
                max: Some(i64::from(MAX_KEEP)),
            },
            default: Value::from(DEFAULT_KEEP),
            label_key: format!("settings.{KEEP_KEY}.label"),
            description_key: Some(format!("settings.{KEEP_KEY}.description")),
            advanced: true,
            scopes: vec![SettingScopeKind::App],
        },
    ]
}

fn schedule(reads: &dyn fc_storage::Reads) -> AppResult<AutoBackupSchedule> {
    let stored = crate::settings::resolve_stored(reads, &SettingScope::App, SCHEDULE_KEY)?;
    Ok(
        match stored.as_ref().and_then(|(v, _)| v.as_str()) {
            Some("off") => AutoBackupSchedule::Off,
            Some("weekly") => AutoBackupSchedule::Weekly,
            _ => AutoBackupSchedule::Daily,
        },
    )
}

pub(crate) fn keep(reads: &dyn fc_storage::Reads) -> AppResult<u32> {
    let stored = crate::settings::resolve_stored(reads, &SettingScope::App, KEEP_KEY)?;
    Ok(stored
        .and_then(|(v, _)| v.as_u64())
        .and_then(|n| u32::try_from(n).ok())
        .unwrap_or(DEFAULT_KEEP)
        .clamp(1, MAX_KEEP))
}

fn period_ms(s: AutoBackupSchedule) -> Option<i64> {
    match s {
        AutoBackupSchedule::Off => None,
        AutoBackupSchedule::Daily => Some(MILLIS_PER_DAY),
        AutoBackupSchedule::Weekly => Some(7 * MILLIS_PER_DAY),
    }
}

/// `<data dir>/backups`: safety backups, scratch profiles, `status.json`.
pub(crate) fn backups_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("backups")
}

/// `<data dir>/backups/auto`.
pub(crate) fn auto_dir(data_dir: &Path) -> PathBuf {
    backups_dir(data_dir).join("auto")
}

// ------------------------------------------------------------------ status.json

/// `backups/status.json`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct StatusFile {
    pub last_verified: Option<VerifiedBackup>,
    pub last_failure: Option<BackupFailure>,
    /// The last copy to the mirror folder that failed (cleared by the next good one).
    #[serde(default)]
    pub mirror_failure: Option<BackupFailure>,
    /// Whether that failure was a full disk.
    #[serde(default)]
    pub mirror_disk_full: bool,
    /// The mirror folder last copied to: kept beside the backups (not only in the
    /// collection's settings) so the rescue of a damaged collection finds it too.
    #[serde(default)]
    pub mirror_dir: Option<String>,
}

fn status_path(data_dir: &Path) -> PathBuf {
    backups_dir(data_dir).join("status.json")
}

/// Missing or unreadable: nothing known.
pub(crate) fn load_status(data_dir: &Path) -> StatusFile {
    std::fs::read(status_path(data_dir))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

pub(crate) fn update_status(data_dir: &Path, f: impl FnOnce(&mut StatusFile)) {
    let mut s = load_status(data_dir);
    f(&mut s);
    let path = status_path(data_dir);
    if let (Some(dir), Ok(bytes)) = (path.parent(), serde_json::to_vec_pretty(&s)) {
        let _ = std::fs::create_dir_all(dir);
        // Best effort: the backups themselves are the truth, this is a summary of them.
        let _ = super::write_file(&path.to_string_lossy(), &bytes);
    }
}

/// Path of the last verified backup recorded in `status.json` (the rescue of a damaged
/// collection looks there too, v1.4.1).
pub(crate) fn last_verified_path(data_dir: &Path) -> Option<String> {
    load_status(data_dir).last_verified.map(|v| v.path)
}

/// The mirror folder recorded in `status.json` (the rescue looks for backups there too).
pub(crate) fn last_mirror_dir(data_dir: &Path) -> Option<PathBuf> {
    load_status(data_dir).mirror_dir.map(PathBuf::from)
}

/// Records a verified backup as the last one (best effort; no folder: nothing to record).
pub(crate) fn record_verified(data_dir: Option<&Path>, backup: VerifiedBackup) {
    if let Some(d) = data_dir {
        update_status(d, |s| {
            if backup.automatic {
                s.last_failure = None;
            }
            s.last_verified = Some(backup);
        });
    }
}

fn record_failure(data_dir: &Path, at: Timestamp, message: String) {
    update_status(data_dir, |s| s.last_failure = Some(BackupFailure { at, message }));
}

/// The report of a written, verified backup.
pub(crate) fn backup_report(path: &Path, w: &Written) -> BackupReport {
    let count = |t: &str| {
        u32::try_from(w.manifest.counts.get(t).copied().unwrap_or(0)).unwrap_or(u32::MAX)
    };
    BackupReport {
        path: path.to_string_lossy().into_owned(),
        bytes: w.bytes,
        notes: count("notes"),
        cards: count("cards"),
        review_log: count("review_log"),
        media: u32::try_from(w.manifest.media).unwrap_or(u32::MAX),
        verified: w.verified.as_ref().is_some_and(|r| r.ok),
        missing_media: w.manifest.missing_media.clone(),
        mirror: None,
    }
}

/// The [`VerifiedBackup`] of a written, verified backup.
pub(crate) fn verified_backup(
    path: &Path,
    w: &Written,
    now: Timestamp,
    automatic: bool,
) -> VerifiedBackup {
    VerifiedBackup {
        path: path.to_string_lossy().into_owned(),
        created_at: w.manifest.created_at,
        verified_at: now,
        bytes: w.bytes,
        complete: w.manifest.is_complete(),
        automatic,
    }
}

// ------------------------------------------------------------------ the folder

/// Verified automatic copies in `dir`, newest first: `(taken at, path)`.
fn list_auto(dir: &Path) -> Vec<(i64, PathBuf)> {
    list_copies(dir, PREFIX)
}

/// Verified copies named `<prefix><millis>[-n].fcbackup` in `dir`, newest first.
pub(crate) fn list_copies(dir: &Path, prefix: &str) -> Vec<(i64, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<(i64, PathBuf)> = entries
        .filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            let stem = name.strip_prefix(prefix)?.strip_suffix(EXT)?;
            let ms = stem.split('-').next()?.parse::<i64>().ok()?;
            Some((ms, e.path()))
        })
        .collect();
    out.sort_by(|a, b| b.cmp(a));
    out
}

/// A name for a copy taken at `now` that no existing file has.
fn fresh_name(dir: &Path, now: Timestamp) -> PathBuf {
    fresh_copy_name(dir, PREFIX, now)
}

/// `<prefix><millis>[-n].fcbackup` in `dir` that no existing file (or `.part`) has.
pub(crate) fn fresh_copy_name(dir: &Path, prefix: &str, now: Timestamp) -> PathBuf {
    let mut path = dir.join(format!("{prefix}{}{EXT}", now.0));
    let mut n = 1;
    while path.exists() || super::backup_file::part_path(&path).exists() {
        path = dir.join(format!("{prefix}{}-{n}{EXT}", now.0));
        n += 1;
    }
    path
}

/// Removes leftovers of interrupted runs (`auto-*.part`; only one automatic run at a time).
fn remove_stale_parts(dir: &Path) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for e in entries.filter_map(Result::ok) {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with(PREFIX) && name.ends_with(".part") {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
}

/// Keeps the newest `keep` copies (never fewer than one).
fn rotate(dir: &Path, keep: u32) {
    rotate_copies(dir, PREFIX, keep);
}

/// Keeps the newest `keep` copies named `<prefix>…` in `dir` (never fewer than one).
pub(crate) fn rotate_copies(dir: &Path, prefix: &str, keep: u32) {
    let keep = usize::try_from(keep.max(1)).unwrap_or(usize::MAX);
    for (_, path) in list_copies(dir, prefix).into_iter().skip(keep) {
        let _ = std::fs::remove_file(path);
    }
}

pub(crate) fn copy_info(path: &Path, taken: i64) -> VerifiedBackup {
    let bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let manifest = super::backup_file::open_backup(path)
        .ok()
        .map(|r| r.manifest().clone());
    VerifiedBackup {
        path: path.to_string_lossy().into_owned(),
        created_at: manifest.as_ref().map_or(Timestamp(taken), |m| m.created_at),
        verified_at: Timestamp(taken),
        bytes,
        complete: manifest.as_ref().is_none_or(|m| m.is_complete()),
        automatic: true,
    }
}

/// When the next automatic backup is due (`None`: off).
fn next_due(s: AutoBackupSchedule, dir: &Path, now: Timestamp) -> Option<Timestamp> {
    let period = period_ms(s)?;
    Some(match list_auto(dir).first() {
        Some((taken, _)) => Timestamp(taken.saturating_add(period)),
        None => now,
    })
}

/// Longest a verified backup may be before Home asks for a new one when automatic backups
/// are off.
const STALE_WHEN_OFF_MS: i64 = 7 * MILLIS_PER_DAY;

/// Whether the backup needs attention (`io.backup.status.stale`): a failure first, then no
/// backup at all, then a backup older than two schedule periods (7 days when off).
pub(crate) fn staleness(
    sched: AutoBackupSchedule,
    last_verified: Option<&VerifiedBackup>,
    failed: bool,
    now: Timestamp,
) -> Option<BackupStale> {
    if failed {
        return Some(BackupStale::Failed);
    }
    let Some(last) = last_verified else {
        return Some(BackupStale::Never);
    };
    let limit = period_ms(sched).map_or(STALE_WHEN_OFF_MS, |p| p.saturating_mul(2));
    (now.0.saturating_sub(last.verified_at.0) > limit).then_some(BackupStale::Old)
}

// ------------------------------------------------------------------ API

/// `io.backup.status` (an action: it needs the collection folder).
pub(super) fn status(_: Empty, app: &crate::App, _actor: &Actor) -> AppResult<BackupStatus> {
    let (sched, keep) = {
        let tx = app.store().read()?;
        (schedule(&*tx)?, keep(&*tx)?)
    };
    let Some(data_dir) = app.data_dir() else {
        return Ok(BackupStatus {
            schedule: sched,
            keep,
            folder: None,
            last_verified: None,
            automatic: Vec::new(),
            next_due: None,
            last_failure: None,
            stale: None,
            mirror: {
                let tx = app.store().read()?;
                super::backup_mirror::status(&*tx, None)?
            },
        });
    };
    let dir = auto_dir(data_dir);
    let file = load_status(data_dir);
    let automatic: Vec<VerifiedBackup> = list_auto(&dir)
        .into_iter()
        .map(|(taken, path)| copy_info(&path, taken))
        .collect();
    // The newest of what status.json remembers and what the folder holds (the folder wins if
    // status.json was lost).
    let last_verified = match (file.last_verified, automatic.first().cloned()) {
        (Some(a), Some(b)) => Some(if b.verified_at.0 > a.verified_at.0 { b } else { a }),
        (a, b) => a.or(b),
    };
    let now = app.now();
    let stale = staleness(sched, last_verified.as_ref(), file.last_failure.is_some(), now);
    let mirror = {
        let tx = app.store().read()?;
        super::backup_mirror::status(&*tx, Some(data_dir))?
    };
    Ok(BackupStatus {
        schedule: sched,
        keep,
        folder: Some(dir.to_string_lossy().into_owned()),
        last_verified,
        automatic,
        next_due: next_due(sched, &dir, now),
        last_failure: file.last_failure,
        stale,
        mirror,
    })
}

/// Whether a job of `kind` is queued or running.
fn job_pending(reads: &dyn fc_storage::Reads, kind: &str) -> AppResult<bool> {
    Ok(reads
        .list_jobs(200)?
        .iter()
        .any(|j| j.kind == kind && (j.status == "queued" || j.status == "running")))
}

/// `io.backup.auto`.
pub(super) fn start(_: Empty, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<AutoBackupStart>> {
    let job = if job_pending(ctx.reads(), JOB_AUTO_BACKUP)? {
        None
    } else {
        Some(enqueue(ctx, JOB_AUTO_BACKUP, Value::Object(Default::default()))?)
    };
    Ok(CommandOutcome {
        response: AutoBackupStart { job },
        inverse: Vec::new(),
        summary: summary::<IoBackupAuto>(),
        events: Vec::new(),
    })
}

/// When [`tick`] last looked (process-wide; the app clock's time).
static LAST_TICK: AtomicI64 = AtomicI64::new(i64::MIN);

/// Queues an automatic backup if one is due (see the module docs). At most one look every
/// ten minutes of the app clock unless `force`. Returns the queued job.
pub fn tick(app: &crate::App, force: bool) -> AppResult<Option<JobDto>> {
    let now = app.now();
    if !force {
        let last = LAST_TICK.load(Ordering::SeqCst);
        if last != i64::MIN && now.0.saturating_sub(last) < TICK_EVERY_MS && now.0 >= last {
            return Ok(None);
        }
    }
    LAST_TICK.store(now.0, Ordering::SeqCst);
    if !app.is_module_enabled(ID) {
        return Ok(None);
    }
    let Some(data_dir) = app.data_dir() else {
        return Ok(None);
    };
    let sched = {
        let tx = app.store().read()?;
        schedule(&*tx)?
    };
    let Some(due) = next_due(sched, &auto_dir(data_dir), now) else {
        return Ok(None);
    };
    if now.0 < due.0 {
        return Ok(None);
    }
    if let Some(f) = load_status(data_dir).last_failure {
        if now.0 >= f.at.0 && now.0 - f.at.0 < RETRY_AFTER_MS {
            return Ok(None);
        }
    }
    let started = app
        .call::<IoBackupAuto>(&Empty {}, &Actor::System)
        .map_err(AppError::Api)?;
    Ok(started.job)
}

/// The `io.auto_backup` job.
pub(crate) struct AutoBackupJob;

impl JobHandler for AutoBackupJob {
    fn kind(&self) -> &'static str {
        JOB_AUTO_BACKUP
    }

    fn run(&self, _payload: Value, ctx: &JobContext<'_>) -> AppResult<Value> {
        ctx.ensure_module_enabled(ID)?;
        let Some(data_dir) = ctx.data_dir().map(Path::to_path_buf) else {
            return Err(AppError::api(
                ErrorCode::ValidationInvalidParams,
                "the collection has no folder for automatic backups",
            ));
        };
        let dir = auto_dir(&data_dir);
        let now = ctx.now();
        let run = || -> AppResult<(PathBuf, Written, u32)> {
            std::fs::create_dir_all(&dir)
                .map_err(|e| super::file_err(&dir.to_string_lossy(), e))?;
            remove_stale_parts(&dir);
            let path = fresh_name(&dir, now);
            ctx.report(Some(0.1), "io.progress.collecting");
            let keep = {
                let tx = ctx.read()?;
                keep(&*tx)?
            };
            let written = write_backup_file(
                &|| ctx.read(),
                now,
                &path,
                &ctx.cancel,
                Check::Full,
                &backups_dir(&data_dir),
            )?;
            Ok((path, written, keep))
        };
        let (path, written, keep) = match run() {
            Ok(done) => done,
            Err(e) => {
                let api = e.into_api();
                if !ctx.is_cancelled() && api.code != ErrorCode::ConflictJobCancelled {
                    record_failure(&data_dir, ctx.now(), api.message.clone());
                }
                return Err(AppError::Api(api));
            }
        };
        record_verified(
            Some(&data_dir),
            verified_backup(&path, &written, ctx.now(), true),
        );
        rotate(&dir, keep);
        let mut report = backup_report(&path, &written);
        report.mirror = super::backup_mirror::mirror_backup(ctx, Some(&data_dir), &path, true);
        serde_json::to_value(report)
            .map_err(|e| AppError::api(ErrorCode::Internal, e.to_string()))
    }
}
