//! Full backup (`.fcbackup`) and restore.
//!
//! A backup is the storage's table dump plus every media file (read through
//! [`super::media::MediaStore`]). A restore
//!
//! 1. reads and checks the backup and brings its dump to this build's schema in a private
//!    database ([`fc_storage::upgrade_dump`]: later data migrations run on the restored rows),
//!    all without holding the writer;
//! 2. writes a backup of the current state (docs/02 §16) into the app data folder
//!    (`<collection folder>/backups/`; the backup's own folder may be read-only);
//! 3. swaps every table and the media in one transaction (`io.backup.load`) — deliberately
//!    atomic: a half-restored collection must never be visible to other commands. Only
//!    prepared bulk inserts run inside it.
//!
//! It is not an undo step — the safety backup is the way back.

use super::{file_err, interop_err, read_file, write_file, Staging, JOB_BACKUP, JOB_RESTORE};
use crate::error::{AppError, AppResult};
use crate::handler::{CommandCtx, CommandHandler, CommandOutcome, QueryCtx};
use crate::jobs::{enqueue, JobContext, JobHandler};
use crate::support::history::{push_barrier, BarrierReason};
use crate::support::{invalid, summary};
use fc_api::v1::common::Empty;
use fc_api::v1::events::DomainEvent;
use fc_api::v1::io::*;
use fc_api::v1::jobs::JobDto;
use fc_api::v1::methods::{IoBackupCreate, IoBackupLoad, IoBackupRestore};
use fc_api::{ApiError, ErrorCode};
use super::media::media_store;
use fc_interop::fcbackup::{self, BackupWriter, Manifest};
use fc_storage::repo::{CollectionDump, MediaRecord};
use fc_storage::Reads;
use serde_json::Value;
use std::path::Path;
use std::sync::Arc;

/// A restore ready to swap in: the dump at this build's schema and the media files.
pub(crate) struct Restore {
    pub dump: CollectionDump,
    pub media: Vec<(String, Vec<u8>)>,
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

fn row_counts(dump: &CollectionDump) -> std::collections::BTreeMap<String, u64> {
    dump.tables
        .iter()
        .map(|t| (t.name.clone(), t.rows.len() as u64))
        .collect()
}

/// Whether this build knows every migration the backup had.
fn compatible(versions: &[u32]) -> bool {
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

/// A complete backup of what `reads` sees.
fn backup_bytes(reads: &dyn Reads, now: fc_core::Timestamp) -> AppResult<(Vec<u8>, Manifest)> {
    let dump = reads.dump_collection()?;
    let store = media_store();
    let media = store.list(reads)?;
    let manifest = Manifest {
        format: fcbackup::FORMAT.into(),
        version: fcbackup::VERSION,
        created_at: now,
        schema_versions: dump.schema_versions.clone(),
        counts: row_counts(&dump),
        media: media.len(),
    };
    let json = serde_json::to_vec(&dump).map_err(internal)?;
    let mut w = BackupWriter::new(&manifest, &json).map_err(interop_err)?;
    for m in &media {
        if let Some(bytes) = store.bytes(reads, &m.hash)? {
            w.add_media(&m.hash, &bytes).map_err(interop_err)?;
        }
    }
    Ok((w.finish().map_err(interop_err)?, manifest))
}

/// `io.backup.inspect`.
pub(super) fn inspect(req: InspectBackup, _ctx: &QueryCtx<'_>) -> AppResult<BackupInfo> {
    let m = fcbackup::read_manifest(&read_file(&req.path)?).map_err(interop_err)?;
    Ok(BackupInfo {
        created_at: m.created_at,
        compatible: compatible(&m.schema_versions),
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
        let (bytes, manifest) = {
            let tx = ctx.read()?;
            backup_bytes(&*tx, ctx.now())?
        };
        ctx.cancel.check()?;
        write_file(&req.path, &bytes)?;
        let count = |t: &str| {
            u32::try_from(manifest.counts.get(t).copied().unwrap_or(0)).unwrap_or(u32::MAX)
        };
        serde_json::to_value(ExportReport {
            path: req.path,
            bytes: bytes.len() as u64,
            notes: count("notes"),
            cards: count("cards"),
            media: u32::try_from(manifest.media).unwrap_or(u32::MAX),
        })
        .map_err(internal)
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
        let backup = fcbackup::read(&read_file(&req.path)?).map_err(interop_err)?;
        if !compatible(&backup.manifest.schema_versions) {
            return Err(AppError::Api(
                ApiError::new(
                    ErrorCode::ConflictUnsupportedVersion,
                    "the backup comes from a newer version",
                )
                .with_detail("format", fcbackup::FORMAT),
            ));
        }
        let dump: CollectionDump = serde_json::from_slice(&backup.collection)
            .map_err(|e| interop_err(fc_interop::InteropError::invalid("fcbackup", e)))?;
        ctx.cancel.check()?;
        // Older schema: migrate the restored rows forward (outside the writer).
        let mut dump = fc_storage::upgrade_dump(dump)?;
        forget_mcp_approvals(&mut dump);
        forget_exec_settings(&mut dump);
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
        {
            let tx = ctx.read()?;
            let (bytes, _) = backup_bytes(&*tx, ctx.now())?;
            write_file(&safety, &bytes)?;
        }
        if !Path::new(&safety).exists() {
            return Err(file_err(
                &safety,
                std::io::Error::new(std::io::ErrorKind::NotFound, "safety backup missing"),
            ));
        }
        ctx.cancel.check()?;

        ctx.report(None, "io.progress.writing");
        let counts = row_counts(&dump);
        let media = u32::try_from(backup.media.len()).unwrap_or(u32::MAX);
        self.0.put_restore(
            ctx.job_id,
            Restore {
                dump,
                media: backup.media,
            },
        );
        let result = ctx.invoke::<IoBackupLoad>(&StagedJob { job_id: ctx.job_id });
        let _ = self.0.take_restore(ctx.job_id);
        result?;
        serde_json::to_value(RestoreReport {
            safety_backup_path: safety,
            counts,
            media,
        })
        .map_err(internal)
    }
}

/// Handler of `io.backup.load`: replaces every table and the media, then starts a fresh
/// history (empty oplog + a restore barrier).
pub(crate) struct LoadHandler(pub Arc<Staging>);

impl CommandHandler<IoBackupLoad> for LoadHandler {
    fn execute(&self, req: StagedJob, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<Empty>> {
        let Restore { dump, media } = self.0.take_restore(req.job_id)?;
        ctx.tx.load_collection(&dump)?;
        let store = media_store();
        for (hash, bytes) in &media {
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
