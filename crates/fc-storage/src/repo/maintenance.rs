//! Maintenance (v1.0, migration `1050_maintenance`): form drafts and the database integrity
//! check.

use super::*;

/// One `drafts` row.
#[derive(Debug, Clone, PartialEq)]
pub struct DraftRecord {
    /// Form key (`notes.add`).
    pub key: String,
    /// The form's content (JSON).
    pub payload: Value,
    /// First saved.
    pub created_at: Timestamp,
    /// Last saved.
    pub updated_at: Timestamp,
}

/// Result of [`MaintenanceRead::integrity_check`]: raw rows, judged by the app
/// (`fc_core::maintenance::recovery::integrity_problems`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IntegrityRecord {
    /// Rows of `PRAGMA integrity_check` / `quick_check` (`["ok"]` when healthy).
    pub rows: Vec<String>,
    /// Rows of `PRAGMA foreign_key_check`.
    pub foreign_key_violations: u64,
}

/// Maintenance reads.
pub trait MaintenanceRead {
    /// Every draft, most recently saved first.
    fn list_drafts(&self) -> StorageResult<Vec<DraftRecord>>;
    /// One draft.
    fn get_draft(&self, key: &str) -> StorageResult<Option<DraftRecord>>;
    /// Runs SQLite's integrity check (`quick` = `quick_check`: skips index content, O(n) not
    /// O(n log n)) returning at most `max_rows` problem rows, plus the foreign key check.
    /// Stores without a file (in memory) report `["ok"]`.
    fn integrity_check(&self, quick: bool, max_rows: u32) -> StorageResult<IntegrityRecord>;
}

/// Maintenance writes.
pub trait MaintenanceWrite: MaintenanceRead {
    /// Inserts or replaces a draft.
    fn upsert_draft(&mut self, draft: &DraftRecord) -> StorageResult<()>;
    /// Deletes a draft. Returns whether it existed.
    fn delete_draft(&mut self, key: &str) -> StorageResult<bool>;
}
