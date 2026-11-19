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
