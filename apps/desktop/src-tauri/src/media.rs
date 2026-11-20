//! The `fcmedia://` protocol (docs/03 §13): serves media files by content hash straight from
//! the collection's media store. Read-only, no business logic; only `media/<ab>/<sha256>`
//! paths can be reached (anything that is not a SHA-256 hex digest is a 404).

use std::path::Path;

/// URI scheme name (`convertFileSrc(hash, 'fcmedia')` in the frontend).
pub const SCHEME: &str = "fcmedia";

/// What to answer for a request path (`/<sha256>`): status, content type, body.
pub fn respond(db_path: &Path, uri_path: &str) -> (u16, &'static str, Vec<u8>) {
    let hash = uri_path
        .trim_start_matches('/')
        .split(['?', '#'])
        .next()
        .unwrap_or("");
    let Some(file) = fc_storage::media::media_file_path(db_path, hash) else {
        return (404, "text/plain", Vec::new());
    };
    match std::fs::read(&file) {
        Ok(bytes) => {
            let mime = fc_core::content::media::detect_mime(&bytes, None)
                .unwrap_or("application/octet-stream");
            (200, mime, bytes)
        }
        Err(_) => (404, "text/plain", Vec::new()),
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn serves_only_stored_hashes() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("collection.db");
        let bytes = b"\x89PNG\r\n\x1a\nimg";
        let hash = fc_core::content::media::media_hash(bytes);
        let path = fc_storage::media::media_file_path(&db, &hash).unwrap();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, bytes).unwrap();

        let (status, mime, body) = respond(&db, &format!("/{hash}"));
        assert_eq!((status, mime), (200, "image/png"));
        assert_eq!(body, bytes);
        assert_eq!(respond(&db, "/../../collection.db").0, 404);
        assert_eq!(respond(&db, &format!("/{}", "0".repeat(64))).0, 404);
    }
}
