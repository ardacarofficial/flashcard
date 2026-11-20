//! `.fcbackup` full collection backup (docs/02 §16): a zip with
//!
//! * `manifest.json` — [`Manifest`] (format, version, schema migrations, row counts),
//! * `collection.json` — the storage's table dump (opaque here; produced and restored by
//!   `fc-storage`),
//! * `media/<sha256>` — every media file.

use crate::error::{InteropError, InteropResult};
use crate::media::sha256_hex;
use crate::zipio::{ZipIn, ZipOut};
use fc_core::Timestamp;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// `manifest.format`.
pub const FORMAT: &str = "fcbackup";
/// Current version.
pub const VERSION: u32 = 1;

const MANIFEST: &str = "manifest.json";
const COLLECTION: &str = "collection.json";

/// Backup header.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    /// Always [`FORMAT`].
    pub format: String,
    /// Format version.
    pub version: u32,
    /// Backup time.
    pub created_at: Timestamp,
    /// Migrations the backed-up database had applied.
    pub schema_versions: Vec<u32>,
    /// Rows per table.
    pub counts: BTreeMap<String, u64>,
    /// Media files.
    pub media: usize,
}

/// A backup read back.
#[derive(Debug, Clone, PartialEq)]
pub struct Backup {
    /// Header.
    pub manifest: Manifest,
    /// Table dump JSON.
    pub collection: Vec<u8>,
    /// `(sha256, bytes)` of every media file.
    pub media: Vec<(String, Vec<u8>)>,
}

/// Streams a backup into memory.
pub struct BackupWriter {
    zip: ZipOut,
}

impl BackupWriter {
    /// Starts a backup with its manifest and table dump.
    pub fn new(manifest: &Manifest, collection: &[u8]) -> InteropResult<Self> {
        let mut zip = ZipOut::new(FORMAT);
        let m =
            serde_json::to_vec_pretty(manifest).map_err(|e| InteropError::invalid(FORMAT, e))?;
        zip.add(MANIFEST, &m, true)?;
        zip.add(COLLECTION, collection, true)?;
        Ok(Self { zip })
    }

    /// Adds a media file.
    pub fn add_media(&mut self, hash: &str, bytes: &[u8]) -> InteropResult<()> {
        self.zip.add(&format!("media/{hash}"), bytes, true)
    }

    /// The finished `.fcbackup` bytes.
    pub fn finish(self) -> InteropResult<Vec<u8>> {
        self.zip.finish()
    }
}

/// Reads only the manifest.
pub fn read_manifest(bytes: &[u8]) -> InteropResult<Manifest> {
    let mut zip = ZipIn::open(bytes, FORMAT)?;
    parse_manifest(&zip.require(MANIFEST)?)
}

fn parse_manifest(bytes: &[u8]) -> InteropResult<Manifest> {
    let m: Manifest = serde_json::from_slice(bytes)
        .map_err(|e| InteropError::invalid(FORMAT, format!("manifest: {e}")))?;
    if m.format != FORMAT {
        return Err(InteropError::invalid(
            FORMAT,
            format!("format is {:?}", m.format),
        ));
    }
    if m.version == 0 || m.version > VERSION {
        return Err(InteropError::UnsupportedVersion {
            format: FORMAT,
            version: m.version.to_string(),
        });
    }
    Ok(m)
}

/// Reads a backup (media hashes are verified).
pub fn read(bytes: &[u8]) -> InteropResult<Backup> {
    let mut zip = ZipIn::open(bytes, FORMAT)?;
    let manifest = parse_manifest(&zip.require(MANIFEST)?)?;
    let collection = zip.require(COLLECTION)?;
    let mut media = Vec::new();
    for name in zip.names() {
        let Some(hash) = name.strip_prefix("media/") else {
            continue;
        };
        if hash.is_empty() {
            continue;
        }
        let bytes = zip.require(&name)?;
        if sha256_hex(&bytes) != hash {
            return Err(InteropError::invalid(
                FORMAT,
                format!("media {hash} is damaged"),
            ));
        }
        media.push((hash.to_owned(), bytes));
    }
    media.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(Backup {
        manifest,
        collection,
        media,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrips() {
        let manifest = Manifest {
            format: FORMAT.into(),
            version: VERSION,
            created_at: Timestamp(1),
            schema_versions: vec![1, 400],
            counts: [("notes".to_string(), 2)].into(),
            media: 1,
        };
        let mut w = BackupWriter::new(&manifest, b"{\"tables\":[]}").unwrap();
        w.add_media(&sha256_hex(b"x"), b"x").unwrap();
        let bytes = w.finish().unwrap();
        let b = read(&bytes).unwrap();
        assert_eq!(b.manifest, manifest);
        assert_eq!(b.collection, b"{\"tables\":[]}");
        assert_eq!(b.media, vec![(sha256_hex(b"x"), b"x".to_vec())]);
        assert_eq!(read_manifest(&bytes).unwrap().media, 1);
    }
}
