//! Subtitle extension: access keys, capture → note mappings, uploads (v1.2, migration
//! `1200_subtitle`).

use super::*;
use fc_core::SubtitleKeyId;

/// One `subtitle_keys` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubtitleKeyRecord {
    /// Id.
    pub id: SubtitleKeyId,
    /// Name the user gave it.
    pub name: String,
    /// First characters of the key (display).
    pub prefix: String,
    /// Hex SHA-256 of the key (unique).
    pub token_hash: String,
    /// Made.
    pub created_at: Timestamp,
    /// Last request.
    pub last_used_at: Option<Timestamp>,
    /// Revoked.
    pub revoked_at: Option<Timestamp>,
}

/// One `subtitle_captures` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubtitleCaptureRecord {
    /// The extension's id (UUIDv7).
    pub capture_id: String,
    /// `<lang>|<lemma>|<sentence hash>` (unique).
    pub dedup_key: String,
    /// Language (lowercase).
    pub lang: String,
    /// Lemma (lowercase).
    pub lemma: String,
    /// Note made.
    pub note_id: NoteId,
    /// Its card.
    pub card_id: CardId,
    /// Key it came with.
    pub key_id: Option<SubtitleKeyId>,
    /// Received.
    pub created_at: Timestamp,
}

/// One `subtitle_uploads` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubtitleUploadRecord {
    /// Upload id (`up_…`).
    pub id: String,
    /// Capture it was uploaded for.
    pub capture_id: String,
    /// `screenshot` or `audio`.
    pub kind: String,
    /// Media store hash of the bytes.
    pub media_hash: String,
    /// MIME type.
    pub mime: String,
    /// Uploaded.
    pub created_at: Timestamp,
}

/// Subtitle reads.
pub trait SubtitleRead {
    /// Every key, newest first.
    fn list_subtitle_keys(&self) -> StorageResult<Vec<SubtitleKeyRecord>>;
    /// One key.
    fn get_subtitle_key(&self, id: SubtitleKeyId) -> StorageResult<Option<SubtitleKeyRecord>>;
    /// The key whose hash is `token_hash`.
    fn subtitle_key_by_token_hash(
        &self,
        token_hash: &str,
    ) -> StorageResult<Option<SubtitleKeyRecord>>;
    /// A capture by id.
    fn get_subtitle_capture(&self, capture_id: &str)
        -> StorageResult<Option<SubtitleCaptureRecord>>;
    /// A capture by dedup key.
    fn subtitle_capture_by_dedup_key(
        &self,
        dedup_key: &str,
    ) -> StorageResult<Option<SubtitleCaptureRecord>>;
    /// How many captures there are, and when the last one arrived.
    fn subtitle_capture_stats(&self) -> StorageResult<(u64, Option<Timestamp>)>;
    /// Up to `limit` distinct `(lang, lemma)` pairs in that order, after `after`, of `lang`
    /// only when given.
    fn subtitle_lemma_page(
        &self,
        lang: Option<&str>,
        after: Option<(&str, &str)>,
        limit: u32,
    ) -> StorageResult<Vec<(String, String)>>;
    /// Every capture of these `(lang, lemma)` pairs.
    fn subtitle_captures_of(
        &self,
        lemmas: &[(String, String)],
    ) -> StorageResult<Vec<SubtitleCaptureRecord>>;
    /// An upload by id.
    fn get_subtitle_upload(&self, id: &str) -> StorageResult<Option<SubtitleUploadRecord>>;
    /// The upload of `capture_id` + `kind`.
    fn subtitle_upload_for(
        &self,
        capture_id: &str,
        kind: &str,
    ) -> StorageResult<Option<SubtitleUploadRecord>>;
}

/// Subtitle writes.
pub trait SubtitleWrite: SubtitleRead {
    /// Inserts or replaces a key (all columns). A hash used by another key →
    /// `Constraint { "subtitle_keys_token" }`.
    fn upsert_subtitle_key(&mut self, key: &SubtitleKeyRecord) -> StorageResult<()>;
    /// Inserts a mapping. An existing id or dedup key →
    /// `Constraint { "subtitle_captures" }`.
    fn insert_subtitle_capture(&mut self, row: &SubtitleCaptureRecord) -> StorageResult<()>;
    /// Deletes a mapping; whether it existed.
    fn delete_subtitle_capture(&mut self, capture_id: &str) -> StorageResult<bool>;
    /// Inserts or replaces the upload of its `(capture_id, kind)`.
    fn upsert_subtitle_upload(&mut self, upload: &SubtitleUploadRecord) -> StorageResult<()>;
    /// Deletes uploads made before `before`; returns them.
    fn delete_subtitle_uploads_before(
        &mut self,
        before: Timestamp,
    ) -> StorageResult<Vec<SubtitleUploadRecord>>;
}
