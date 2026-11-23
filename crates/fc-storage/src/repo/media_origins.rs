//! Where stored media came from, and the TTS cache (v1.4 W4, migration `1420_media_origins`).

use super::*;
use fc_core::content::media::MediaOrigin;

/// One `media_origins` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaOriginRecord {
    /// Media hash.
    pub hash: String,
    /// Origin.
    pub origin: MediaOrigin,
    /// TTS provider id (`tts` only).
    pub provider: String,
    /// TTS voice id (`""` = the language's default voice).
    pub voice: String,
    /// TTS language.
    pub language: String,
    /// TTS speed × 1000 (`fc_core::content::media::tts_rate_key`).
    pub rate_milli: i64,
    /// TTS text hash (`fc_core::content::media::tts_text_hash`).
    pub text_hash: String,
    /// Clip: the video file name it was cut from (no path).
    pub source: String,
    /// Recorded.
    pub created_at: Timestamp,
}

/// The TTS cache key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TtsCacheKey {
    /// Provider id.
    pub provider: String,
    /// Language.
    pub language: String,
    /// Voice (`""` = default).
    pub voice: String,
    /// Speed × 1000.
    pub rate_milli: i64,
    /// Text hash.
    pub text_hash: String,
}

/// Media origin reads.
pub trait MediaOriginRead {
    /// Rows of `hashes` (hashes without a row are absent).
    fn media_origins_of(&self, hashes: &[String]) -> StorageResult<Vec<MediaOriginRecord>>;
    /// A stored (media row present) TTS file for `key`, if any.
    fn find_tts_media(&self, key: &TtsCacheKey) -> StorageResult<Option<String>>;
}

/// Media origin writes.
pub trait MediaOriginWrite: MediaOriginRead {
    /// Inserts or replaces the row of `rec.hash`.
    fn put_media_origin(&mut self, rec: &MediaOriginRecord) -> StorageResult<()>;
}
