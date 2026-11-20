//! Import / export (v0.5).

use crate::v1::common::*;
use crate::v1::io::*;
use crate::v1::jobs::JobDto;

methods! {
    // ---- io: import ----
    /// Parses the first rows / package header and suggests separators and column mapping.
    IoImportPreview = "io.import.preview", Query, "io", "io:read", Public, undo: false, PreviewImport => ImportPreview;
    /// Starts an import job (one undo step, cancellable until it commits).
    IoImportStart = "io.import.start", Command, "io", "io:write", Public, undo: false, StartImport => JobDto;
    /// Internal: writes the next chunk of what an import job staged. The job runs it once per
    /// chunk (own transaction each) and records all chunks as one undo step.
    IoImportApply = "io.import.apply", Command, "io", "io:write", Internal, undo: true (replay: false), StagedJob => ImportReport;
    /// Internal: removes what the committed chunks of a cancelled/failed import wrote. Run
    /// without history (the import never became an undo step); it has no inverse.
    IoImportDiscard = "io.import.discard", Command, "io", "io:write", Internal, undo: true (replay: false), DiscardImport => Empty;
    /// Internal: inverse of `io.import.apply` — trashes what the import created.
    IoImportTrash = "io.import.trash", Command, "io", "io:write", Internal, undo: true, ImportedIds => AffectedCount;
    /// Internal: inverse of `io.import.trash`.
    IoImportUntrash = "io.import.untrash", Command, "io", "io:write", Internal, undo: true, ImportedIds => AffectedCount;

    // ---- io: export, backup, print ----
    /// Starts an export job.
    IoExportStart = "io.export.start", Command, "io", "io:write", Public, undo: false, StartExport => JobDto;
    /// Starts a full backup job (`.fcbackup`).
    IoBackupCreate = "io.backup.create", Command, "io", "io:write", Public, undo: false, CreateBackup => JobDto;
    /// Reads a backup's header.
    IoBackupInspect = "io.backup.inspect", Query, "io", "io:read", Public, undo: false, InspectBackup => BackupInfo;
    /// Starts a restore job: backs up the current state first, then replaces the collection.
    IoBackupRestore = "io.backup.restore", Command, "io", "io:write", Public, undo: false, RestoreBackup => JobDto;
    /// Internal: replaces the collection with what a restore job staged (records no undo step:
    /// it has no inverse; the safety backup is the way back).
    IoBackupLoad = "io.backup.load", Command, "io", "io:write", Internal, undo: true (replay: false), StagedJob => Empty;
    /// Printable HTML of a deck.
    IoPrintRender = "io.print.render", Query, "io", "io:read", Public, undo: false, RenderPrint => PrintDocument;
}
