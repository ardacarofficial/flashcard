//! Full backup (`.fcbackup`) and restore.
//!
//! A backup is the storage's table dump plus every media file (read through
//! [`super::media::MediaStore`]), streamed file to file ([`super::backup_file`]): written as
//! `.part`, restored into a scratch profile and compared, then renamed into place
//! (`io.backup.create`; `io.backup.verify` runs the same check on any file). A restore
//!
//! 1. reads the backup entry by entry: parses the dump, checks that every media file the
//!    manifest lists is there and matches its hash (nothing held in memory but the dump),
//!    and brings the dump to this build's schema in a private database
//!    ([`fc_storage::upgrade_dump`]: later data migrations run on the restored rows), all
//!    without holding the writer;
//! 2. writes a backup of the current state (docs/02 §16) into the app data folder
//!    (`<collection folder>/backups/`; the backup's own folder may be read-only), re-read and
//!    hash-checked before the restore goes on;
//! 3. swaps every table and the media in one transaction (`io.backup.load`) — deliberately
//!    atomic: a half-restored collection must never be visible to other commands. Media are
//!    streamed from the backup file one at a time (each re-verified by the media store).
//!
//! It is not an undo step — the safety backup is the way back.

use super::backup_auto::{backup_report, backups_dir, record_verified, verified_backup};
use super::backup_file::{open_backup, verify_file, write_backup_file, Check};
use super::{file_err, interop_err, Staging, JOB_BACKUP, JOB_RESTORE, JOB_VERIFY};
use crate::error::{AppError, AppResult};
use crate::handler::{CommandCtx, CommandHandler, CommandOutcome, QueryCtx};
use crate::jobs::{enqueue, JobContext, JobHandler};
use crate::support::history::{push_barrier, BarrierReason};
use crate::support::{invalid, summary};
use fc_api::v1::common::Empty;
use fc_api::v1::events::DomainEvent;
use fc_api::v1::io::*;
use fc_api::v1::jobs::JobDto;
use fc_api::v1::methods::{IoBackupCreate, IoBackupLoad, IoBackupRestore, IoBackupVerify};
use fc_api::{ApiError, ErrorCode};
use super::media::media_store;
use fc_interop::fcbackup::{self, MediaEntry};
use fc_storage::repo::{CollectionDump, MediaRecord};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// A restore ready to swap in: the dump at this build's schema and the backup's media
/// (streamed from `path` by `io.backup.load`).
pub(crate) struct Restore {
    pub dump: CollectionDump,
    pub path: PathBuf,
    pub media: Vec<MediaEntry>,
}

fn internal(e: impl ToString) -> AppError {
    AppError::api(ErrorCode::Internal, e.to_string())
}

/// `io.backup.create`.
pub(super) fn create(
    req: CreateBackup,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<JobDto>> {
    if req.path.trim().is_empty() {
        return Err(invalid("path is empty"));
    }
    let job = enqueue(ctx, JOB_BACKUP, serde_json::to_value(&req).map_err(internal)?)?;
    Ok(CommandOutcome {
        response: job,
        inverse: Vec::new(),
        summary: summary::<IoBackupCreate>(),
        events: Vec::new(),
    })
}

/// `io.backup.restore`.
pub(super) fn restore(
    req: RestoreBackup,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<JobDto>> {
    if req.path.trim().is_empty() {
        return Err(invalid("path is empty"));
    }
    let job = enqueue(ctx, JOB_RESTORE, serde_json::to_value(&req).map_err(internal)?)?;
    Ok(CommandOutcome {
        response: job,
        inverse: Vec::new(),
        summary: summary::<IoBackupRestore>(),
        events: Vec::new(),
    })
}

pub(crate) fn row_counts(dump: &CollectionDump) -> std::collections::BTreeMap<String, u64> {
    dump.tables
        .iter()
        .map(|t| (t.name.clone(), t.rows.len() as u64))
        .collect()
}

/// Whether this build knows every migration the backup had.
pub(crate) fn compatible(versions: &[u32]) -> bool {
    versions
        .iter()
        .all(|v| fc_storage::migrate::MIGRATIONS.iter().any(|m| m.version == *v))
}

/// A backup is data from outside: registered MCP servers that come with it must not launch
/// programs until the user approves them again here (re-save in the MCP screen).
fn forget_mcp_approvals(dump: &mut CollectionDump) {
    for t in dump.tables.iter_mut().filter(|t| t.name == "mcp_servers") {
        if let Some(i) = t.columns.iter().position(|c| c == "approved") {
            for row in &mut t.rows {
                row[i] = fc_storage::repo::SqlValue::Null;
            }
        }
    }
}

/// …nor may settings that name a program the app runs (`crate::settings::EXEC_SETTINGS`,
/// e.g. `reading.ffmpeg_path`): a backup could otherwise point them at any program. They go
/// back to their defaults; the user sets them again.
fn forget_exec_settings(dump: &mut CollectionDump) {
    for t in dump.tables.iter_mut().filter(|t| t.name == "settings") {
        if let Some(i) = t.columns.iter().position(|c| c == "key") {
            t.rows.retain(|row| {
                !matches!(&row[i], fc_storage::repo::SqlValue::Text(k)
                    if crate::settings::is_exec_setting(k))
            });
        }
    }
}

/// `io.backup.verify`.
pub(super) fn verify(
    req: VerifyBackup,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<JobDto>> {
    if req.path.trim().is_empty() {
        return Err(invalid("path is empty"));
    }
    let job = enqueue(ctx, JOB_VERIFY, serde_json::to_value(&req).map_err(internal)?)?;
    Ok(CommandOutcome {
        response: job,
        inverse: Vec::new(),
        summary: summary::<IoBackupVerify>(),
        events: Vec::new(),
    })
}

/// Where scratch profiles and safety backups go: the app data folder, else beside `target`.
fn scratch_dir(ctx: &JobContext<'_>, target: &Path) -> PathBuf {
    ctx.data_dir()
        .map(backups_dir)
        .or_else(|| target.parent().map(Path::to_path_buf))
        .unwrap_or_else(std::env::temp_dir)
}

/// `io.backup.inspect`.
pub(super) fn inspect(req: InspectBackup, _ctx: &QueryCtx<'_>) -> AppResult<BackupInfo> {
    let m = open_backup(Path::new(&req.path))?.manifest().clone();
    Ok(BackupInfo {
        created_at: m.created_at,
        compatible: compatible(&m.schema_versions),
        complete: m.is_complete(),
        counts: m.counts,
        media: u32::try_from(m.media).unwrap_or(u32::MAX),
    })
}

/// The `io.backup` job.
pub(crate) struct BackupJob;

impl JobHandler for BackupJob {
    fn kind(&self) -> &'static str {
        JOB_BACKUP
    }

    fn run(&self, payload: Value, ctx: &JobContext<'_>) -> AppResult<Value> {
        let req: CreateBackup = serde_json::from_value(payload).map_err(internal)?;
        ctx.report(Some(0.1), "io.progress.collecting");
        let path = PathBuf::from(&req.path);
        let written = write_backup_file(
            &|| ctx.read(),
            ctx.now(),
            &path,
            &ctx.cancel,
            Check::Full,
            &scratch_dir(ctx, &path),
        )?;
        record_verified(
            ctx.data_dir(),
            verified_backup(&path, &written, ctx.now(), false),
        );
        serde_json::to_value(backup_report(&path, &written)).map_err(internal)
    }
}

/// The `io.verify` job.
pub(crate) struct VerifyJob;

impl JobHandler for VerifyJob {
    fn kind(&self) -> &'static str {
        JOB_VERIFY
    }

    fn run(&self, payload: Value, ctx: &JobContext<'_>) -> AppResult<Value> {
        let req: VerifyBackup = serde_json::from_value(payload).map_err(internal)?;
        ctx.report(None, "io.progress.reading");
        let path = PathBuf::from(&req.path);
        let report = verify_file(&path, &scratch_dir(ctx, &path), &ctx.cancel)?;
        if report.ok {
            if let Some(created_at) = report.created_at {
                record_verified(
                    ctx.data_dir(),
                    VerifiedBackup {
                        path: req.path.clone(),
                        created_at,
                        verified_at: ctx.now(),
                        bytes: std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0),
                        complete: report.complete,
                        automatic: false,
                    },
                );
            }
        }
        serde_json::to_value(report).map_err(internal)
    }
}

/// The `io.restore` job.
pub(crate) struct RestoreJob(pub Arc<Staging>);

impl JobHandler for RestoreJob {
    fn kind(&self) -> &'static str {
        JOB_RESTORE
    }

    fn run(&self, payload: Value, ctx: &JobContext<'_>) -> AppResult<Value> {
        let req: RestoreBackup = serde_json::from_value(payload).map_err(internal)?;
        ctx.report(Some(0.1), "io.progress.reading");
        let path = PathBuf::from(&req.path);
        let mut reader = open_backup(&path)?;
        let manifest = reader.manifest().clone();
        if !compatible(&manifest.schema_versions) {
            return Err(AppError::Api(
                ApiError::new(
                    ErrorCode::ConflictUnsupportedVersion,
                    "the backup comes from a newer version",
                )
                .with_detail("format", fcbackup::FORMAT),
            ));
        }
        let dump: CollectionDump = reader.collection().map_err(interop_err)?;
        // Every listed media file is there and matches, before anything is replaced.
        let media = reader.media_list();
        for (i, m) in media.iter().enumerate() {
            ctx.cancel.check()?;
            if i % 64 == 0 {
                let p = 0.1 + 0.1 * (i as f32 / media.len().max(1) as f32);
                ctx.report(Some(p), "io.progress.reading");
            }
            reader.check_media(m).map_err(interop_err)?;
        }
        drop(reader);
        ctx.cancel.check()?;
        // Older schema: migrate the restored rows forward (outside the writer).
        let mut dump = fc_storage::upgrade_dump(dump)?;
        forget_mcp_approvals(&mut dump);
        forget_exec_settings(&mut dump);
        crate::modules::local_api::forget_clients(&mut dump);
        crate::modules::subtitle::forget_keys(&mut dump);
        ctx.cancel.check()?;

        // Safety backup of the current state first, into the app data folder.
        ctx.report(Some(0.3), "io.progress.safety_backup");
        let safety = match req.safety_backup_path.clone() {
            Some(p) => p,
            None => {
                let dir = ctx
                    .data_dir()
                    .map(|d| d.join("backups"))
                    .unwrap_or_else(std::env::temp_dir);
                std::fs::create_dir_all(&dir)
                    .map_err(|e| file_err(&dir.to_string_lossy(), e))?;
                dir.join(format!("pre-restore-{}.fcbackup", ctx.now().0))
                    .to_string_lossy()
                    .into_owned()
            }
        };
        let safety_path = PathBuf::from(&safety);
        write_backup_file(
            &|| ctx.read(),
            ctx.now(),
            &safety_path,
            &ctx.cancel,
            Check::Archive,
            &scratch_dir(ctx, &safety_path),
        )?;
        if !safety_path.exists() {
            return Err(file_err(
                &safety,
                std::io::Error::new(std::io::ErrorKind::NotFound, "safety backup missing"),
            ));
        }
        ctx.cancel.check()?;

        ctx.report(None, "io.progress.writing");
        let counts = row_counts(&dump);
        let restored = u32::try_from(media.len()).unwrap_or(u32::MAX);
        self.0.put_restore(
            ctx.job_id,
            Restore {
                dump,
                path,
                media,
            },
        );
        let result = ctx.invoke::<IoBackupLoad>(&StagedJob { job_id: ctx.job_id });
        let _ = self.0.take_restore(ctx.job_id);
        result?;
        serde_json::to_value(RestoreReport {
            safety_backup_path: safety,
            counts,
            media: restored,
            missing_media: manifest.missing_media,
        })
        .map_err(internal)
    }
}

/// Handler of `io.backup.load`: replaces every table and the media, then starts a fresh
/// history (empty oplog + a restore barrier).
pub(crate) struct LoadHandler(pub Arc<Staging>);

impl CommandHandler<IoBackupLoad> for LoadHandler {
    fn execute(&self, req: StagedJob, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<Empty>> {
        let Restore { dump, path, media } = self.0.take_restore(req.job_id)?;
        ctx.tx.load_collection(&dump)?;
        drop(dump);
        let store = media_store();
        let mut reader = open_backup(&path)?;
        for m in &media {
            let bytes = reader.read_media(m).map_err(interop_err)?;
            let (hash, bytes) = (&m.hash, &bytes);
            let record = store.record(ctx.reads(), hash)?.unwrap_or(MediaRecord {
                hash: hash.clone(),
                mime: "application/octet-stream".into(),
                size: bytes.len() as u64,
                original_name: None,
                ref_count: 0,
                created_at: ctx.now,
            });
            store.put(&mut *ctx.tx, &record, bytes)?;
        }
        // Jobs that were running when the backup was taken have no worker now.
        ctx.tx.mark_unfinished_jobs_interrupted(ctx.now)?;
        // History starts afresh: the backup's oplog is never replayed (its inverses are
        // untrusted data and may refer to rows that are gone), and undo stops at a barrier.
        ctx.tx.clear_oplog()?;
        push_barrier(ctx, BarrierReason::Restore);
        Ok(CommandOutcome {
            response: Empty {},
            inverse: Vec::new(),
            summary: summary::<IoBackupLoad>(),
            events: vec![DomainEvent::CollectionReplaced],
        })
    }
}
