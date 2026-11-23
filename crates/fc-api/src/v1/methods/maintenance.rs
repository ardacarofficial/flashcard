//! App maintenance (v1.0): recovery after a crash, integrity check, logs, drafts; updates.

use crate::v1::common::*;
use crate::v1::jobs::JobDto;
use crate::v1::maintenance::*;

methods! {
    // ---- maintenance ----
    /// Version, folders, log files, crash reports, previous run, last integrity check.
    MaintenanceStatusGet = "maintenance.status", Query, "maintenance", "maintenance:read", Public, undo: false, Empty => MaintenanceStatus;
    /// What to offer after the previous run crashed or stopped uncleanly (nothing otherwise).
    MaintenanceRecovery = "maintenance.recovery.get", Query, "maintenance", "maintenance:read", Public, undo: false, Empty => RecoveryDto;
    /// The user saw the recovery offer: it is not shown again this run. Not an undo step.
    MaintenanceRecoveryDismiss = "maintenance.recovery.dismiss", Command, "maintenance", "maintenance:write", Public, undo: false, Empty => RecoveryDto;
    /// SQLite integrity check of the collection (+ foreign keys).
    MaintenanceIntegrityCheck = "maintenance.integrity.check", Query, "maintenance", "maintenance:read", Public, undo: false, CheckIntegrity => IntegrityDto;
    /// The collection opened but its integrity check failed (v1.4.1): records the user's
    /// rescue choice and asks the desktop shell to restart; the rescue (`backup` = newest
    /// verified backup, `readable` = keep what is readable; the damaged files are moved
    /// aside, never deleted) runs before the collection opens again. Not an undo step.
    MaintenanceRescueRequest = "maintenance.rescue.request", Command, "maintenance", "maintenance:write", Public, undo: false, RequestRescue => RescueRequestDto;
    /// Writes the logs, crash reports and a system summary — redacted — into a `.zip` the
    /// user chose. Nothing is sent anywhere. Writes a file: not an undo step.
    MaintenanceLogsExport = "maintenance.logs.export", Command, "maintenance", "maintenance:write", Public, undo: false, ExportLogs => LogExportDto;
    /// Deletes the crash reports on disk. Not an undo step.
    MaintenanceCrashReportsClear = "maintenance.crash_reports.clear", Command, "maintenance", "maintenance:write", Public, undo: false, Empty => ClearedCount;
    /// Saves a form draft (autosave while typing). Scratch state: not an undo step.
    MaintenanceDraftsSave = "maintenance.drafts.save", Command, "maintenance", "maintenance:write", Public, undo: false, SaveDraft => DraftDto;
    /// One draft.
    MaintenanceDraftsGet = "maintenance.drafts.get", Query, "maintenance", "maintenance:read", Public, undo: false, DraftRef => DraftResult;
    /// Every draft, newest first.
    MaintenanceDraftsList = "maintenance.drafts.list", Query, "maintenance", "maintenance:read", Public, undo: false, Empty => DraftList;
    /// Removes a draft (the form was saved or cleared). Not an undo step.
    MaintenanceDraftsDiscard = "maintenance.drafts.discard", Command, "maintenance", "maintenance:write", Public, undo: false, DraftRef => DraftResult;

    // ---- updates ----
    /// Update state (no network).
    UpdatesStatus = "updates.status", Query, "updates", "updates:read", Public, undo: false, Empty => UpdateStatus;
    /// Asks the update server whether a newer version exists (network; only on request or
    /// with `updates.check_on_start`).
    UpdatesCheck = "updates.check", Query, "updates", "updates:read", Public, undo: false, Empty => UpdateStatus;
    /// Starts downloading the available update as a job (lane `updates`, cancellable); the
    /// download is verified against the app's update key before it is kept.
    UpdatesInstall = "updates.install", Command, "updates", "updates:write", Public, undo: false, Empty => JobDto;
    /// Installs the downloaded, verified update and restarts the app (the run ends cleanly
    /// first).
    UpdatesApply = "updates.apply", Command, "updates", "updates:write", Public, undo: false, Empty => UpdateStatus;
}
