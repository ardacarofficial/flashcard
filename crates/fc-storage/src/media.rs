//! Media file locations (docs/03 §13): `media/<ab>/<sha256>` next to the collection file.
//! Transports serve files from here (`fcmedia://` protocol) without going through the app.

use crate::error::{StorageError, StorageResult};
use fc_core::content::media::media_rel_path;
use fc_core::content::refs::is_media_hash;
use rusqlite::Connection;
use std::path::{Path, PathBuf};

/// Where the file of `hash` lives for the collection at `db_path` (`None` for anything that
/// is not a lowercase SHA-256 hex digest, so callers can pass untrusted input).
pub fn media_file_path(db_path: &Path, hash: &str) -> Option<PathBuf> {
    if !is_media_hash(hash) {
        return None;
    }
    let dir = db_path.parent().unwrap_or_else(|| Path::new("."));
    Some(dir.join(media_rel_path(hash)))
}

/// Before migration 0402 drops it: copies every row of the pre-release `media_blobs` table
/// (migration 0400, v0.5's temporary media adapter) to the file store of the collection at
/// `db_path`. Idempotent (existing files are kept; content-addressed) and a no-op once the
/// table is gone. Returns how many files were written.
pub fn export_legacy_blobs(conn: &Connection, db_path: &Path) -> StorageResult<u64> {
    if !crate::migrate::schema_object_exists(conn, "media_blobs")? {
        return Ok(0);
    }
    let file_err = |p: &Path, e: std::io::Error| StorageError::File {
        path: p.display().to_string(),
        message: e.to_string(),
    };
    let mut st = conn.prepare("SELECT hash, data FROM media_blobs ORDER BY hash")?;
    let mut rows = st.query([])?;
    let mut written = 0;
    while let Some(row) = rows.next()? {
        let hash: String = row.get(0)?;
        let Some(path) = media_file_path(db_path, &hash) else {
            continue;
        };
        if path.exists() {
            continue;
        }
        let data: Vec<u8> = row.get(1)?;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| file_err(dir, e))?;
        }
        let part = path.with_extension("part");
        std::fs::write(&part, &data).map_err(|e| file_err(&part, e))?;
        std::fs::rename(&part, &path).map_err(|e| file_err(&path, e))?;
        written += 1;
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_sharded_and_validated() {
        let h = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";
        let p = media_file_path(Path::new("/data/collection.db"), h).unwrap();
        assert_eq!(p, Path::new("/data/media/9f").join(h));
        assert!(media_file_path(Path::new("/data/collection.db"), "../../etc/passwd").is_none());
        assert!(media_file_path(Path::new("/data/collection.db"), &h.to_uppercase()).is_none());
    }
}
