//! Store and transaction handles.
//!
//! ```text
//! let store: Arc<dyn Store> = Arc::new(SqliteStore::open(path)?);
//! let mut tx = store.write()?;          // BEGIN IMMEDIATE
//! tx.insert_deck(&deck)?;               // any *Write method
//! tx.append_oplog(&entry)?;
//! tx.commit()?;                         // drop without commit = ROLLBACK
//! ```

use crate::error::{StorageError, StorageResult};
use crate::migrate;
use crate::repo::{Reads, Writes};
use rusqlite::Connection;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// A read-only snapshot (deferred transaction on a read connection; WAL lets it run alongside
/// the writer).
pub trait ReadTx: Reads {}

/// A write transaction (`BEGIN IMMEDIATE`; one writer at a time). Dropping it without
/// [`WriteTx::commit`] rolls back.
pub trait WriteTx: Writes {
    /// Commits.
    fn commit(self: Box<Self>) -> StorageResult<()>;
}

/// The storage entry point used by `fc-app`.
pub trait Store: Send + Sync {
    /// Opens a read snapshot.
    fn read(&self) -> StorageResult<Box<dyn ReadTx + '_>>;
    /// Opens a write transaction (blocks until the writer is free, up to the busy timeout).
    fn write(&self) -> StorageResult<Box<dyn WriteTx + '_>>;
    /// Current schema version.
    fn schema_version(&self) -> StorageResult<u32>;
}

/// SQLite-backed [`Store`].
///
/// v0.1 contract skeleton: opening, configuring and migrating work; transactions and
/// repositories are implemented by the storage lane (L2). Expected shape: one writer
/// connection behind a `Mutex`, a small pool of read connections, transactions holding the
/// connection guard and issuing `BEGIN IMMEDIATE`/`COMMIT`/`ROLLBACK` manually.
pub struct SqliteStore {
    writer: Mutex<Connection>,
    path: Option<PathBuf>,
}

impl SqliteStore {
    /// Opens (creating if needed) the database file, configures the connection (WAL, foreign
    /// keys) and runs pending migrations.
    pub fn open(path: &Path) -> StorageResult<Self> {
        let mut conn = Connection::open(path)?;
        migrate::configure_connection(&conn)?;
        migrate::migrate(&mut conn)?;
        Ok(Self {
            writer: Mutex::new(conn),
            path: Some(path.to_owned()),
        })
    }

    /// Opens a private in-memory database (tests, CLI dry runs).
    pub fn open_in_memory() -> StorageResult<Self> {
        let mut conn = Connection::open_in_memory()?;
        migrate::configure_connection(&conn)?;
        migrate::migrate(&mut conn)?;
        Ok(Self {
            writer: Mutex::new(conn),
            path: None,
        })
    }

    /// Database file path (`None` for in-memory).
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }
}

impl Store for SqliteStore {
    fn read(&self) -> StorageResult<Box<dyn ReadTx + '_>> {
        Err(StorageError::NotImplemented("SqliteStore::read"))
    }

    fn write(&self) -> StorageResult<Box<dyn WriteTx + '_>> {
        Err(StorageError::NotImplemented("SqliteStore::write"))
    }

    fn schema_version(&self) -> StorageResult<u32> {
        let conn = self.writer.lock().map_err(|_| StorageError::Busy)?;
        migrate::current_version(&conn)
    }
}
