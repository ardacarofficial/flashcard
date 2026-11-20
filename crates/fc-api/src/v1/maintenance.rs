//! App maintenance (v1.0, docs/02 §34): crash recovery, database integrity, log export, form
//! drafts — and signed updates (`updates.*`).
//!
//! Nothing here sends anything anywhere: crash reports and logs stay on disk until the user
//! exports them; an update check contacts the update server only when the user asks for it
//! (or turned on `updates.check_on_start`).

use crate::error::ApiError;
use fc_core::maintenance::recovery::RecoveryReason;
use fc_core::{JobId, SessionId, Timestamp};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

// ------------------------------------------------------------------ runs, crashes, integrity

/// The previous run of the desktop app, as its run marker recorded it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RunInfoDto {
    /// When it started.
    pub started_at: Timestamp,
    /// App version it ran.
    pub version: String,
    /// Whether it closed normally (`false` = crashed, killed or lost power).
    pub clean: bool,
}

/// One crash report on disk (`<data dir>/crash/`), already redacted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CrashReportDto {
    /// File name (`crash-20261120-101112-123.txt`).
    pub file: String,
    /// App version that crashed.
    pub version: String,
    /// When (`YYYY-MM-DDTHH:MM:SS.mmmZ`).
    pub time: String,
    /// Thread that panicked.
    pub thread: String,
    /// `file:line:column`, if known.
    pub location: Option<String>,
    /// First line of the panic message.
    pub message: String,
}

/// Result of a database integrity check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct IntegrityDto {
    /// No problem found.
    pub ok: bool,
    /// `true` = SQLite `quick_check` (startup), `false` = full `integrity_check`.
    pub quick: bool,
    /// What SQLite reported (bounded), empty when `ok`.
    pub problems: Vec<String>,
    /// When it ran.
    pub checked_at: Timestamp,
    /// How long it took.
    pub duration_ms: u32,
}

/// `maintenance.integrity.check`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct CheckIntegrity {
    /// Quick check (default `false`: the full check).
    #[serde(default)]
    pub quick: Option<bool>,
}

/// A study session left open by the previous run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RecoverableSession {
    /// Session (resume with `study.session` / the study screen).
    pub id: SessionId,
    /// Session kind (`normal`, `custom`, `mistakes`, `learn`, `test`).
    pub kind: String,
    /// When it started.
    pub started_at: Timestamp,
}

/// `maintenance.recovery.get`: what to offer after the previous run ended badly.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct RecoveryDto {
    /// Why to offer recovery; `null` = nothing to offer (clean start, or dismissed).
    pub reason: Option<RecoveryReason>,
    /// The previous run, when the app knows it.
    pub previous_run: Option<RunInfoDto>,
    /// Newest crash report written during the previous run.
    pub crash: Option<CrashReportDto>,
    /// The integrity check run at this start (after an unclean shutdown).
    pub integrity: Option<IntegrityDto>,
    /// Study session to resume.
    pub session: Option<RecoverableSession>,
    /// Saved form drafts to restore, newest first.
    pub drafts: Vec<DraftDto>,
}

// ------------------------------------------------------------------ logs

/// One log file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct LogFileDto {
    /// File name (`flashcard.log`, `flashcard.log.1`, …).
    pub name: String,
    /// Size in bytes.
    pub bytes: u64,
}

/// `maintenance.status`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct MaintenanceStatus {
    /// App version.
    pub version: String,
    /// Folder of the collection (`null` when unknown, e.g. in memory).
    pub data_dir: Option<String>,
    /// Folder of the log files (`null` = logging to files is off, e.g. the CLI).
    pub log_dir: Option<String>,
    /// Log files, current first.
    pub logs: Vec<LogFileDto>,
    /// Crash reports, newest first.
    pub crash_reports: Vec<CrashReportDto>,
    /// The previous run, when known.
    pub previous_run: Option<RunInfoDto>,
    /// Latest integrity check of this run, if any.
    pub integrity: Option<IntegrityDto>,
}

/// `maintenance.logs.export`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ExportLogs {
    /// Absolute path of the `.zip` to write (from a save dialog).
    pub path: String,
}

/// Response of `maintenance.logs.export`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct LogExportDto {
    /// Where it was written.
    pub path: String,
    /// Files in the bundle (logs, crash reports, `system.txt`).
    pub files: u32,
    /// Bundle size.
    pub bytes: u64,
    /// Secrets masked while exporting (beyond what was masked when written).
    pub redactions: u32,
}

/// Response of `maintenance.crash_reports.clear`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ClearedCount {
    /// Files removed.
    pub removed: u32,
}

// ------------------------------------------------------------------ drafts

/// A saved form draft.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct DraftDto {
    /// Form key (`notes.add`).
    pub key: String,
    /// The form's content (opaque JSON, the form's own shape).
    #[ts(type = "unknown")]
    pub payload: Value,
    /// First saved.
    pub created_at: Timestamp,
    /// Last saved.
    pub updated_at: Timestamp,
}

/// `maintenance.drafts.save`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SaveDraft {
    /// Form key: 1–100 characters of `a-z 0-9 . _ : -`.
    pub key: String,
    /// The form's content (at most 256 KiB as JSON).
    #[ts(type = "unknown")]
    pub payload: Value,
}

/// A draft key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct DraftRef {
    /// Form key.
    pub key: String,
}

/// Response of `maintenance.drafts.get` / `.discard` (the draft, if there was one).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct DraftResult {
    /// The draft.
    pub draft: Option<DraftDto>,
}

/// Response of `maintenance.drafts.list`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct DraftList {
    /// Drafts, newest first.
    pub items: Vec<DraftDto>,
}

// ------------------------------------------------------------------ updates

/// An update offered by the update server (its signature is checked when downloading).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct UpdateInfoDto {
    /// New version.
    pub version: String,
    /// Release notes (plain text / Markdown from the release).
    pub notes: Option<String>,
    /// Publication date as the manifest gives it.
    pub date: Option<String>,
}

/// Where the update flow is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum UpdatePhase {
    /// Nothing going on.
    Idle,
    /// Downloading (job `install_job`).
    Downloading,
    /// Downloaded and signature-verified: `updates.apply` installs it and restarts.
    Ready,
}

/// `updates.status` / `updates.check`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct UpdateStatus {
    /// Running version.
    pub current_version: String,
    /// Whether this build can update itself (installed release build).
    pub supported: bool,
    /// Setting `updates.check_on_start`.
    pub check_on_start: bool,
    /// The app should check now: supported, `check_on_start` on, not checked yet this run.
    /// (The UI calls `updates.check` when it sees this; the policy stays here.)
    pub check_due: bool,
    /// Last successful check.
    pub last_checked_at: Option<Timestamp>,
    /// Newer version found by the last check.
    pub available: Option<UpdateInfoDto>,
    /// Phase.
    pub phase: UpdatePhase,
    /// Download job, while downloading.
    pub install_job: Option<JobId>,
    /// Why the last check or download failed.
    pub last_error: Option<ApiError>,
}
