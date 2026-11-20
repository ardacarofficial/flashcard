//! `io` module (v0.5; docs/02 §15–16): import (text/CSV, Quizlet, JSON, Anki, `.fcpack`),
//! export (text, JSON, `.apkg`, `.colpkg`, `.fcpack`), full backup/restore and printing.
//!
//! Building blocks: `fc_interop` parses/writes formats (pure), background jobs
//! ([`crate::jobs`]) read files and report progress, and one internal command per job writes
//! the result:
//!
//! * **Import** = job `io.import` parses the source into a staged [`import::Staged`] value,
//!   then invokes `io.import.apply` (internal) once per chunk of ~500 rows, each in its own
//!   short transaction (other commands get the writer in between), all recorded as **one undo
//!   step**. A chunk's inverse is `io.import.trash` (soft-deletes what it created); updated
//!   notes and created classification options go through nested `notes.update` /
//!   `attributes.option.add` calls with their own inverses. Cancelling (or a failure) rolls
//!   the current chunk back and removes the committed ones (`io.import.discard`).
//! * **Export / backup** = jobs reading one snapshot and writing a file.
//! * **Restore** = job `io.backup.restore` brings the backup to the current schema in a
//!   private database (later data migrations included), writes a safety backup of the current
//!   state under the app data folder, then invokes `io.backup.load` (internal), which swaps
//!   every table in one transaction (atomic on purpose: a half-restored collection must never
//!   be visible). Media go through [`media::MediaStore`].

mod apply;
mod backup;
mod export;
mod import;
mod media;
mod preview;
mod print;
mod rows;

use crate::error::{AppError, AppResult};
use crate::jobs::CancelToken;
use crate::module::{main_menu, ModuleDescriptor};
use crate::registry::Registry;
use fc_api::v1::io::{ImportFormat, TextOptionsDto};
use fc_api::v1::methods::*;
use fc_api::{ApiError, ErrorCode};
use fc_core::JobId;
use fc_interop::text::TextOptions;
use fc_interop::InteropError;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// Module id.
pub const ID: &str = "io";

/// Job kinds.
pub const JOB_IMPORT: &str = "io.import";
/// Export job kind.
pub const JOB_EXPORT: &str = "io.export";
/// Backup job kind.
pub const JOB_BACKUP: &str = "io.backup";
/// Restore job kind.
pub const JOB_RESTORE: &str = "io.restore";

/// Descriptor.
pub fn descriptor() -> ModuleDescriptor {
    ModuleDescriptor {
        id: ID,
        name_key: "modules.io.name",
        description_key: "modules.io.description",
        enabled_by_default: true,
        can_disable: true,
        settings: Vec::new(),
        ui: vec![main_menu("io.main", "nav.io", "io", "/io", 60)],
    }
}

/// Registers the module, its methods and its jobs.
pub fn register(reg: &mut Registry) {
    reg.module(descriptor());
    let staging = Arc::new(Staging::default());
    reg.query::<IoImportPreview>(preview::preview);
    reg.command::<IoImportStart>(import::start);
    reg.command::<IoImportApply>(apply::ApplyHandler(staging.clone()));
    reg.command::<IoImportTrash>(apply::trash);
    reg.command::<IoImportUntrash>(apply::untrash);
    reg.command::<IoImportDiscard>(apply::discard);
    reg.command::<IoExportStart>(export::start);
    reg.command::<IoBackupCreate>(backup::create);
    reg.query::<IoBackupInspect>(backup::inspect);
    reg.command::<IoBackupRestore>(backup::restore);
    reg.command::<IoBackupLoad>(backup::LoadHandler(staging.clone()));
    reg.query::<IoPrintRender>(print::render);
    reg.job(Arc::new(import::ImportJob(staging.clone())));
    reg.job(Arc::new(export::ExportJob));
    reg.job(Arc::new(backup::BackupJob));
    reg.job(Arc::new(backup::RestoreJob(staging)));
}

/// Data a job hands to its internal command (kept in memory, not in the oplog).
#[derive(Default)]
pub(crate) struct Staging {
    imports: Mutex<HashMap<JobId, (apply::ApplyState, CancelToken)>>,
    restores: Mutex<HashMap<JobId, backup::Restore>>,
}

impl Staging {
    fn put_import(&self, job: JobId, staged: apply::ApplyState, cancel: CancelToken) {
        if let Ok(mut m) = self.imports.lock() {
            m.insert(job, (staged, cancel));
        }
    }

    fn take_import(&self, job: JobId) -> AppResult<(apply::ApplyState, CancelToken)> {
        self.imports
            .lock()
            .ok()
            .and_then(|mut m| m.remove(&job))
            .ok_or_else(|| AppError::api(ErrorCode::Internal, format!("nothing staged for {job}")))
    }

    /// Progress of a staged import between chunks (`None` = finished or unknown).
    fn import_progress(&self, job: JobId) -> Option<f32> {
        self.imports
            .lock()
            .ok()
            .and_then(|m| m.get(&job).map(|(s, _)| s.progress()))
    }

    fn put_restore(&self, job: JobId, backup: backup::Restore) {
        if let Ok(mut m) = self.restores.lock() {
            m.insert(job, backup);
        }
    }

    fn take_restore(&self, job: JobId) -> AppResult<backup::Restore> {
        self.restores
            .lock()
            .ok()
            .and_then(|mut m| m.remove(&job))
            .ok_or_else(|| AppError::api(ErrorCode::Internal, format!("nothing staged for {job}")))
    }
}

/// Maps a format error to the API.
pub(crate) fn interop_err(e: InteropError) -> AppError {
    let msg = e.to_string();
    AppError::Api(match e {
        InteropError::UnsupportedVersion { format, version } => {
            ApiError::new(ErrorCode::ConflictUnsupportedVersion, msg)
                .with_detail("format", format)
                .with_detail("version", version)
        }
        InteropError::Options(reason) => {
            ApiError::new(ErrorCode::ValidationTextOptions, msg).with_detail("reason", reason)
        }
        InteropError::Invalid { format, reason } => {
            ApiError::new(ErrorCode::ValidationImportFormat, msg)
                .with_detail("format", format)
                .with_detail("reason", reason)
        }
        _ => ApiError::new(ErrorCode::ValidationImportFormat, msg),
    })
}

/// `IO_FILE` with the path.
pub(crate) fn file_err(path: &str, e: std::io::Error) -> AppError {
    AppError::Api(
        ApiError::new(ErrorCode::IoFile, format!("{path}: {e}"))
            .with_detail("path", path)
            .with_detail("reason", e.to_string()),
    )
}

/// Reads a whole file.
pub(crate) fn read_file(path: &str) -> AppResult<Vec<u8>> {
    std::fs::read(path).map_err(|e| file_err(path, e))
}

/// Writes a whole file (via a temporary sibling, so a failed export never leaves half a file).
pub(crate) fn write_file(path: &str, bytes: &[u8]) -> AppResult<()> {
    let target = Path::new(path);
    let tmp = target.with_extension(format!(
        "{}.part",
        target.extension().and_then(|e| e.to_str()).unwrap_or("tmp")
    ));
    std::fs::write(&tmp, bytes).map_err(|e| file_err(path, e))?;
    std::fs::rename(&tmp, target).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        file_err(path, e)
    })
}

/// Format from an explicit choice or the file extension (text otherwise).
pub(crate) fn format_of(explicit: Option<ImportFormat>, path: Option<&str>) -> ImportFormat {
    if let Some(f) = explicit {
        return f;
    }
    let ext = path
        .and_then(|p| Path::new(p).extension())
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    match ext.as_str() {
        "apkg" | "colpkg" => ImportFormat::Anki,
        "fcpack" => ImportFormat::Fcpack,
        "json" => ImportFormat::Json,
        _ => ImportFormat::Text,
    }
}

/// API → interop separators.
pub(crate) fn text_options(dto: &TextOptionsDto) -> TextOptions {
    TextOptions {
        field_separator: dto.field_separator.clone(),
        record_separator: dto.record_separator.clone(),
        quotes: dto.quotes,
        max_fields: dto.max_fields.map(|n| n as usize),
    }
}

/// Interop → API separators.
pub(crate) fn text_options_dto(o: &TextOptions) -> TextOptionsDto {
    TextOptionsDto {
        field_separator: o.field_separator.clone(),
        record_separator: o.record_separator.clone(),
        quotes: o.quotes,
        max_fields: o.max_fields.and_then(|n| u32::try_from(n).ok()),
    }
}

/// Text of an import source: UTF-8 (BOM stripped), UTF-16 LE/BE (BOM or detected), else
/// Windows-1254 (Turkish Excel/Notepad on Windows); see [`fc_interop::text::decode`].
pub(crate) fn decode_text(bytes: &[u8]) -> String {
    fc_interop::text::decode(bytes).text
}

#[cfg(test)]
pub(crate) mod tests;
#[cfg(test)]
mod real_anki_tests;
