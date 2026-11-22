//! Subtitle capture diagnostics (v1.4 A5, migration `1430_subtitle_capture_log`): one row per
//! capture attempt, bounded. Only ids, codes and times — never the sentence, the word, a URL
//! or a video title. Written straight (no oplog: diagnostics, not user content).

use super::*;

/// One `subtitle_capture_log` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureLogRecord {
    /// Insertion order (assigned by storage; ignored on insert).
    pub seq: i64,
    /// The extension's capture id as sent (may be empty or malformed for a rejected one).
    pub capture_id: String,
    /// When the attempt arrived.
    pub at: Timestamp,
    /// `created`, `duplicate`, `rejected` or `media_missing`.
    pub result: String,
    /// Machine reason (`invalid_capture:term.lemma`, `audio:upload_unknown`, …).
    pub reason: Option<String>,
    /// The card made or matched.
    pub card_id: Option<CardId>,
    /// Time the server spent on it, ms.
    pub duration_ms: i64,
}

/// Capture log reads.
pub trait CaptureLogRead {
    /// The newest `limit` rows, newest first.
    fn recent_capture_log(&self, limit: u32) -> StorageResult<Vec<CaptureLogRecord>>;
}

/// Capture log writes.
pub trait CaptureLogWrite: CaptureLogRead {
    /// Appends `rec` and deletes all but the newest `keep` rows (`keep` 0 keeps none).
    fn append_capture_log(&mut self, rec: &CaptureLogRecord, keep: u32) -> StorageResult<()>;
}
