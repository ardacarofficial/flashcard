//! Format errors.

/// Why a file could not be read or written.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum InteropError {
    /// The input is not in the expected format (or is damaged).
    #[error("invalid {format} file: {reason}")]
    Invalid {
        /// Format name (`apkg`, `fcpack`, …).
        format: &'static str,
        /// What is wrong.
        reason: String,
    },
    /// The file is from a newer/unknown version of the format.
    #[error("unsupported {format} version {version}")]
    UnsupportedVersion {
        /// Format name.
        format: &'static str,
        /// Version found in the file.
        version: String,
    },
    /// Options are inconsistent (e.g. equal field and record separators).
    #[error("invalid options: {0}")]
    Options(String),
    /// Writing the file failed because the disk is full (v1.4.1; `fc-app` says it in plain
    /// words as `IO_DISK_FULL`).
    #[error("writing the {format} file: the disk is full ({reason})")]
    DiskFull {
        /// Format name.
        format: &'static str,
        /// The OS error.
        reason: String,
    },
}

/// Whether an I/O error means "the disk is full" (or the quota is used up), on any platform.
pub fn is_disk_full(e: &std::io::Error) -> bool {
    use std::io::ErrorKind as K;
    if matches!(e.kind(), K::StorageFull | K::QuotaExceeded) {
        return true;
    }
    // ENOSPC / EDQUOT (Linux, macOS); ERROR_HANDLE_DISK_FULL, ERROR_DISK_FULL (Windows).
    let codes: &[i32] = if cfg!(windows) {
        &[39, 112]
    } else if cfg!(target_os = "macos") {
        &[28, 69]
    } else {
        &[28, 122]
    };
    e.raw_os_error().is_some_and(|c| codes.contains(&c))
}

impl InteropError {
    /// Shorthand for [`InteropError::Invalid`].
    pub fn invalid(format: &'static str, reason: impl ToString) -> Self {
        Self::Invalid {
            format,
            reason: reason.to_string(),
        }
    }
}

/// Result alias.
pub type InteropResult<T> = Result<T, InteropError>;
