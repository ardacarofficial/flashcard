//! Full backup (`.fcbackup`) and restore.
//!
//! A backup is the storage's table dump plus every media file. A restore first writes a
//! backup of the current state (docs/02 §16), then replaces every table in one transaction
//! (`io.backup.load`). It is not an undo step — the safety backup is the way back.

use super::{file_err, interop_err, read_file, write_file, Staging, JOB_BACKUP, JOB_RESTORE};
use crate::error::{AppError, AppResult};
use crate::handler::{CommandCtx, CommandHandler, CommandOutcome, QueryCtx};
use crate::jobs::{enqueue, JobContext, JobHandler};
use crate::support::{invalid, summary};
use fc_api::v1::common::Empty;
use fc_api::v1::events::DomainEvent;
use fc_api::v1::io::*;
use fc_api::v1::jobs::JobDto;
use fc_api::v1::methods::{IoBackupCreate, IoBackupLoad, IoBackupRestore};
use fc_api::{ApiError, ErrorCode};
use fc_interop::fcbackup::{self, Backup, BackupWriter, Manifest};
use fc_storage::repo::{CollectionDump, MediaRecord};
use fc_storage::Reads;
use serde_json::Value;
use std::path::Path;
use std::sync::Arc;

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

/// A complete backup of what `reads` sees.
fn backup_bytes(reads: &dyn Reads, now: fc_core::Timestamp) -> AppResult<(Vec<u8>, Manifest)> {
    let dump = reads.dump_collection()?;
    let media = reads.list_media()?;
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
        if let Some(bytes) = reads.media_bytes(&m.hash)? {
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

        // Safety backup of the current state first.
        ctx.report(Some(0.3), "io.progress.safety_backup");
        let safety = req.safety_backup_path.clone().unwrap_or_else(|| {
            let dir = Path::new(&req.path)
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_default();
            dir.join(format!("pre-restore-{}.fcbackup", ctx.now().0))
                .to_string_lossy()
                .into_owned()
        });
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
        self.0.put_restore(ctx.job_id, backup);
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

/// Handler of `io.backup.load`: replaces every table and the media.
pub(crate) struct LoadHandler(pub Arc<Staging>);

impl CommandHandler<IoBackupLoad> for LoadHandler {
    fn execute(&self, req: StagedJob, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<Empty>> {
        let Backup {
            collection, media, ..
        } = self.0.take_restore(req.job_id)?;
        let dump: CollectionDump = serde_json::from_slice(&collection)
            .map_err(|e| interop_err(fc_interop::InteropError::invalid("fcbackup", e)))?;
        ctx.tx.load_collection(&dump)?;
        for (hash, bytes) in &media {
            let record = ctx.reads().get_media(hash)?.unwrap_or(MediaRecord {
                hash: hash.clone(),
                mime: "application/octet-stream".into(),
                size: bytes.len() as u64,
                original_name: None,
                created_at: ctx.now,
            });
            ctx.tx.put_media(&record, bytes)?;
        }
        // Jobs that were running when the backup was taken have no worker now.
        ctx.tx.mark_unfinished_jobs_interrupted(ctx.now)?;
        Ok(CommandOutcome {
            response: Empty {},
            inverse: Vec::new(),
            summary: summary::<IoBackupLoad>(),
            events: vec![DomainEvent::CollectionReplaced],
        })
    }
}
