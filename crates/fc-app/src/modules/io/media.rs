//! Every media read and write of the `io` module goes through [`MediaStore`] — imports
//! (Anki/`.fcpack` media), exports (`.apkg`, `.fcpack`) and backup/restore alike.
//!
//! The collection's canonical media store is v0.3's content-addressed file store
//! (`media/<ab>/<sha256>` next to the collection, the `media` table with reference counts
//! computed on read, served to the UI through the `fcmedia://` protocol; fields reference
//! media as `media:<sha256>`, see [`fc_interop::media`]). Until both tracks meet, the only
//! implementation here is [`BlobMedia`], a **temporary adapter** over the `media_blobs` table
//! of migration 0400. Binding the module to v0.3's store is one new impl plus one line in
//! [`media_store`]:
//!
//! ```ignore
//! impl MediaStore for FileMedia {
//!     fn put(&self, tx: &mut dyn WriteTx, r: &MediaRecord, bytes: &[u8]) -> AppResult<bool> {
//!         tx.write_media_file(&r.hash, bytes)?;
//!         let new = tx.get_media(&r.hash)?.is_none();
//!         if new { tx.insert_media(r)?; }
//!         Ok(new)
//!     }
//!     fn bytes(&self, reads: &dyn Reads, hash: &str) -> AppResult<Option<Vec<u8>>> {
//!         Ok(reads.read_media_file(hash)?)
//!     }
//!     fn record(..) /* reads.get_media */   fn list(..) /* reads.list_media */
//! }
//! ```

use crate::error::AppResult;
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

/// The store the module uses (the one line to change when binding another store).
pub(crate) fn media_store() -> &'static dyn MediaStore {
    &BlobMedia
}

/// Temporary adapter: bytes in the `media_blobs` table (migration 0400), metadata in `media`.
pub(crate) struct BlobMedia;

impl MediaStore for BlobMedia {
    fn put(&self, tx: &mut dyn WriteTx, record: &MediaRecord, bytes: &[u8]) -> AppResult<bool> {
        Ok(tx.put_media(record, bytes)?)
    }

    fn bytes(&self, reads: &dyn Reads, hash: &str) -> AppResult<Option<Vec<u8>>> {
        Ok(reads.media_bytes(hash)?)
    }

    fn record(&self, reads: &dyn Reads, hash: &str) -> AppResult<Option<MediaRecord>> {
        Ok(reads.get_media(hash)?)
    }

    fn list(&self, reads: &dyn Reads) -> AppResult<Vec<MediaRecord>> {
        Ok(reads.list_media()?)
    }
}
