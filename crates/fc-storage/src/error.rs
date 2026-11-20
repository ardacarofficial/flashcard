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
        /// Constraint/index name, e.g. `decks_sibling_name`, `tags.name`, `foreign_key`.
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
    /// A paging cursor was not produced by this listing (or is from another version). Caused
    /// by client input, not by the database: `fc-app` maps it to
    /// `VALIDATION_INVALID_PARAMS` (details `{"field": "cursor"}`), never to `IO_*`.
    #[error("invalid paging cursor: {0}")]
    InvalidCursor(String),
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
    /// The database is older than this build and opening it was asked not to migrate (see
    /// `SqliteStore::open_current`): open it with the app first.
    #[error(
        "database schema {found} is older than {latest} and must be migrated by the app first"
    )]
    NeedsMigration {
        /// Version in the file.
        found: u32,
        /// Version of this build.
        latest: u32,
    },
    /// Contract exists but the implementation is pending.
    #[error("storage operation {0} not implemented")]
    NotImplemented(&'static str),
    /// A media file could not be read or written (`fc-app` maps it to `IO_FILE`).
    #[error("file {path}: {message}")]
    File {
        /// File path.
        path: String,
        /// OS error text.
        message: String,
    },
    /// Any other SQLite error.
    #[error("sqlite: {0}")]
    Sqlite(rusqlite::Error),
}

impl From<rusqlite::Error> for StorageError {
    /// Classifies SQLite errors: busy/locked → [`StorageError::Busy`], constraint violations →
    /// [`StorageError::Constraint`], decode failures → [`StorageError::Corrupt`].
    fn from(e: rusqlite::Error) -> Self {
        in_table(e, "unknown")
    }
}

/// Like `From<rusqlite::Error>` but names the table for decode failures.
pub(crate) fn in_table(e: rusqlite::Error, table: &'static str) -> StorageError {
    use rusqlite::ErrorCode as C;
    match &e {
        rusqlite::Error::SqliteFailure(f, msg) => match f.code {
            C::DatabaseBusy | C::DatabaseLocked => StorageError::Busy,
            C::ConstraintViolation => StorageError::Constraint {
                constraint: constraint_name(msg.as_deref().unwrap_or_default()),
            },
            _ => StorageError::Sqlite(e),
        },
        rusqlite::Error::FromSqlConversionFailure(..)
        | rusqlite::Error::InvalidColumnType(..)
        | rusqlite::Error::IntegralValueOutOfRange(..) => StorageError::Corrupt {
            table,
            message: e.to_string(),
        },
        _ => StorageError::Sqlite(e),
    }
}

/// Extracts a stable constraint name from an SQLite message:
/// `UNIQUE constraint failed: index 'decks_sibling_name'` → `decks_sibling_name`,
/// `UNIQUE constraint failed: tags.name` → `tags.name`, `FOREIGN KEY constraint failed` →
/// `foreign_key`, `CHECK constraint failed: <expr>` → `check: <expr>`.
pub(crate) fn constraint_name(msg: &str) -> String {
    if msg.starts_with("FOREIGN KEY") {
        return "foreign_key".to_owned();
    }
    let Some((head, tail)) = msg.split_once(": ") else {
        return msg.to_owned();
    };
    let tail = tail.trim();
    if let Some(name) = tail
        .strip_prefix("index '")
        .and_then(|s| s.strip_suffix('\''))
    {
        return name.to_owned();
    }
    if head.starts_with("CHECK") {
        return format!("check: {tail}");
    }
    tail.to_owned()
}
