//! Small helpers over the `zip` crate (all packages are zip files).

use crate::error::{InteropError, InteropResult};
use std::io::{Cursor, Read, Write};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

/// Largest single entry we inflate (guards against zip bombs).
const MAX_ENTRY: u64 = 4 * 1024 * 1024 * 1024;

pub(crate) struct ZipIn<'a> {
    zip: ZipArchive<Cursor<&'a [u8]>>,
    format: &'static str,
}

impl<'a> ZipIn<'a> {
    pub(crate) fn open(bytes: &'a [u8], format: &'static str) -> InteropResult<Self> {
        let zip = ZipArchive::new(Cursor::new(bytes))
            .map_err(|e| InteropError::invalid(format, format!("not a zip archive: {e}")))?;
        Ok(Self { zip, format })
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

    /// Bytes of entry `name` (`None` if absent).
    pub(crate) fn read(&mut self, name: &str) -> InteropResult<Option<Vec<u8>>> {
        let format = self.format;
        let mut file = match self.zip.by_name(name) {
            Ok(f) => f,
            Err(zip::result::ZipError::FileNotFound) => return Ok(None),
            Err(e) => return Err(InteropError::invalid(format, format!("{name}: {e}"))),
        };
        if file.size() > MAX_ENTRY {
            return Err(InteropError::invalid(
                format,
                format!("{name} is too large"),
            ));
        }
        let mut out = Vec::with_capacity(usize::try_from(file.size()).unwrap_or(0));
        file.read_to_end(&mut out)
            .map_err(|e| InteropError::invalid(format, format!("{name}: {e}")))?;
        Ok(Some(out))
    }

    pub(crate) fn require(&mut self, name: &str) -> InteropResult<Vec<u8>> {
        self.read(name)?
            .ok_or_else(|| InteropError::invalid(self.format, format!("{name} is missing")))
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
