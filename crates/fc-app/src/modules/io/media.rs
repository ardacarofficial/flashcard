//! Every media read and write of the `io` module goes through [`MediaStore`] — imports
//! (Anki/`.fcpack` media), exports (`.apkg`, `.fcpack`) and backup/restore alike.
//!
//! The store is v0.3's content-addressed file store ([`FileMedia`]): bytes in
//! `media/<ab>/<sha256>` next to the collection (served to the UI through `fcmedia://`), one
//! `media` row per hash with its reference count computed on read, files without a row
//! removed at startup. Fields reference media as `media:<sha256>` (`<img src="media:…">`,
//! `<audio controls src="media:…">`, bare refs in image/audio fields; see
//! [`fc_core::content::refs`]).

use crate::error::AppResult;
use fc_core::content::media::{detect_mime, media_hash};
use fc_storage::repo::MediaRecord;
use fc_storage::{Reads, WriteTx};

/// Where the `io` module stores and finds media bytes.
pub(crate) trait MediaStore: Send + Sync {
    /// Stores `bytes` as `record.hash` (content-addressed: an existing hash is kept as is).
    /// Returns whether the media was new.
    fn put(&self, tx: &mut dyn WriteTx, record: &MediaRecord, bytes: &[u8]) -> AppResult<bool>;
    /// Content of `hash` (`None` if unknown or its bytes are missing).
    fn bytes(&self, reads: &dyn Reads, hash: &str) -> AppResult<Option<Vec<u8>>>;
    /// Metadata of `hash`.
    fn record(&self, reads: &dyn Reads, hash: &str) -> AppResult<Option<MediaRecord>>;
    /// Every media file with stored content (full backups).
    fn list(&self, reads: &dyn Reads) -> AppResult<Vec<MediaRecord>>;
}

/// The store the module uses.
pub(crate) fn media_store() -> &'static dyn MediaStore {
    &FileMedia
}

/// v0.3's media store: `media` rows + content-addressed files.
pub(crate) struct FileMedia;

impl MediaStore for FileMedia {
    fn put(&self, tx: &mut dyn WriteTx, record: &MediaRecord, bytes: &[u8]) -> AppResult<bool> {
        // Readers verify hashes already; this keeps a bad caller from poisoning the store.
        if media_hash(bytes) != record.hash {
            return Err(crate::support::invalid(format!(
                "media {} does not match its content",
                record.hash
            )));
        }
        // File first: a row never points at missing bytes (a crash leaves at most an orphan
        // file, removed at the next startup).
        tx.write_media_file(&record.hash, bytes)?;
        let new = tx.get_media(&record.hash)?.is_none();
        if new {
            // The type as `media.add` would see it (content first, then the name).
            let mime = detect_mime(bytes, record.original_name.as_deref())
                .map_or_else(|| record.mime.clone(), str::to_owned);
            tx.insert_media(&MediaRecord {
                mime,
                size: bytes.len() as u64,
                ref_count: 0,
                ..record.clone()
            })?;
        }
        Ok(new)
    }

    fn bytes(&self, reads: &dyn Reads, hash: &str) -> AppResult<Option<Vec<u8>>> {
        if !fc_core::content::refs::is_media_hash(hash) {
            return Ok(None);
        }
        Ok(reads.read_media_file(hash)?)
    }

    fn record(&self, reads: &dyn Reads, hash: &str) -> AppResult<Option<MediaRecord>> {
        Ok(reads.get_media(hash)?)
    }

    fn list(&self, reads: &dyn Reads) -> AppResult<Vec<MediaRecord>> {
        let mut all = reads.list_media()?;
        all.sort_by(|a, b| a.hash.cmp(&b.hash));
        Ok(all)
    }
}
