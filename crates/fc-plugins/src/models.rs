//! Model download manager (docs/05 §8): large files (local TTS voices, Whisper models,
//! lemmatizer dictionaries) declared by plugins and providers as `{id, name, size, url,
//! sha256, license}`.
//!
//! Files live under `<root>/<owner>/<id>`; a download writes `<id>.part` and **resumes** from
//! its length, the SHA-256 of the whole file is verified before it is renamed into place
//! (a mismatch deletes the partial file). Fetching is behind [`Fetcher`]: v0.7 ships
//! [`FileFetcher`] (`file://` URLs, used by tests and local mirrors); an HTTP fetcher plugs in
//! later without touching the manager.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

/// A downloadable model.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelSpec {
    /// Id, unique per owner (`[a-z0-9_-]`).
    pub id: String,
    /// Display name.
    pub name: String,
    /// Size in bytes.
    pub size: u64,
    /// Where to fetch it (`https://…`, `file://…`).
    pub url: String,
    /// Lowercase hex SHA-256 of the file.
    pub sha256: String,
    /// License name / URL (shown before download).
    #[serde(default)]
    pub license: String,
}

impl ModelSpec {
    /// Field checks.
    pub fn validate(&self) -> Result<(), String> {
        if !crate::manifest::valid_local_id(&self.id) {
            return Err(format!("invalid model id {:?}", self.id));
        }
        if self.size == 0 {
            return Err(format!("{}: size must be > 0", self.id));
        }
        if self.sha256.len() != 64
            || !self
                .sha256
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(format!(
                "{}: sha256 must be 64 lowercase hex digits",
                self.id
            ));
        }
        if !["https://", "http://", "file://"]
            .iter()
            .any(|s| self.url.starts_with(s))
        {
            return Err(format!("{}: unsupported URL scheme", self.id));
        }
        Ok(())
    }
}

/// Download state of a model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ModelStatus {
    /// Not on disk.
    Missing,
    /// A partial download exists (resumable).
    Partial {
        /// Bytes already on disk.
        downloaded: u64,
    },
    /// Verified and ready.
    Ready,
}

/// Download/removal failure.
#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    /// The fetcher cannot handle this URL (no HTTP fetcher yet, …).
    #[error("cannot fetch {0}")]
    Unsupported(String),
    /// Content does not match `sha256`.
    #[error("checksum mismatch for {0}")]
    ChecksumMismatch(String),
    /// More bytes than declared.
    #[error("{0} is larger than declared")]
    TooLarge(String),
    /// Cancelled by the caller; the partial file stays for resuming.
    #[error("download cancelled")]
    Cancelled,
    /// File system error.
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

/// Opens a byte stream for `url` starting at `offset` (resume).
pub trait Fetcher: Send + Sync {
    /// Stream from `offset` to the end.
    fn open(&self, url: &str, offset: u64) -> Result<Box<dyn Read + Send>, ModelError>;
}

/// `file://` fetcher.
#[derive(Debug, Default, Clone, Copy)]
pub struct FileFetcher;

impl Fetcher for FileFetcher {
    fn open(&self, url: &str, offset: u64) -> Result<Box<dyn Read + Send>, ModelError> {
        let path = url
            .strip_prefix("file://")
            .ok_or_else(|| ModelError::Unsupported(url.to_owned()))?;
        let mut f = fs::File::open(path)?;
        f.seek(SeekFrom::Start(offset))?;
        Ok(Box::new(f))
    }
}

/// Model files on disk.
#[derive(Debug, Clone)]
pub struct ModelStore {
    root: PathBuf,
}

fn safe_segment(s: &str) -> bool {
    !s.is_empty()
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
        && s != "."
        && s != ".."
}

impl ModelStore {
    /// Store rooted at `root` (created on first download).
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Final path of `owner`'s model `id`.
    pub fn path(&self, owner: &str, id: &str) -> Result<PathBuf, ModelError> {
        if !safe_segment(owner) || !safe_segment(id) {
            return Err(ModelError::Unsupported(format!("{owner}/{id}")));
        }
        Ok(self.root.join(owner).join(id))
    }

    fn part(path: &Path) -> PathBuf {
        let mut p = path.as_os_str().to_owned();
        p.push(".part");
        PathBuf::from(p)
    }

    /// Current state.
    pub fn status(&self, owner: &str, spec: &ModelSpec) -> Result<ModelStatus, ModelError> {
        let path = self.path(owner, &spec.id)?;
        if path.is_file() {
            return Ok(ModelStatus::Ready);
        }
        match fs::metadata(Self::part(&path)) {
            Ok(m) => Ok(ModelStatus::Partial {
                downloaded: m.len(),
            }),
            Err(_) => Ok(ModelStatus::Missing),
        }
    }

    /// Downloads (or resumes) `spec`, verifying its SHA-256. `cancelled` is polled between
    /// chunks (the partial file stays for resuming); `progress(done, total)` reports bytes.
    pub fn download(
        &self,
        owner: &str,
        spec: &ModelSpec,
        fetcher: &dyn Fetcher,
        cancelled: &dyn Fn() -> bool,
        progress: &mut dyn FnMut(u64, u64),
    ) -> Result<PathBuf, ModelError> {
        let path = self.path(owner, &spec.id)?;
        if path.is_file() {
            return Ok(path);
        }
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let part = Self::part(&path);
        let mut file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&part)?;
        let mut done = file.metadata()?.len();
        if done < spec.size {
            let mut stream = fetcher.open(&spec.url, done)?;
            let mut buf = vec![0u8; 64 * 1024];
            loop {
                if cancelled() {
                    return Err(ModelError::Cancelled);
                }
                let n = stream.read(&mut buf)?;
                if n == 0 {
                    break;
                }
                done += n as u64;
                if done > spec.size {
                    drop(file);
                    let _ = fs::remove_file(&part);
                    return Err(ModelError::TooLarge(spec.id.clone()));
                }
                file.write_all(&buf[..n])?;
                progress(done, spec.size);
            }
        }
        file.sync_all()?;
        drop(file);
        let mut hasher = Sha256::new();
        let mut f = fs::File::open(&part)?;
        std::io::copy(&mut f, &mut hasher)?;
        let hex: String = hasher
            .finalize()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        if hex != spec.sha256 {
            let _ = fs::remove_file(&part);
            return Err(ModelError::ChecksumMismatch(spec.id.clone()));
        }
        fs::rename(&part, &path)?;
        Ok(path)
    }

    /// Removes a model (and any partial file). Returns the bytes freed.
    pub fn delete(&self, owner: &str, id: &str) -> Result<u64, ModelError> {
        let path = self.path(owner, id)?;
        let mut freed = 0;
        for p in [Self::part(&path), path] {
            if let Ok(m) = fs::metadata(&p) {
                freed += m.len();
                fs::remove_file(&p)?;
            }
        }
        Ok(freed)
    }

    /// Bytes used by all models (partial files included).
    pub fn disk_usage(&self) -> u64 {
        fn walk(p: &Path) -> u64 {
            let Ok(entries) = fs::read_dir(p) else {
                return 0;
            };
            entries
                .flatten()
                .map(|e| match e.metadata() {
                    Ok(m) if m.is_dir() => walk(&e.path()),
                    Ok(m) => m.len(),
                    Err(_) => 0,
                })
                .sum()
        }
        walk(&self.root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec_for(src: &Path, bytes: &[u8]) -> ModelSpec {
        let hex: String = Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        ModelSpec {
            id: "voice".into(),
            name: "Voice".into(),
            size: bytes.len() as u64,
            url: format!("file://{}", src.display()),
            sha256: hex,
            license: "MIT".into(),
        }
    }

    #[test]
    fn download_resume_verify_delete() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src.bin");
        let bytes: Vec<u8> = (0..300_000u32).map(|i| (i % 251) as u8).collect();
        fs::write(&src, &bytes).unwrap();
        let spec = spec_for(&src, &bytes);
        spec.validate().unwrap();
        let store = ModelStore::new(dir.path().join("models"));
        assert_eq!(store.status("p", &spec).unwrap(), ModelStatus::Missing);

        // Cancel after the first chunk: a partial file stays.
        let mut calls = 0;
        let cancel = || {
            calls += 1;
            calls > 1
        };
        let cancel_cell = std::cell::RefCell::new(cancel);
        let r = store.download(
            "p",
            &spec,
            &FileFetcher,
            &|| (cancel_cell.borrow_mut())(),
            &mut |_, _| {},
        );
        assert!(matches!(r, Err(ModelError::Cancelled)));
        let ModelStatus::Partial { downloaded } = store.status("p", &spec).unwrap() else {
            panic!("expected partial");
        };
        assert!(downloaded > 0 && downloaded < spec.size);

        // Resume: only the rest is fetched, the result verifies.
        let mut first_progress = None;
        let path = store
            .download("p", &spec, &FileFetcher, &|| false, &mut |d, _| {
                first_progress.get_or_insert(d);
            })
            .unwrap();
        assert!(first_progress.unwrap() > downloaded);
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert_eq!(store.status("p", &spec).unwrap(), ModelStatus::Ready);
        assert_eq!(store.disk_usage(), bytes.len() as u64);
        assert_eq!(store.delete("p", "voice").unwrap(), bytes.len() as u64);
        assert_eq!(store.status("p", &spec).unwrap(), ModelStatus::Missing);
    }

    #[test]
    fn checksum_mismatch_discards_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let src = dir.path().join("src.bin");
        fs::write(&src, b"hello").unwrap();
        let mut spec = spec_for(&src, b"hello");
        spec.sha256 = "0".repeat(64);
        let store = ModelStore::new(dir.path().join("models"));
        let r = store.download("p", &spec, &FileFetcher, &|| false, &mut |_, _| {});
        assert!(matches!(r, Err(ModelError::ChecksumMismatch(_))));
        assert_eq!(store.status("p", &spec).unwrap(), ModelStatus::Missing);
        assert!(store.path("../x", "y").is_err());
    }
}
