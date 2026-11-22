//! Import / export DTOs (v0.5; docs/02 §15–16). Imports, exports, backups and restores run
//! as background jobs: the `*.start` commands return the queued [`JobDto`]; progress arrives as
//! `job_progress` events and the report is in `JobDto.result` when it finishes.
//!
//! An import is **one undo step** (`oplog.undo` removes everything it added and reverts what
//! it updated) and can be cancelled (`jobs.cancel`) until it commits — a cancelled import
//! leaves nothing behind.
//!
//! [`JobDto`]: super::jobs::JobDto

use super::common::I18nMessage;
use fc_core::{AttributeDefId, CardId, DeckId, JobId, NoteId, NoteTypeId, Timestamp};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

/// Where import data comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ImportSource {
    /// A file on disk (chosen in a file dialog).
    File {
        /// Absolute path.
        path: String,
    },
    /// Pasted text (text, Quizlet and JSON formats).
    Text {
        /// The text.
        text: String,
    },
}

/// Import formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ImportFormat {
    /// Delimited text / CSV / TSV (`term, definition`, `term: definition`, custom).
    Text,
    /// Quizlet export text (two columns; the definition may contain the separator).
    Quizlet,
    /// Our JSON note format.
    Json,
    /// Anki package (`.apkg` or `.colpkg`).
    Anki,
    /// Our deck package (`.fcpack`).
    Fcpack,
}

/// Export formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ExportFormat {
    /// Delimited text (one note per record; fields in note type order, then tags).
    Text,
    /// Our JSON note format.
    Json,
    /// Anki deck package (`.apkg`).
    Apkg,
    /// Anki collection package (`.colpkg`, whole collection).
    Colpkg,
    /// Our deck package (`.fcpack`).
    Fcpack,
}

/// Separators of the text formats (see `fc_interop::text`). `record_separator`: `"\n"` one
/// record per line, `"\n\n"` blank-line separated, anything else is a custom separator.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(optional_fields = nullable)]
pub struct TextOptionsDto {
    /// Between fields (`,`, `:`, `\t`, ` - `, `>>` …). Two spaces (`"  "`) = columns aligned
    /// with runs of blanks.
    pub field_separator: String,
    /// Between records.
    pub record_separator: String,
    /// Honour `"…"` quoting.
    pub quotes: bool,
    /// Split into at most this many fields (`null` = unlimited).
    pub max_fields: Option<u32>,
}

/// What a text column becomes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ColumnTarget {
    /// Not imported.
    Ignore,
    /// A note type field.
    Field {
        /// Field key.
        key: String,
    },
    /// The note type's field with this role (v1.3: `example` keeps a sentence column with
    /// the word, whatever the field is called). A role the note type lacks is refused like an
    /// unknown field (`VALIDATION_COLUMN_MAPPING`).
    Role {
        /// Field role.
        role: fc_core::notetype::FieldRole,
    },
    /// Space/comma separated tags.
    Tags,
    /// A classification (option label, number, `true/false`, text or date; multi-choice
    /// values separated by `|` or `,`).
    Attribute {
        /// Definition.
        attr_id: AttributeDefId,
    },
}

/// What to do with a note that already exists (same id, or same checksum in the same note
/// type).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum DuplicatePolicy {
    /// Keep the existing note, skip the incoming one.
    #[default]
    Skip,
    /// Overwrite the existing note's fields, tags and classifications.
    Update,
    /// Add the incoming note anyway.
    Add,
}

/// `io.import.preview`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct PreviewImport {
    /// Data.
    pub source: ImportSource,
    /// Format (`null` = from the file extension, else text).
    pub format: Option<ImportFormat>,
    /// Separators (`null` = auto-detect).
    pub text_options: Option<TextOptionsDto>,
    /// Note type the columns map to (`null` = Classic).
    pub notetype_id: Option<NoteTypeId>,
    /// Target deck (classifications applicable there are offered for mapping).
    pub deck_id: Option<DeckId>,
    /// Rows to return (default 20, max 200).
    pub limit: Option<u32>,
}

/// Contents of a package (Anki / `.fcpack`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PackageSummary {
    /// Deck paths in the package.
    pub decks: Vec<String>,
    /// Note types.
    pub notetypes: u32,
    /// Notes.
    pub notes: u32,
    /// Cards.
    pub cards: u32,
    /// Review log entries.
    pub review_logs: u32,
    /// Media files.
    pub media: u32,
    /// Classification definitions.
    pub attribute_defs: u32,
}

/// Response of `io.import.preview`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ImportPreview {
    /// Format that will be used.
    pub format: ImportFormat,
    /// Separators used for the rows below (detected when not given).
    pub text_options: Option<TextOptionsDto>,
    /// Detection confidence 0–1 (`null` when separators were given).
    pub confidence: Option<f32>,
    /// Columns in the widest row.
    pub columns: u32,
    /// First rows (text formats; JSON notes are shown as their fields in note type order).
    pub rows: Vec<Vec<String>>,
    /// All rows/notes.
    pub total_rows: u32,
    /// Suggested mapping, one per column.
    pub suggested_columns: Vec<ColumnTarget>,
    /// Package contents (Anki / `.fcpack`).
    pub package: Option<PackageSummary>,
}

/// `io.import.start`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct StartImport {
    /// Data.
    pub source: ImportSource,
    /// Format (`null` = from the file extension, else text).
    pub format: Option<ImportFormat>,
    /// Separators (`null` = auto-detect).
    pub text_options: Option<TextOptionsDto>,
    /// Text/JSON: deck of the notes (required unless every JSON note names its deck).
    /// Packages: parent of the imported deck tree (`null` = top level).
    pub deck_id: Option<DeckId>,
    /// Text: note type (`null` = Classic).
    pub notetype_id: Option<NoteTypeId>,
    /// Text: one target per column (`null`/empty = note type fields in order).
    pub columns: Option<Vec<ColumnTarget>>,
    /// Text: the first row is a header.
    pub skip_first_row: Option<bool>,
    /// Existing notes (default `skip`).
    pub duplicates: Option<DuplicatePolicy>,
    /// Classification columns: create options for unknown labels (default `false` → the
    /// row's value is reported and left empty).
    pub create_missing_options: Option<bool>,
    /// Packages: keep scheduling state and review history (default `true`).
    pub include_history: Option<bool>,
}

/// A row that could not be imported.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ImportIssue {
    /// 1-based row/note number in the source (`0` = whole file).
    pub row: u32,
    /// What went wrong.
    pub message: I18nMessage,
}

/// Result of an import (`JobDto.result` of an import job).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
pub struct ImportReport {
    /// Notes added.
    pub added_notes: u32,
    /// Existing notes updated.
    pub updated_notes: u32,
    /// Notes skipped as duplicates.
    pub skipped_duplicates: u32,
    /// Cards added.
    pub added_cards: u32,
    /// Decks created.
    pub added_decks: u32,
    /// Note types created.
    pub added_notetypes: u32,
    /// Classification definitions created.
    pub added_attributes: u32,
    /// Classification options created.
    pub added_options: u32,
    /// Media files stored (new ones).
    pub added_media: u32,
    /// Review log entries added.
    pub added_reviews: u32,
    /// Rows/notes not imported, and other warnings.
    pub issues: Vec<ImportIssue>,
}

/// `io.export.start`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct StartExport {
    /// Format.
    pub format: ExportFormat,
    /// Destination file (overwritten).
    pub path: String,
    /// Deck (with sub-decks) to export (`null` = whole collection; ignored by `colpkg`).
    pub deck_id: Option<DeckId>,
    /// Scheduling state and review history (`fcpack`, `apkg`, `colpkg`; default `true`).
    pub include_history: Option<bool>,
    /// Separators of `text` (default tab / newline).
    pub text_options: Option<TextOptionsDto>,
}

/// Result of an export / backup (`JobDto.result`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ExportReport {
    /// Written file.
    pub path: String,
    /// File size.
    pub bytes: u64,
    /// Notes written.
    pub notes: u32,
    /// Cards written.
    pub cards: u32,
    /// Media files written.
    pub media: u32,
}

/// `io.backup.create`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct CreateBackup {
    /// Destination `.fcbackup` file.
    pub path: String,
}

/// Result of a backup (`io.backup.create`, automatic backups; `JobDto.result`). The file is
/// written to a temporary sibling, restored into a scratch profile and compared
/// (`io.backup.verify`), and only then put in place: an existing file at `path` is replaced
/// only by a verified backup.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct BackupReport {
    /// Written file.
    pub path: String,
    /// File size.
    pub bytes: u64,
    /// Notes in it.
    pub notes: u32,
    /// Cards in it.
    pub cards: u32,
    /// Review history rows in it.
    pub review_log: u32,
    /// Media files in it.
    pub media: u32,
    /// Whether it was restored into a scratch profile and matched (always `true` for a
    /// report of a finished backup; a failed check fails the job).
    pub verified: bool,
    /// Media the collection lists whose bytes are missing or damaged on disk: not in the
    /// backup (empty = complete). The backup is written anyway, marked incomplete.
    pub missing_media: Vec<String>,
}

/// `io.backup.verify`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct VerifyBackup {
    /// `.fcbackup` file.
    pub path: String,
}

/// Result of `io.backup.verify` (`JobDto.result`): the backup restored into a scratch profile
/// (the live collection is not touched) and read back.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct BackupVerifyReport {
    /// Checked file.
    pub path: String,
    /// When the backup was taken (`null` if its header is unreadable).
    pub created_at: Option<Timestamp>,
    /// Whether it restores completely and matches: `problems` is empty.
    pub ok: bool,
    /// Whether every media file of the collection is in it (see `missing_media`).
    pub complete: bool,
    /// Rows of the main tables (`decks`, `notetypes`, `notes`, `cards`, `review_log`,
    /// `media`) as read back from the scratch profile.
    pub counts: BTreeMap<String, u64>,
    /// Media files restored and matching their hash.
    pub media_files: u32,
    /// Media missing when the backup was taken.
    pub missing_media: Vec<String>,
    /// What does not match (untranslated diagnostics; empty when `ok`).
    pub problems: Vec<String>,
}

/// `io.auto_backup` setting values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum AutoBackupSchedule {
    /// No automatic backups.
    Off,
    /// One a day.
    Daily,
    /// One a week.
    Weekly,
}

/// A verified backup on disk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct VerifiedBackup {
    /// File.
    pub path: String,
    /// When it was taken.
    pub created_at: Timestamp,
    /// When it was last verified.
    pub verified_at: Timestamp,
    /// File size.
    pub bytes: u64,
    /// Whether every media file is in it.
    pub complete: bool,
    /// Whether the app made it on its own (kept in the automatic backup folder).
    pub automatic: bool,
}

/// The last automatic backup that failed (cleared by the next success).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct BackupFailure {
    /// When.
    pub at: Timestamp,
    /// Why (untranslated diagnostic).
    pub message: String,
}

/// Response of `io.backup.status`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct BackupStatus {
    /// `io.auto_backup`.
    pub schedule: AutoBackupSchedule,
    /// `io.auto_backup_keep`: verified automatic copies kept.
    pub keep: u32,
    /// Folder of the automatic backups (`null`: the collection has no folder, e.g. in memory).
    pub folder: Option<String>,
    /// The newest verified backup (automatic, `io.backup.create` or `io.backup.verify`).
    pub last_verified: Option<VerifiedBackup>,
    /// Automatic backups kept, newest first.
    pub automatic: Vec<VerifiedBackup>,
    /// When the next automatic backup is due (`null` = off or no folder).
    pub next_due: Option<Timestamp>,
    /// The last automatic backup that failed, if none succeeded since.
    pub last_failure: Option<BackupFailure>,
}

/// Response of `io.backup.auto`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct AutoBackupStart {
    /// The queued job (`null`: one is already queued or running, or there is no folder).
    pub job: Option<super::jobs::JobDto>,
}

/// `io.backup.restore`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct RestoreBackup {
    /// `.fcbackup` to restore.
    pub path: String,
    /// Where the automatic backup of the current state goes (`null` = next to `path`,
    /// named `pre-restore-<time>.fcbackup`).
    pub safety_backup_path: Option<String>,
}

/// Result of a restore (`JobDto.result`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct RestoreReport {
    /// Backup of the state before the restore.
    pub safety_backup_path: String,
    /// Rows restored per table.
    pub counts: BTreeMap<String, u64>,
    /// Media files restored.
    pub media: u32,
    /// Media that were already missing when the backup was taken (not restored).
    #[serde(default)]
    pub missing_media: Vec<String>,
}

/// `io.backup.inspect`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct InspectBackup {
    /// `.fcbackup` file.
    pub path: String,
}

/// Response of `io.backup.inspect`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct BackupInfo {
    /// When it was taken.
    pub created_at: Timestamp,
    /// Rows per table.
    pub counts: BTreeMap<String, u64>,
    /// Media files.
    pub media: u32,
    /// Whether this app version can restore it (no unknown schema migrations).
    pub compatible: bool,
    /// Whether every media file of the collection is in it (backups before v1.4 do not
    /// know: `true`).
    pub complete: bool,
}

/// Print layouts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PrintLayoutDto {
    /// Paper flash cards: fronts sheet, then backs sheet mirrored for duplex printing.
    Cards {
        /// Cards per row (1–6).
        columns: u32,
        /// Rows per sheet (1–10).
        rows: u32,
    },
    /// Two-column list.
    List,
}

/// `io.print.render`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct RenderPrint {
    /// Deck (with sub-decks).
    pub deck_id: DeckId,
    /// Layout.
    pub layout: PrintLayoutDto,
}

/// Response of `io.print.render`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PrintDocument {
    /// Standalone HTML (text only, escaped) to show in a sandboxed frame and print.
    pub html: String,
    /// Cards on it.
    pub cards: u32,
}

/// Internal `io.import.apply` / `io.backup.load`: the staged data of a running job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct StagedJob {
    /// Job that staged the data.
    pub job_id: JobId,
}

/// Internal `io.import.discard`: reverts the chunks an unfinished import already committed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct DiscardImport {
    /// Inverses of the committed chunks, newest first (`io.import.trash` calls are purged
    /// instead of trashed).
    pub calls: Vec<crate::InvokeRequest>,
    /// When the import started (tags it created are removed too).
    pub since: Timestamp,
}

/// Internal `io.import.trash` / `io.import.untrash`: rows an import created.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ImportedIds {
    /// Decks.
    pub decks: Vec<DeckId>,
    /// Note types.
    pub notetypes: Vec<NoteTypeId>,
    /// Notes.
    pub notes: Vec<NoteId>,
    /// Cards.
    pub cards: Vec<CardId>,
    /// Classification definitions.
    pub attribute_defs: Vec<AttributeDefId>,
    /// Trashed note types the import brought back (trashed again, never purged).
    #[serde(default)]
    pub revived_notetypes: Vec<NoteTypeId>,
    /// Media the import added (hashes). Kept by undo (trashed notes still reference them);
    /// removed again when an unfinished import is discarded and nothing else uses them.
    #[serde(default)]
    pub media: Vec<String>,
}

impl ImportedIds {
    /// Whether nothing was created.
    pub fn is_empty(&self) -> bool {
        self.decks.is_empty()
            && self.notetypes.is_empty()
            && self.notes.is_empty()
            && self.cards.is_empty()
            && self.attribute_defs.is_empty()
            && self.revived_notetypes.is_empty()
            && self.media.is_empty()
    }
}
