//! Small helpers over the `zip` crate (all packages are zip files).
//!
//! **Decompression budget** (zip/zstd bombs): every [`ZipIn`] has a total budget of
//! decompressed bytes, charged with the bytes *actually produced* (headers can lie), plus a
//! per-entry cap. Zstd payloads inside the archive are decoded through [`ZipIn::zstd`] and
//! charged to the same budget. Buffers are never pre-allocated from a claimed size beyond
//! [`MAX_PREALLOC`].

use crate::error::{InteropError, InteropResult};
use std::io::{Cursor, Read, Write};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

/// Largest single entry we inflate.
const MAX_ENTRY: u64 = 4 * 1024 * 1024 * 1024;
/// Total budget: `input length × MAX_RATIO`, clamped to `[MIN_BUDGET, MAX_BUDGET]`.
const MAX_RATIO: u64 = 200;
const MIN_BUDGET: u64 = 64 * 1024 * 1024;
const MAX_BUDGET: u64 = 4 * 1024 * 1024 * 1024;
/// Largest up-front allocation taken from a size the archive claims.
const MAX_PREALLOC: u64 = 16 * 1024 * 1024;
/// Largest zstd window we accept (2^27 = 128 MiB, zstd's default decoder limit).
const ZSTD_WINDOW_LOG_MAX: u32 = 27;

const TOO_LARGE: &str = "decompressed data exceeds the size limit";

/// Default budget for an archive of `input_len` bytes.
fn default_budget(input_len: usize) -> u64 {
    (input_len as u64)
        .saturating_mul(MAX_RATIO)
        .clamp(MIN_BUDGET, MAX_BUDGET)
}

/// Reads all of `r` into a buffer, failing once more than `limit` bytes come out.
fn read_limited(
    r: impl Read,
    limit: u64,
    claimed: u64,
    format: &'static str,
    what: &str,
) -> InteropResult<Vec<u8>> {
    let mut out =
        Vec::with_capacity(usize::try_from(claimed.min(limit).min(MAX_PREALLOC)).unwrap_or(0));
    r.take(limit.saturating_add(1))
        .read_to_end(&mut out)
        .map_err(|e| InteropError::invalid(format, format!("{what}: {e}")))?;
    if out.len() as u64 > limit {
        return Err(InteropError::invalid(format, TOO_LARGE));
    }
    Ok(out)
}

pub(crate) struct ZipIn<'a> {
    zip: ZipArchive<Cursor<&'a [u8]>>,
    format: &'static str,
    /// Decompressed bytes still allowed.
    budget: u64,
}

impl<'a> ZipIn<'a> {
    pub(crate) fn open(bytes: &'a [u8], format: &'static str) -> InteropResult<Self> {
        Self::open_with_budget(bytes, format, default_budget(bytes.len()))
    }

    /// [`ZipIn::open`] with an explicit total budget (tests).
    pub(crate) fn open_with_budget(
        bytes: &'a [u8],
        format: &'static str,
        budget: u64,
    ) -> InteropResult<Self> {
        let zip = ZipArchive::new(Cursor::new(bytes))
            .map_err(|e| InteropError::invalid(format, format!("not a zip archive: {e}")))?;
        Ok(Self {
            zip,
            format,
            budget,
        })
    }

    pub(crate) fn names(&self) -> Vec<String> {
        self.zip
            .file_names()
            .filter_map(Result::ok)
            .map(|n| n.into_owned())
            .collect()
    }

    #[cfg(test)]
    pub(crate) fn has(&self, name: &str) -> bool {
        self.zip.index_for_name(name).is_some()
    }

    fn charge(&mut self, n: usize) {
        self.budget = self.budget.saturating_sub(n as u64);
    }

    /// Bytes of entry `name` (`None` if absent).
    pub(crate) fn read(&mut self, name: &str) -> InteropResult<Option<Vec<u8>>> {
        let format = self.format;
        let limit = self.budget.min(MAX_ENTRY);
        let file = match self.zip.by_name(name) {
            Ok(f) => f,
            Err(zip::result::ZipError::FileNotFound) => return Ok(None),
            Err(e) => return Err(InteropError::invalid(format, format!("{name}: {e}"))),
        };
        let claimed = file.size();
        let out = read_limited(file, limit, claimed, format, name)?;
        self.charge(out.len());
        Ok(Some(out))
    }

    pub(crate) fn require(&mut self, name: &str) -> InteropResult<Vec<u8>> {
        self.read(name)?
            .ok_or_else(|| InteropError::invalid(self.format, format!("{name} is missing")))
    }

    /// Decodes a zstd payload found in this archive, charged to the same budget.
    pub(crate) fn zstd(&mut self, bytes: &[u8]) -> InteropResult<Vec<u8>> {
        let format = self.format;
        let zerr = |e: std::io::Error| InteropError::invalid(format, format!("zstd: {e}"));
        let mut decoder = zstd::stream::read::Decoder::new(bytes).map_err(zerr)?;
        decoder.window_log_max(ZSTD_WINDOW_LOG_MAX).map_err(zerr)?;
        let limit = self.budget.min(MAX_ENTRY);
        let out = read_limited(decoder, limit, 0, format, "zstd")?;
        self.charge(out.len());
        Ok(out)
    }
}

pub(crate) struct ZipOut {
    zip: ZipWriter<Cursor<Vec<u8>>>,
    format: &'static str,
}

impl ZipOut {
    pub(crate) fn new(format: &'static str) -> Self {
        Self {
            zip: ZipWriter::new(Cursor::new(Vec::new())),
            format,
        }
    }

    /// Adds an entry; `compress = false` stores it (already-compressed data).
    pub(crate) fn add(&mut self, name: &str, bytes: &[u8], compress: bool) -> InteropResult<()> {
        let method = if compress {
            CompressionMethod::Deflated
        } else {
            CompressionMethod::Stored
        };
        let options = SimpleFileOptions::default()
            .compression_method(method)
            .large_file(bytes.len() as u64 >= u32::MAX as u64);
        let format = self.format;
        self.zip
            .start_file(name, options)
            .map_err(|e| InteropError::invalid(format, e))?;
        self.zip
            .write_all(bytes)
            .map_err(|e| InteropError::invalid(format, e))
    }

    pub(crate) fn finish(self) -> InteropResult<Vec<u8>> {
        let format = self.format;
        self.zip
            .finish()
            .map(Cursor::into_inner)
            .map_err(|e| InteropError::invalid(format, e))
    }
}

#[cfg(test)]
mod tests;
