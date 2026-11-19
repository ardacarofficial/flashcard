//! Media file locations (docs/03 §13): `media/<ab>/<sha256>` next to the collection file.
//! Transports serve files from here (`fcmedia://` protocol) without going through the app.

use fc_core::content::media::media_rel_path;
use fc_core::content::refs::is_media_hash;
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
