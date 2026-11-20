//! `.fcpack` deck package (docs/02 §16): a zip with
//!
//! * `manifest.json` — [`Manifest`] (format, version, counts),
//! * `pack.json` — the [`Pack`] (`fc-core` models; media bytes excluded),
//! * `media/<sha256>` — one entry per media file.
//!
//! Classification definitions, options and values are stored with their ids, so a pack
//! re-imported elsewhere reproduces them exactly.

use crate::error::{InteropError, InteropResult};
use crate::media::sha256_hex;
use crate::pack::{Pack, PackCounts};
use crate::zipio::{ZipIn, ZipOut};
use fc_core::Timestamp;
use serde::{Deserialize, Serialize};

/// `manifest.format`.
pub const FORMAT: &str = "fcpack";
/// Current version. Readers accept this and older versions.
pub const VERSION: u32 = 1;

const MANIFEST: &str = "manifest.json";
const DATA: &str = "pack.json";

/// Package header.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    /// Always [`FORMAT`].
    pub format: String,
    /// Format version.
    pub version: u32,
    /// Export time.
    pub created_at: Timestamp,
    /// Display name (root deck name).
    pub name: String,
    /// Whether scheduling state and review history are included.
    pub include_history: bool,
    /// What is inside.
    pub counts: PackCounts,
}

impl Manifest {
    /// Manifest for `pack`.
    pub fn for_pack(pack: &Pack, name: &str, include_history: bool, now: Timestamp) -> Self {
        Self {
            format: FORMAT.into(),
            version: VERSION,
            created_at: now,
            name: name.into(),
            include_history,
            counts: pack.counts(),
        }
    }
}

/// Serializes `pack` into `.fcpack` bytes.
pub fn write(pack: &Pack, manifest: &Manifest) -> InteropResult<Vec<u8>> {
    let mut zip = ZipOut::new(FORMAT);
    let manifest_json =
        serde_json::to_vec_pretty(manifest).map_err(|e| InteropError::invalid(FORMAT, e))?;
    zip.add(MANIFEST, &manifest_json, true)?;
    let data = serde_json::to_vec(pack).map_err(|e| InteropError::invalid(FORMAT, e))?;
    zip.add(DATA, &data, true)?;
    for m in &pack.media {
        zip.add(&format!("media/{}", m.hash), &m.bytes, true)?;
    }
    zip.finish()
}

/// Reads only the manifest (cheap, for previews).
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

/// Reads a `.fcpack` (media hashes are verified).
pub fn read(bytes: &[u8]) -> InteropResult<(Manifest, Pack)> {
    let mut zip = ZipIn::open(bytes, FORMAT)?;
    let manifest = parse_manifest(&zip.require(MANIFEST)?)?;
    let mut pack: Pack = serde_json::from_slice(&zip.require(DATA)?)
        .map_err(|e| InteropError::invalid(FORMAT, format!("pack.json: {e}")))?;
    for m in &mut pack.media {
        let bytes = zip.require(&format!("media/{}", m.hash))?;
        if sha256_hex(&bytes) != m.hash {
            return Err(InteropError::invalid(
                FORMAT,
                format!("media {} is damaged", m.hash),
            ));
        }
        m.bytes = bytes;
    }
    Ok((manifest, pack))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pack::MediaFile;

    #[test]
    fn roundtrips_and_checks_versions() {
        let pack = Pack {
            media: vec![MediaFile {
                hash: sha256_hex(b"img"),
                mime: "image/png".into(),
                name: Some("a.png".into()),
                bytes: b"img".to_vec(),
            }],
            ..Pack::default()
        };
        let m = Manifest::for_pack(&pack, "Deck", true, Timestamp(5));
        let bytes = write(&pack, &m).unwrap();
        let (m2, p2) = read(&bytes).unwrap();
        assert_eq!(m2, m);
        assert_eq!(p2, pack);
        assert_eq!(read_manifest(&bytes).unwrap().counts.media, 1);

        let mut newer = m.clone();
        newer.version = VERSION + 1;
        let bytes = write(&pack, &newer).unwrap();
        assert!(matches!(
            read(&bytes),
            Err(InteropError::UnsupportedVersion { .. })
        ));
        assert!(read(b"not a zip").is_err());
    }
}
