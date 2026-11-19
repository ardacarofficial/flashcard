//! Storage errors.

use thiserror::Error;

/// Result alias.
pub type StorageResult<T> = Result<T, StorageError>;

/// Storage failure. `fc-app` maps these to `IO_DATABASE` / `NOT_FOUND_*` / `CONFLICT_*`.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum StorageError {
    /// A row expected to exist does not (`entity` = table-ish name, `id` = key).
    #[error("{entity} {id} not found")]
    NotFound {
        /// Entity kind (`deck`, `note`, …).
        entity: &'static str,
        /// Key.
        id: String,
    },
    /// A uniqueness/foreign-key constraint failed (`constraint` = index or rule name).
    #[error("constraint violated: {constraint}")]
    Constraint {
        /// Constraint/index name, e.g. `decks_sibling_name`.
        constraint: String,
    },
    /// Database is locked/busy; retryable.
    #[error("database busy")]
    Busy,
    /// Stored JSON or value could not be decoded (corruption or version skew).
    #[error("corrupt data in {table}: {message}")]
    Corrupt {
        /// Table.
        table: &'static str,
        /// Details.
        message: String,
    },
    /// Migration failed.
    #[error("migration {version} failed: {message}")]
    Migration {
        /// Migration version.
        version: u32,
        /// Details.
        message: String,
    },
    /// The database was written by a newer app (schema version above what we know).
    #[error("database schema {found} is newer than supported {supported}")]
    TooNew {
        /// Version in the file.
        found: u32,
        /// Highest version this build knows.
        supported: u32,
    },
    /// Contract exists but the implementation is pending (L2 fills these in).
    #[error("storage operation {0} not implemented")]
    NotImplemented(&'static str),
    /// Any other SQLite error.
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
}
