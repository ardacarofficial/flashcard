//! "Export logs for a bug report" (docs/02 §34): log files, crash reports and a short system
//! summary in one `.zip` at a path the user chose. Nothing is sent anywhere.
//!
//! Everything is redacted **again** while exporting, now also with the exact API keys the
//! app holds (the AI providers' keychain entries): a secret that reached a log before the
//! rules knew its shape still never leaves in the bundle.

use super::{crash, done, logging, MaintenanceServices, APP_VERSION, CRASH_DIR};
use crate::error::{AppError, AppResult};
use crate::handler::{CommandCtx, CommandOutcome};
use crate::modules::ai::{key_account, AiServices};
use crate::App;
use fc_api::v1::maintenance::{ExportLogs, LogExportDto};
use fc_api::v1::methods::MaintenanceLogsExport;
use fc_api::{ApiError, ErrorCode};
use fc_core::maintenance::iso_datetime;
use fc_core::maintenance::redact::Redactor;
use fc_storage::Reads;
use std::io::Write;
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;

/// Largest single file put into the bundle (a runaway log is cut to its tail).
pub const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;

/// The exact secrets the app holds right now: every AI provider's key from the keychain.
pub(crate) fn secrets_from(reads: &dyn Reads, ai: Option<&AiServices>) -> Vec<String> {
    let Some(ai) = ai else {
        return Vec::new();
    };
    let secrets = ai.secrets();
    reads
        .list_ai_providers(true)
        .unwrap_or_default()
        .iter()
        .filter_map(|p| secrets.get(&key_account(p.id)).ok().flatten())
        .map(|s| s.expose().to_owned())
        .collect()
}

/// [`secrets_from`] for the app (start, settings change).
pub(crate) fn known_secrets(app: &App) -> Vec<String> {
    let Ok(tx) = app.store().read() else {
        return Vec::new();
    };
    let ai = app.registry().extension::<AiServices>();
    secrets_from(&*tx, ai.as_deref())
}

fn io_error(path: &Path, e: impl std::fmt::Display) -> AppError {
    AppError::Api(
        ApiError::new(ErrorCode::IoFile, format!("{}: {e}", path.display()))
            .with_detail("path", path.display().to_string()),
    )
}

fn invalid_path(why: &str) -> AppError {
    AppError::Api(
        ApiError::new(ErrorCode::ValidationInvalidParams, why.to_owned()).with_detail("field", "path"),
    )
}

/// Reads a file as text, keeping only its last [`MAX_FILE_BYTES`].
fn read_tail(path: &Path) -> std::io::Result<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut f = std::fs::File::open(path)?;
    let len = f.metadata()?.len();
    if len > MAX_FILE_BYTES {
        f.seek(SeekFrom::Start(len - MAX_FILE_BYTES))?;
    }
    let mut buf = Vec::new();
    f.read_to_end(&mut buf)?;
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

/// What goes into the bundle: `(name in the zip, text)`, before the final redaction.
fn collect(svc: &MaintenanceServices, ctx: &CommandCtx<'_>) -> AppResult<Vec<(String, String)>> {
    let mut files = Vec::new();
    files.push(("system.txt".to_owned(), system_summary(svc, ctx)?));
    if let Some(log) = svc.log() {
        log.flush();
        for (name, _) in logging::list_files(log.dir()) {
            let path = log.dir().join(&name);
            files.push((format!("logs/{name}"), read_tail(&path).map_err(|e| io_error(&path, e))?));
        }
    }
    if let Some(dir) = svc.data_dir().map(|d| d.join(CRASH_DIR)) {
        for r in crash::list_reports(&dir) {
            let path = dir.join(&r.file);
            files.push((format!("crash/{}", r.file), read_tail(&path).map_err(|e| io_error(&path, e))?));
        }
    }
    Ok(files)
}

fn system_summary(svc: &MaintenanceServices, ctx: &CommandCtx<'_>) -> AppResult<String> {
    let mut s = String::new();
    s.push_str("Flashcard log bundle\n");
    s.push_str(&format!("version: {APP_VERSION}\n"));
    s.push_str(&format!("os: {}\n", crash::os_label()));
    s.push_str(&format!("locale: {:?}\n", ctx.locale));
    s.push_str(&format!("exported: {}\n", iso_datetime(ctx.now)));
    if let Some(dir) = svc.data_dir() {
        s.push_str(&format!("data folder: {}\n", dir.display()));
    }
    match svc.previous_run() {
        Some(p) => s.push_str(&format!(
            "previous run: {} {} ({})\n",
            iso_datetime(p.started_at),
            p.version,
            if p.clean { "ended cleanly" } else { "did not end cleanly" }
        )),
        None => s.push_str("previous run: unknown\n"),
    }
    match svc.read().integrity {
        Some(i) if i.ok => s.push_str(&format!("integrity: ok ({})\n", iso_datetime(i.checked_at))),
        Some(i) => s.push_str(&format!(
            "integrity: {} problem(s) ({})\n",
            i.problems.len(),
            iso_datetime(i.checked_at)
        )),
        None => s.push_str("integrity: not checked this run\n"),
    }
    let states = crate::support::module_states(ctx.reads(), ctx.registry)?;
    let mut modules: Vec<String> = states
        .iter()
        .map(|(id, on)| format!("{id}={}", if *on { "on" } else { "off" }))
        .collect();
    modules.sort();
    s.push_str(&format!("modules: {}\n", modules.join(", ")));
    Ok(s)
}

/// Writes the bundle to `target` (through a temporary file next to it). Returns
/// `(files, bytes, redactions)`.
pub(crate) fn write_bundle(
    target: &Path,
    files: &[(String, String)],
    redactor: &Redactor,
) -> AppResult<(u32, u64, u32)> {
    let tmp: PathBuf = target.with_extension("zip.part");
    let mut redactions = 0u32;
    {
        let file = std::fs::File::create(&tmp).map_err(|e| io_error(&tmp, e))?;
        let mut zip = zip::ZipWriter::new(file);
        let options = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(0o644);
        for (name, text) in files {
            let clean = redactor.redact(text);
            redactions = redactions.saturating_add(u32::try_from(clean.count).unwrap_or(u32::MAX));
            zip.start_file(name.as_str(), options)
                .map_err(|e| io_error(&tmp, e))?;
            zip.write_all(clean.text.as_bytes())
                .map_err(|e| io_error(&tmp, e))?;
        }
        zip.finish().map_err(|e| io_error(&tmp, e))?;
    }
    std::fs::rename(&tmp, target).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        io_error(target, e)
    })?;
    let bytes = std::fs::metadata(target).map(|m| m.len()).unwrap_or(0);
    Ok((u32::try_from(files.len()).unwrap_or(u32::MAX), bytes, redactions))
}

pub(super) fn export_logs(req: ExportLogs, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<LogExportDto>> {
    let target = PathBuf::from(req.path.trim());
    if !target.is_absolute() {
        return Err(invalid_path("path must be absolute"));
    }
    if !target
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("zip"))
    {
        return Err(invalid_path("path must end in .zip"));
    }
    if !target.parent().is_some_and(Path::is_dir) {
        return Err(invalid_path("folder does not exist"));
    }
    let svc = ctx
        .registry
        .extension::<MaintenanceServices>()
        .ok_or_else(|| AppError::api(ErrorCode::Internal, "maintenance services missing"))?;
    let files = collect(&svc, ctx)?;
    let ai = ctx.registry.extension::<AiServices>();
    let base = svc.log().map_or_else(Redactor::new, |l| (*l.redactor()).clone());
    let redactor = base.with_secrets(secrets_from(ctx.reads(), ai.as_deref()));
    let (files, bytes, redactions) = write_bundle(&target, &files, &redactor)?;
    log::info!(target: "fc_app::maintenance", "logs exported ({files} files, {bytes} bytes, {redactions} masked)");
    Ok(done::<MaintenanceLogsExport, _>(LogExportDto {
        path: target.display().to_string(),
        files,
        bytes,
        redactions,
    }))
}
