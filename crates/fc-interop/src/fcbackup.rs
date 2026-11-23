//! `.fcbackup` full collection backup (docs/02 §16): a zip with
//!
//! * `collection.json` — the storage's table dump (opaque here; produced and restored by
//!   `fc-storage`), deflated,
//! * `media/<sha256>` — every media file, stored (audio and images do not shrink),
//! * `manifest.json` — [`Manifest`] (format, version, schema migrations, row counts and, since
//!   version 2, the size and SHA-256 of `collection.json` and of every media file, plus the
//!   media whose bytes were missing when the backup was taken). Written last: it describes
//!   what was actually written.
//!
//! **Streaming, both ways** (v1.4): [`BackupWriter`] writes into any `Write + Seek` (a file)
//! one entry at a time — the dump is serialized straight into its entry, media files go in
//! one by one — and [`BackupReader`] reads from any `Read + Seek`, one entry at a time.
//! Memory is bounded by the largest single media file, never by the archive. So whatever the
//! app can write, it can read back: there is no total size ceiling for this format (unlike
//! the foreign formats behind [`crate::zipio`]'s budget).
//!
//! **Damaged or hostile input** is still refused entry by entry: the size a header declares
//! must match the manifest (v2), a header may not claim more than deflate can produce from
//! its compressed bytes ([`MAX_RATIO`]), an entry may not yield more (or fewer) bytes than it
//! declares, the zip CRC must match, and every listed file must exist and hash to its name.
//!
//! **Missing media**: a media row whose file is gone (or whose bytes no longer match their
//! hash) cannot be backed up. The backup is still written — a collection with one lost audio
//! file must not lose every later backup — but the hash is listed in
//! [`Manifest::missing_media`] and the backup counts as *incomplete*
//! ([`Manifest::is_complete`]); callers report it.
//!
//! Version 1 backups (no hashes in the manifest) are still read: their media are whatever
//! `media/*` entries the archive has, each verified against its name.

use crate::error::{InteropError, InteropResult};
use crate::media::{hex, sha256_hex};
use fc_core::Timestamp;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Cursor, Read, Seek, Write};
use zip::read::ZipFile;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

/// `manifest.format`.
pub const FORMAT: &str = "fcbackup";
/// Current version (2: streamed, every entry listed with its size and hash).
pub const VERSION: u32 = 2;

const MANIFEST: &str = "manifest.json";
const COLLECTION: &str = "collection.json";
const MEDIA_PREFIX: &str = "media/";
/// Largest manifest accepted (a few hundred bytes per media file).
const MAX_MANIFEST: u64 = 256 * 1024 * 1024;
/// Deflate cannot expand data more than ~1032:1; a header claiming more lies.
pub const MAX_RATIO: u64 = 1100;
/// Allowance on top of the ratio for tiny entries.
const RATIO_SLACK: u64 = 64 * 1024;
/// Largest up-front allocation taken from a size the archive claims.
const MAX_PREALLOC: u64 = 16 * 1024 * 1024;

/// Size and SHA-256 of one archive entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntrySum {
    /// Lowercase hex SHA-256 of the content.
    pub sha256: String,
    /// Uncompressed size.
    pub size: u64,
}

/// One media file of a backup (`media/<hash>`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaEntry {
    /// SHA-256 of the content (also its name).
    pub hash: String,
    /// Size in bytes.
    pub size: u64,
}

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
    /// Media files in the archive.
    pub media: usize,
    /// v2: size and hash of `collection.json`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collection: Option<EntrySum>,
    /// v2: every media file in the archive, by hash.
    #[serde(default)]
    pub media_files: Vec<MediaEntry>,
    /// v2: media the collection had but whose bytes could not be read (missing file, or
    /// content no longer matching its hash) when the backup was taken.
    #[serde(default)]
    pub missing_media: Vec<String>,
}

impl Manifest {
    /// Whether every media file of the collection is in the backup.
    pub fn is_complete(&self) -> bool {
        self.missing_media.is_empty()
    }
}

/// What [`BackupWriter::new`] needs to know up front.
#[derive(Debug, Clone, PartialEq)]
pub struct BackupHeader {
    /// Backup time.
    pub created_at: Timestamp,
    /// Migrations the backed-up database had applied.
    pub schema_versions: Vec<u32>,
    /// Rows per table.
    pub counts: BTreeMap<String, u64>,
}

fn write_err(e: impl ToString) -> InteropError {
    InteropError::invalid(FORMAT, format!("write: {}", e.to_string()))
}

/// Counts and hashes what passes through.
struct Hashing<W> {
    inner: W,
    hasher: Sha256,
    n: u64,
}

impl<W: Write> Write for Hashing<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.hasher.update(&buf[..n]);
        self.n += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// Writes a backup into `W`, one entry at a time.
pub struct BackupWriter<W: Write + Seek> {
    zip: ZipWriter<W>,
    header: BackupHeader,
    collection: Option<EntrySum>,
    media: Vec<MediaEntry>,
    missing: BTreeSet<String>,
    seen: BTreeSet<String>,
}

impl<W: Write + Seek> BackupWriter<W> {
    /// Starts a backup into `out`.
    pub fn new(out: W, header: BackupHeader) -> Self {
        Self {
            zip: ZipWriter::new(out),
            header,
            collection: None,
            media: Vec::new(),
            missing: BTreeSet::new(),
            seen: BTreeSet::new(),
        }
    }

    /// Serializes the table dump straight into `collection.json` (no intermediate buffer).
    pub fn write_collection<T: Serialize + ?Sized>(&mut self, dump: &T) -> InteropResult<()> {
        // Buffered: serde_json writes token by token, and every write would otherwise go
        // through the hasher and the deflater on its own (v1.4 Z1: 12 s → ~1 s on 200k rows).
        self.collection_entry(|w| {
            let mut b = io::BufWriter::with_capacity(1 << 16, w);
            serde_json::to_writer(&mut b, dump).map_err(io::Error::other)?;
            b.flush()
        })
    }

    /// Writes `collection.json` from bytes already serialized (fixtures, forged backups).
    pub fn write_collection_bytes(&mut self, json: &[u8]) -> InteropResult<()> {
        self.collection_entry(|w| w.write_all(json))
    }

    fn collection_entry(
        &mut self,
        body: impl FnOnce(&mut Hashing<&mut ZipWriter<W>>) -> io::Result<()>,
    ) -> InteropResult<()> {
        if self.collection.is_some() {
            return Err(write_err("collection written twice"));
        }
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Deflated)
            .large_file(true);
        self.zip
            .start_file(COLLECTION, options)
            .map_err(write_err)?;
        let mut h = Hashing {
            inner: &mut self.zip,
            hasher: Sha256::new(),
            n: 0,
        };
        body(&mut h).map_err(write_err)?;
        let (hasher, n) = (h.hasher, h.n);
        self.collection = Some(EntrySum {
            sha256: hex(&hasher.finalize()),
            size: n,
        });
        Ok(())
    }

    /// Adds media file `hash`. Returns `false` (and records it as missing instead) when
    /// `bytes` do not match the hash. A hash already added is skipped.
    pub fn add_media(&mut self, hash: &str, bytes: &[u8]) -> InteropResult<bool> {
        if sha256_hex(bytes) != hash {
            self.mark_missing(hash);
            return Ok(false);
        }
        if !self.seen.insert(hash.to_owned()) {
            return Ok(true);
        }
        let options = SimpleFileOptions::default()
            .compression_method(CompressionMethod::Stored)
            .large_file(bytes.len() as u64 >= u64::from(u32::MAX));
        self.zip
            .start_file(format!("{MEDIA_PREFIX}{hash}"), options)
            .map_err(write_err)?;
        self.zip.write_all(bytes).map_err(write_err)?;
        self.media.push(MediaEntry {
            hash: hash.to_owned(),
            size: bytes.len() as u64,
        });
        Ok(true)
    }

    /// Records media file `hash` as missing (its bytes could not be read).
    pub fn mark_missing(&mut self, hash: &str) {
        if !self.seen.contains(hash) {
            self.missing.insert(hash.to_owned());
        }
    }

    /// Writes the manifest, finishes the archive and returns the output and the manifest.
    pub fn finish(mut self) -> InteropResult<(W, Manifest)> {
        let Some(collection) = self.collection.take() else {
            return Err(write_err("no collection written"));
        };
        let mut media = std::mem::take(&mut self.media);
        media.sort_by(|a, b| a.hash.cmp(&b.hash));
        let manifest = Manifest {
            format: FORMAT.into(),
            version: VERSION,
            created_at: self.header.created_at,
            schema_versions: self.header.schema_versions.clone(),
            counts: self.header.counts.clone(),
            media: media.len(),
            collection: Some(collection),
            media_files: media,
            missing_media: std::mem::take(&mut self.missing).into_iter().collect(),
        };
        let m = serde_json::to_vec_pretty(&manifest).map_err(write_err)?;
        self.zip
            .start_file(
                MANIFEST,
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
            )
            .map_err(write_err)?;
        self.zip.write_all(&m).map_err(write_err)?;
        let out = self.zip.finish().map_err(write_err)?;
        Ok((out, manifest))
    }
}

/// Yields at most `declared` bytes of an entry; more is an error.
struct Bounded<R> {
    inner: R,
    declared: u64,
    produced: u64,
}

impl<R: Read> Read for Bounded<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.produced += n as u64;
        if self.produced > self.declared {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "entry is larger than its header says",
            ));
        }
        Ok(n)
    }
}

/// Reads a backup from `R`, one entry at a time.
pub struct BackupReader<R: Read + Seek> {
    zip: ZipArchive<R>,
    manifest: Manifest,
}

fn invalid(reason: impl ToString) -> InteropError {
    InteropError::invalid(FORMAT, reason)
}

fn damaged(hash: &str) -> InteropError {
    invalid(format!("media {hash} is damaged"))
}

impl<R: Read + Seek> BackupReader<R> {
    /// Opens a backup and reads its manifest (the rest is read on demand).
    pub fn open(input: R) -> InteropResult<Self> {
        let zip = ZipArchive::new(input).map_err(|e| invalid(format!("not a zip archive: {e}")))?;
        let mut r = Self {
            zip,
            manifest: placeholder_manifest(),
        };
        let bytes = r.with_entry(MANIFEST, None, |f| {
            let mut out = Vec::new();
            f.take(MAX_MANIFEST + 1).read_to_end(&mut out)?;
            if out.len() as u64 > MAX_MANIFEST {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "too large"));
            }
            Ok(out)
        })?;
        r.manifest = parse_manifest(&bytes)?;
        Ok(r)
    }

    /// The manifest.
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// The media files the archive must contain: the manifest's list (v2), or every
    /// `media/<hash>` entry (v1, sizes from the headers).
    pub fn media_list(&self) -> Vec<MediaEntry> {
        if self.manifest.version >= 2 {
            return self.manifest.media_files.clone();
        }
        let names: Vec<String> = self
            .zip
            .file_names()
            .filter_map(Result::ok)
            .map(|n| n.into_owned())
            .collect();
        let mut out: Vec<MediaEntry> = names
            .into_iter()
            .filter_map(|n| {
                let hash = n.strip_prefix(MEDIA_PREFIX)?;
                if hash.is_empty() {
                    return None;
                }
                Some(MediaEntry {
                    hash: hash.to_owned(),
                    size: 0,
                })
            })
            .collect();
        out.sort_by(|a, b| a.hash.cmp(&b.hash));
        out
    }

    /// Runs `f` over entry `name`, guarded: present, header size as `expected` (when known),
    /// a plausible compression ratio, never more bytes than declared, and — after `f` — the
    /// entry read to its end (so the zip CRC is checked) with exactly the declared size.
    fn with_entry<T>(
        &mut self,
        name: &str,
        expected: Option<u64>,
        f: impl FnOnce(&mut Bounded<ZipFile<'_, R>>) -> io::Result<T>,
    ) -> InteropResult<T> {
        let file = match self.zip.by_name(name) {
            Ok(f) => f,
            Err(zip::result::ZipError::FileNotFound) => {
                return Err(invalid(format!("{name} is missing")))
            }
            Err(e) => return Err(invalid(format!("{name}: {e}"))),
        };
        let declared = file.size();
        if let Some(want) = expected {
            if declared != want {
                return Err(invalid(format!(
                    "{name}: {declared} bytes, the manifest says {want}"
                )));
            }
        }
        let limit = file
            .compressed_size()
            .saturating_mul(MAX_RATIO)
            .saturating_add(RATIO_SLACK);
        if declared > limit {
            return Err(invalid(format!(
                "{name}: claims an implausible compression ratio"
            )));
        }
        let mut bounded = Bounded {
            inner: file,
            declared,
            produced: 0,
        };
        let out = f(&mut bounded).map_err(|e| invalid(format!("{name}: {e}")))?;
        io::copy(&mut bounded, &mut io::sink()).map_err(|e| invalid(format!("{name}: {e}")))?;
        if bounded.produced != declared {
            return Err(invalid(format!("{name} is truncated")));
        }
        Ok(out)
    }

    /// Parses `collection.json` as it streams out of the archive (checked against the
    /// manifest's size and hash in v2).
    pub fn collection<T: DeserializeOwned>(&mut self) -> InteropResult<T> {
        let sum = self.manifest.collection.clone();
        let (value, hash) = self.with_entry(COLLECTION, sum.as_ref().map(|s| s.size), |f| {
            let mut h = HashingRead {
                inner: f,
                hasher: Sha256::new(),
            };
            // Buffered: serde_json reads byte by byte (see `write_collection`). What the buffer
            // read ahead is hashed already; the rest is drained below.
            let value: T = serde_json::from_reader(io::BufReader::with_capacity(1 << 16, &mut h))
                .map_err(io::Error::other)?;
            io::copy(&mut h, &mut io::sink())?;
            Ok((value, hex(&h.hasher.finalize())))
        })?;
        check_sum(COLLECTION, sum.as_ref(), &hash)?;
        Ok(value)
    }

    /// The raw `collection.json` (tests and tools; restores use [`BackupReader::collection`]).
    pub fn collection_bytes(&mut self) -> InteropResult<Vec<u8>> {
        let sum = self.manifest.collection.clone();
        let bytes = self.with_entry(COLLECTION, sum.as_ref().map(|s| s.size), |f| {
            let mut out = Vec::new();
            f.read_to_end(&mut out)?;
            Ok(out)
        })?;
        check_sum(COLLECTION, sum.as_ref(), &sha256_hex(&bytes))?;
        Ok(bytes)
    }

    fn media_expected(&self, entry: &MediaEntry) -> Option<u64> {
        (self.manifest.version >= 2).then_some(entry.size)
    }

    /// Checks that media file `entry` is in the archive and hashes to its name, streaming
    /// (nothing is held in memory).
    pub fn check_media(&mut self, entry: &MediaEntry) -> InteropResult<()> {
        let expected = self.media_expected(entry);
        let hash = self.with_entry(&format!("{MEDIA_PREFIX}{}", entry.hash), expected, |f| {
            let mut h = HashingRead {
                inner: f,
                hasher: Sha256::new(),
            };
            io::copy(&mut h, &mut io::sink())?;
            Ok(hex(&h.hasher.finalize()))
        })?;
        if hash != entry.hash {
            return Err(damaged(&entry.hash));
        }
        Ok(())
    }

    /// Bytes of media file `entry`, verified against its hash.
    pub fn read_media(&mut self, entry: &MediaEntry) -> InteropResult<Vec<u8>> {
        let expected = self.media_expected(entry);
        let bytes = self.with_entry(&format!("{MEDIA_PREFIX}{}", entry.hash), expected, |f| {
            let cap = usize::try_from(f.declared.min(MAX_PREALLOC)).unwrap_or(0);
            let mut out = Vec::with_capacity(cap);
            f.read_to_end(&mut out)?;
            Ok(out)
        })?;
        if sha256_hex(&bytes) != entry.hash {
            return Err(damaged(&entry.hash));
        }
        Ok(bytes)
    }

    /// Checks the whole archive without holding it: the collection's size and hash (v2; v1:
    /// that it is readable) and every listed media file. `step` runs before each media file
    /// with `(done, total)` (progress, cancellation: an error stops the check).
    pub fn check(
        &mut self,
        mut step: impl FnMut(usize, usize) -> InteropResult<()>,
    ) -> InteropResult<()> {
        let sum = self.manifest.collection.clone();
        let hash = self.with_entry(COLLECTION, sum.as_ref().map(|s| s.size), |f| {
            let mut h = HashingRead {
                inner: f,
                hasher: Sha256::new(),
            };
            io::copy(&mut h, &mut io::sink())?;
            Ok(hex(&h.hasher.finalize()))
        })?;
        check_sum(COLLECTION, sum.as_ref(), &hash)?;
        let media = self.media_list();
        for (i, m) in media.iter().enumerate() {
            step(i, media.len())?;
            self.check_media(m)?;
        }
        Ok(())
    }
}

fn check_sum(name: &str, sum: Option<&EntrySum>, hash: &str) -> InteropResult<()> {
    match sum {
        Some(s) if s.sha256 != hash => Err(invalid(format!("{name} is damaged"))),
        _ => Ok(()),
    }
}

/// Hashes what is read through it.
struct HashingRead<R> {
    inner: R,
    hasher: Sha256,
}

impl<R: Read> Read for HashingRead<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.hasher.update(&buf[..n]);
        Ok(n)
    }
}

fn placeholder_manifest() -> Manifest {
    Manifest {
        format: FORMAT.into(),
        version: VERSION,
        created_at: Timestamp(0),
        schema_versions: Vec::new(),
        counts: BTreeMap::new(),
        media: 0,
        collection: None,
        media_files: Vec::new(),
        missing_media: Vec::new(),
    }
}

fn parse_manifest(bytes: &[u8]) -> InteropResult<Manifest> {
    let m: Manifest =
        serde_json::from_slice(bytes).map_err(|e| invalid(format!("manifest: {e}")))?;
    if m.format != FORMAT {
        return Err(invalid(format!("format is {:?}", m.format)));
    }
    if m.version == 0 || m.version > VERSION {
        return Err(InteropError::UnsupportedVersion {
            format: FORMAT,
            version: m.version.to_string(),
        });
    }
    if m.version >= 2 && m.collection.is_none() {
        return Err(invalid("manifest: collection is not described"));
    }
    if m.media_files.iter().any(|e| !is_hash(&e.hash)) {
        return Err(invalid("manifest: bad media name"));
    }
    Ok(m)
}

fn is_hash(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

// ---------------------------------------------------------------- in-memory convenience

/// A backup read back whole (tests and small tools; the app streams with [`BackupReader`]).
#[derive(Debug, Clone, PartialEq)]
pub struct Backup {
    /// Header.
    pub manifest: Manifest,
    /// Table dump JSON.
    pub collection: Vec<u8>,
    /// `(sha256, bytes)` of every media file.
    pub media: Vec<(String, Vec<u8>)>,
}

/// Reads only the manifest of an in-memory backup.
pub fn read_manifest(bytes: &[u8]) -> InteropResult<Manifest> {
    Ok(BackupReader::open(Cursor::new(bytes))?.manifest)
}

/// Reads an in-memory backup whole (every entry verified).
pub fn read(bytes: &[u8]) -> InteropResult<Backup> {
    let mut r = BackupReader::open(Cursor::new(bytes))?;
    let collection = r.collection_bytes()?;
    let mut media = Vec::new();
    for m in r.media_list() {
        let bytes = r.read_media(&m)?;
        media.push((m.hash, bytes));
    }
    Ok(Backup {
        manifest: r.manifest,
        collection,
        media,
    })
}

/// Writes a whole backup into memory (tests and fixtures).
pub fn write(
    header: BackupHeader,
    collection_json: &[u8],
    media: &[(String, Vec<u8>)],
) -> InteropResult<(Vec<u8>, Manifest)> {
    let mut w = BackupWriter::new(Cursor::new(Vec::new()), header);
    w.write_collection_bytes(collection_json)?;
    for (hash, bytes) in media {
        w.add_media(hash, bytes)?;
    }
    let (out, manifest) = w.finish()?;
    Ok((out.into_inner(), manifest))
}

#[cfg(test)]
mod tests;
