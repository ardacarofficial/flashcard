//! Store and transaction handles.
//!
//! ```text
//! let store: Arc<dyn Store> = Arc::new(SqliteStore::open(path)?);
//! let mut tx = store.write()?;          // BEGIN IMMEDIATE
//! tx.insert_deck(&deck)?;               // any *Write method
//! tx.append_oplog(&entry)?;
//! tx.commit()?;                         // drop without commit = ROLLBACK
//! ```
//!
//! ## Connection model
//!
//! * **One writer** connection behind a `Mutex`. A write transaction holds the guard for its
//!   whole life and issues `BEGIN IMMEDIATE` / `COMMIT` manually; dropping it without commit
//!   issues `ROLLBACK`. Commands are serialized in-process by the mutex and across processes
//!   by SQLite's write lock (+ `busy_timeout`).
//! * **A fixed pool of [`READ_POOL_SIZE`] reader connections** (`query_only = ON`), opened
//!   eagerly. A read transaction takes a free one (`BEGIN DEFERRED`, the snapshot starts at
//!   the first read) and gives it back on drop. WAL lets readers run while the writer writes.
//! * **In-memory databases** cannot be shared between connections, so reads use the writer
//!   connection (a read and a write cannot be open at the same time on one thread there).
//! * Acquiring a connection waits up to [`migrate::BUSY_TIMEOUT_MS`] and then fails with
//!   [`StorageError::Busy`] instead of dead-locking (e.g. a read opened while the same thread
//!   holds the in-memory writer).
//!
//! * **WAL checkpoints off the write path** (v1.4 Z2): a file-backed store runs a
//!   [`Checkpointer`] thread with its own connection. SQLite's own auto-checkpoint ran inside
//!   the commit that crossed 1 000 WAL pages — copying pages and syncing the WAL and the
//!   database file under the writer mutex: every ~15th Subtitle capture paid it (Linux
//!   ~12 ms, a Windows CI disk up to 2 s, stalling every command behind it). Now the writer's
//!   WAL hook only signals the thread when the WAL reaches [`BACKGROUND_CHECKPOINT_PAGES`]
//!   (the same point as before), and the thread runs a `PASSIVE` checkpoint (never blocks
//!   readers or the writer). **Not earlier:** after a complete checkpoint the next commit
//!   starts the WAL over and syncs its header (SQLite's rule, `walFrames`), so checkpointing
//!   after every burst put a WAL sync into the first command after each pause (seen with
//!   strace; on Windows CI it pushed a plugin hook's storage write past its deadline). The
//!   writer still checkpoints itself at [`SAFETY_CHECKPOINT_PAGES`] if the thread falls
//!   behind.
//!
//! Why not an `r2d2`-style pool: one more dependency for what is, on a desktop app, a fixed
//! handful of connections; the scheme above is ~50 lines plus the checkpoint thread.

use crate::error::{StorageError, StorageResult};
use crate::migrate;
use crate::repo::{Reads, Writes};
use crate::sqlite::{ReadMode, SqliteTx, WriteMode};
use rusqlite::{Connection, OpenFlags};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, TryLockError};
use std::time::{Duration, Instant};

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

/// Number of reader connections of a file-backed [`SqliteStore`].
pub const READ_POOL_SIZE: usize = 4;

/// SQLite-backed [`Store`]. See the module docs for the connection model.
pub struct SqliteStore {
    writer: Mutex<Connection>,
    readers: Vec<Mutex<Connection>>,
    path: Option<PathBuf>,
    backup: Option<PathBuf>,
    timeout: Duration,
    /// Background WAL checkpoints (file-backed stores; `None` in memory or when its
    /// connection could not be opened — SQLite's auto-checkpoint then stays as it was).
    checkpointer: Option<Checkpointer>,
}

/// WAL size (pages) at which the background thread checkpoints: SQLite's own default, so the
/// WAL starts over (and its header is synced by a commit) no more often than before.
pub const BACKGROUND_CHECKPOINT_PAGES: i32 = 1_000;

/// WAL size (pages) at which the writer still checkpoints itself, inside the commit as SQLite
/// did: reached only if the background thread falls behind.
pub const SAFETY_CHECKPOINT_PAGES: i32 = 20_000;

#[derive(Default)]
struct CheckpointState {
    /// The WAL reached [`BACKGROUND_CHECKPOINT_PAGES`] since the last checkpoint started.
    dirty: bool,
    stop: bool,
    /// Checkpoints run (tests).
    runs: u64,
    /// The thread is inside `PRAGMA wal_checkpoint` (tests: crash/close during a checkpoint).
    #[cfg(test)]
    in_progress: bool,
}

/// The WAL checkpoint thread of a file-backed store (see the module docs).
struct Checkpointer {
    state: Arc<(Mutex<CheckpointState>, Condvar)>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Checkpointer {
    /// Starts the thread on its own connection to `path` and hooks `writer`'s WAL to it (the
    /// hook replaces SQLite's auto-checkpoint). `None` (nothing changed) when the connection
    /// cannot be opened.
    fn start(path: &Path, writer: &mut Connection) -> Option<Self> {
        let conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .ok()?;
        conn.pragma_update(None, "busy_timeout", migrate::BUSY_TIMEOUT_MS)
            .ok()?;
        let state: Arc<(Mutex<CheckpointState>, Condvar)> = Arc::default();
        let theirs = state.clone();
        let thread = std::thread::Builder::new()
            .name("fc-wal-checkpoint".into())
            .spawn(move || checkpoint_loop(&conn, &theirs))
            .ok()?;
        let hooked = state.clone();
        // Called after each commit with the WAL's size in pages.
        let hook = writer.wal_hook(Some(move |wal: &mut rusqlite::hooks::Wal, pages: i32| {
            if pages >= SAFETY_CHECKPOINT_PAGES {
                // The thread fell behind: as SQLite's auto-checkpoint would.
                let _ = wal.checkpoint();
            } else if pages >= BACKGROUND_CHECKPOINT_PAGES {
                let (m, c) = &*hooked;
                if let Ok(mut s) = m.lock() {
                    s.dirty = true;
                }
                c.notify_one();
            }
            Ok(())
        }));
        let mut me = Self {
            state,
            thread: Some(thread),
        };
        if hook.is_err() {
            let _ = writer.pragma_update(None, "wal_autocheckpoint", 1_000);
            me.stop();
            return None;
        }
        Some(me)
    }

    fn stop(&mut self) {
        let (m, c) = &*self.state;
        if let Ok(mut s) = m.lock() {
            s.stop = true;
        }
        c.notify_all();
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }

    fn runs(&self) -> u64 {
        self.state.0.lock().map_or(0, |s| s.runs)
    }

    #[cfg(test)]
    fn probe(&self) -> CheckpointProbe {
        CheckpointProbe(Arc::downgrade(&self.state))
    }
}

/// Watches a store's checkpoint thread without borrowing the store (tests).
#[cfg(test)]
#[derive(Clone)]
pub(crate) struct CheckpointProbe(std::sync::Weak<(Mutex<CheckpointState>, Condvar)>);

#[cfg(test)]
impl CheckpointProbe {
    /// The thread is inside `PRAGMA wal_checkpoint` now (`false` once the store is closed).
    pub(crate) fn in_progress(&self) -> bool {
        self.0
            .upgrade()
            .is_some_and(|st| st.0.lock().is_ok_and(|s| s.in_progress))
    }

    /// Checkpoints completed so far (0 once the store is closed).
    pub(crate) fn runs(&self) -> u64 {
        self.0
            .upgrade()
            .map_or(0, |st| st.0.lock().map_or(0, |s| s.runs))
    }

    /// Nothing holds the state any more: the store, the thread (its closure owned a clone)
    /// and the writer connection (its WAL hook owned one, freed when it closes) are all gone.
    pub(crate) fn released(&self) -> bool {
        self.0.strong_count() == 0
    }
}

/// Waits for the writer's signal, checkpoints (`PASSIVE`: copies what no reader still needs,
/// never waits for or blocks other connections); until told to stop.
fn checkpoint_loop(conn: &Connection, state: &(Mutex<CheckpointState>, Condvar)) {
    let (m, c) = state;
    loop {
        {
            let Ok(mut s) = m.lock() else { return };
            while !s.dirty && !s.stop {
                s = match c.wait(s) {
                    Ok(s) => s,
                    Err(_) => return,
                };
            }
            if s.stop {
                return;
            }
            s.dirty = false;
            #[cfg(test)]
            {
                s.in_progress = true;
            }
        }
        // A failure (busy, I/O) is retried at the next commit; the writer's safety net
        // bounds the WAL meanwhile.
        let _ = conn.query_row("PRAGMA wal_checkpoint(PASSIVE)", [], |_| Ok(()));
        if let Ok(mut s) = m.lock() {
            s.runs += 1;
            #[cfg(test)]
            {
                s.in_progress = false;
            }
        }
    }
}

impl SqliteStore {
    /// Opens (creating if needed) the database file, configures the connection (WAL, foreign
    /// keys, …), takes a snapshot copy if an existing database is about to be migrated
    /// (docs/03 §15; see [`SqliteStore::migration_backup`]), runs pending migrations and opens
    /// the reader pool.
    pub fn open(path: &Path) -> StorageResult<Self> {
        Self::open_with_migrations(path, migrate::MIGRATIONS)
    }

    /// [`SqliteStore::open`] with an explicit migration list (tests of upgrade paths).
    pub(crate) fn open_with_migrations(
        path: &Path,
        migrations: &[migrate::Migration],
    ) -> StorageResult<Self> {
        let mut conn = Connection::open(path)?;
        migrate::configure_connection(&conn)?;
        crate::interrupt::install(&mut conn)?;
        #[cfg(feature = "crash-points")]
        crate::crash_points::install(&mut conn);
        let mut backup = snapshot_before_migrating(&mut conn, path, migrations)?;
        crate::media::export_legacy_blobs(&conn, path)?;
        let applied = migrate::migrate_with(&mut conn, migrations)?;
        if applied.is_empty() {
            // Another process migrated while this one waited: its snapshot is the one that
            // counts, this copy describes no migration of ours.
            if let Some(extra) = backup.take() {
                let _ = std::fs::remove_file(extra);
            }
        }
        let readers = (0..READ_POOL_SIZE)
            .map(|_| {
                let mut r = Connection::open_with_flags(
                    path,
                    OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
                )?;
                migrate::configure_reader(&r)?;
                crate::interrupt::install(&mut r)?;
                Ok(Mutex::new(r))
            })
            .collect::<StorageResult<Vec<_>>>()?;
        let checkpointer = Checkpointer::start(path, &mut conn);
        Ok(Self {
            writer: Mutex::new(conn),
            readers,
            path: Some(path.to_owned()),
            backup,
            timeout: Duration::from_millis(u64::from(migrate::BUSY_TIMEOUT_MS)),
            checkpointer,
        })
    }

    /// Opens an existing database **without migrating or snapshotting it**: for secondary
    /// processes (`fc mcp serve`) that must not rewrite a collection the app owns. Fails with
    /// [`StorageError::NeedsMigration`] when migrations are pending and with
    /// [`StorageError::TooNew`] when the file is ahead of this build; never creates the file.
    pub fn open_current(path: &Path) -> StorageResult<Self> {
        let mut conn = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )?;
        migrate::configure_connection(&conn)?;
        #[cfg(feature = "crash-points")]
        crate::crash_points::install(&mut conn);
        let latest = migrate::latest_version();
        let found = migrate::current_version(&conn)?;
        if found > latest {
            return Err(StorageError::TooNew {
                found,
                supported: latest,
            });
        }
        // Also catches an edited migration (checksum) and gaps, without applying anything.
        if !migrate::pending_versions(&mut conn)?.is_empty() {
            return Err(StorageError::NeedsMigration { found, latest });
        }
        let readers = (0..READ_POOL_SIZE)
            .map(|_| {
                let r = Connection::open_with_flags(
                    path,
                    OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
                )?;
                migrate::configure_reader(&r)?;
                Ok(Mutex::new(r))
            })
            .collect::<StorageResult<Vec<_>>>()?;
        let checkpointer = Checkpointer::start(path, &mut conn);
        Ok(Self {
            writer: Mutex::new(conn),
            readers,
            path: Some(path.to_owned()),
            backup: None,
            timeout: Duration::from_millis(u64::from(migrate::BUSY_TIMEOUT_MS)),
            checkpointer,
        })
    }

    /// Opens a private in-memory database (tests, CLI dry runs). Reads share the writer
    /// connection.
    pub fn open_in_memory() -> StorageResult<Self> {
        let mut conn = Connection::open_in_memory()?;
        migrate::configure_connection(&conn)?;
        crate::interrupt::install(&mut conn)?;
        migrate::migrate(&mut conn)?;
        Ok(Self {
            writer: Mutex::new(conn),
            readers: Vec::new(),
            path: None,
            backup: None,
            timeout: Duration::from_millis(u64::from(migrate::BUSY_TIMEOUT_MS)),
            checkpointer: None,
        })
    }

    /// Database file path (`None` for in-memory).
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    /// Snapshot file written by [`SqliteStore::open`] before migrating an existing database
    /// (`None` if nothing was migrated).
    pub fn migration_backup(&self) -> Option<&Path> {
        self.backup.as_deref()
    }

    /// Background checkpoints run so far (`None` without the thread; tests).
    pub fn checkpoints_run(&self) -> Option<u64> {
        self.checkpointer.as_ref().map(Checkpointer::runs)
    }

    /// A handle on the checkpoint thread's state that outlives a borrow of the store (`None`
    /// without the thread; tests).
    #[cfg(test)]
    pub(crate) fn checkpoint_probe(&self) -> Option<CheckpointProbe> {
        self.checkpointer.as_ref().map(Checkpointer::probe)
    }

    fn reader(&self) -> StorageResult<MutexGuard<'_, Connection>> {
        if self.readers.is_empty() {
            return lock_within(&self.writer, self.timeout);
        }
        let start = Instant::now();
        let mut wait = Duration::from_micros(100);
        loop {
            for slot in &self.readers {
                match slot.try_lock() {
                    Ok(g) => return Ok(g),
                    Err(TryLockError::Poisoned(p)) => return Ok(p.into_inner()),
                    Err(TryLockError::WouldBlock) => {}
                }
            }
            if start.elapsed() >= self.timeout {
                return Err(StorageError::Busy);
            }
            std::thread::sleep(wait);
            wait = (wait * 2).min(Duration::from_millis(10));
        }
    }
}

impl Store for SqliteStore {
    fn read(&self) -> StorageResult<Box<dyn ReadTx + '_>> {
        let conn = self.reader()?;
        Ok(Box::new(SqliteTx::<ReadMode>::begin(conn)?))
    }

    fn write(&self) -> StorageResult<Box<dyn WriteTx + '_>> {
        let conn = lock_within(&self.writer, self.timeout)?;
        Ok(Box::new(SqliteTx::<WriteMode>::begin(conn)?))
    }

    fn schema_version(&self) -> StorageResult<u32> {
        let conn = lock_within(&self.writer, self.timeout)?;
        migrate::current_version(&conn)
    }
}

impl Drop for SqliteStore {
    fn drop(&mut self) {
        // The thread's connection closes before the writer's (the last close checkpoints
        // and removes the WAL).
        if let Some(mut c) = self.checkpointer.take() {
            c.stop();
        }
        // Recommended before closing: lets SQLite refresh planner statistics if useful.
        if let Ok(conn) = self.writer.try_lock() {
            let _ = conn.execute_batch("PRAGMA optimize;");
        }
    }
}

/// Locks `m`, waiting at most `timeout`. A poisoned mutex is recovered (the transaction
/// guard rolls back any half-finished transaction on next `begin`).
fn lock_within(
    m: &Mutex<Connection>,
    timeout: Duration,
) -> StorageResult<MutexGuard<'_, Connection>> {
    let start = Instant::now();
    let mut wait = Duration::from_micros(100);
    loop {
        match m.try_lock() {
            Ok(g) => return Ok(g),
            Err(TryLockError::Poisoned(p)) => return Ok(p.into_inner()),
            Err(TryLockError::WouldBlock) => {
                if start.elapsed() >= timeout {
                    return Err(StorageError::Busy);
                }
                std::thread::sleep(wait);
                wait = (wait * 2).min(Duration::from_millis(10));
            }
        }
    }
}

/// docs/03 §15: before migrating an existing database, copy it next to itself with
/// `VACUUM INTO` (`<file>.pre-v<from>-to-v<to>[-n].bak`). Fresh databases are not copied.
fn snapshot_before_migrating(
    conn: &mut Connection,
    path: &Path,
    migrations: &[migrate::Migration],
) -> StorageResult<Option<PathBuf>> {
    let from = migrate::current_version(conn)?;
    if from == 0 {
        return Ok(None);
    }
    let pending = migrate::pending_versions_with(conn, migrations)?;
    let Some(to) = pending.last() else {
        return Ok(None);
    };
    let file = path
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_else(|| "collection.db".to_owned());
    let mut n = 0u32;
    loop {
        let suffix = if n == 0 {
            String::new()
        } else {
            format!("-{n}")
        };
        let candidate = path.with_file_name(format!("{file}.pre-v{from}-to-v{to}{suffix}.bak"));
        if candidate.exists() {
            n += 1;
            continue;
        }
        match conn.execute("VACUUM INTO ?1", [candidate.to_string_lossy().as_ref()]) {
            Ok(_) => return Ok(Some(candidate)),
            // Another process opening at the same moment took this name first (`VACUUM INTO`
            // refuses an existing file): the next one.
            Err(_) if candidate.exists() && n < 100 => n += 1,
            Err(e) => return Err(e.into()),
        }
    }
}
