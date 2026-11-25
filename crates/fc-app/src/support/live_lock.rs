//! "Is the process that started this still running?" (v1.4.2 K1, review finding 9).
//!
//! Several processes may work on one collection: the desktop app, `fc` commands, `fc api
//! serve`, `fc mcp serve`. The app's startup repairs what a crash left behind — it undoes
//! unfinished undo groups and marks running jobs interrupted — and must not do that to work
//! another **live** process is in the middle of.
//!
//! So whoever opens an undo group or runs a job holds an operating-system file lock for as
//! long as that work lives: `process-locks/<kind>-<id>.lock` next to the collection, locked
//! exclusively (`flock` on Unix, `LockFileEx` on Windows). The system releases the lock when
//! the process ends, however it ends — no process ids (they are reused), no timestamps (a
//! long import is not a dead one). Startup asks for the same lock: refused = the owner is
//! alive, leave its work alone; granted = the owner is gone (or finished), repair.
//!
//! Best effort by design: a collection without a folder (in memory), or a file system that
//! cannot lock, has no locks, and startup behaves as it did before — everything unfinished
//! counts as crashed.
//!
//! `File::try_lock` is in the standard library since Rust 1.89; the workspace states 1.86 as
//! its oldest compiler but builds with the pinned toolchain (`rust-toolchain.toml`), so the
//! lint about newer APIs is switched off here rather than raising the stated version, which
//! would turn on new lints across the whole code base.
#![allow(clippy::incompatible_msrv)]

use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};

/// Folder of the lock files, inside the collection's folder.
pub(crate) const DIR: &str = "process-locks";
/// Lock of an open undo group (`group-<batch id>.lock`).
pub(crate) const GROUP: &str = "group";
/// Lock of a running job (`job-<job id>.lock`).
pub(crate) const JOB: &str = "job";

/// A held lock. Dropping it releases the lock and removes its file.
#[derive(Debug)]
pub(crate) struct LiveLock {
    file: Option<File>,
    path: PathBuf,
}

/// What asking for somebody's lock found.
#[derive(Debug)]
pub(crate) enum Owner {
    /// Its lock is held: the owner is running.
    Alive,
    /// Nobody holds it. With a lock file present the lock is now ours, so no second process
    /// repairs the same work at the same time.
    Gone(Option<LiveLock>),
}

fn path_of(data_dir: &Path, kind: &str, id: &str) -> PathBuf {
    data_dir.join(DIR).join(format!("{kind}-{id}.lock"))
}

fn open(path: &Path, create: bool) -> std::io::Result<File> {
    // Read and write: `LockFileEx` needs one of them, and nothing is ever written.
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(create)
        .truncate(false)
        .open(path)
}

impl LiveLock {
    /// Takes the lock `kind`-`id` for the work this process starts now. `None` when there
    /// is no folder or the lock cannot be had (see the module docs).
    pub(crate) fn hold(data_dir: Option<&Path>, kind: &str, id: &str) -> Option<Self> {
        let path = path_of(data_dir?, kind, id);
        std::fs::create_dir_all(path.parent()?).ok()?;
        // A starting app sweeping stale files may hold a new file for an instant, or remove
        // it before it is locked: a few tries settle that.
        for _ in 0..50 {
            if let Ok(file) = open(&path, true) {
                match file.try_lock() {
                    Ok(()) if path.exists() => {
                        return Some(Self {
                            file: Some(file),
                            path,
                        })
                    }
                    Ok(()) | Err(TryLockError::WouldBlock) => {}
                    Err(TryLockError::Error(_)) => return None,
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        None
    }

    /// Whether the owner of `kind`-`id` is running.
    pub(crate) fn owner(data_dir: Option<&Path>, kind: &str, id: &str) -> Owner {
        match data_dir {
            Some(dir) => probe(path_of(dir, kind, id)),
            None => Owner::Gone(None),
        }
    }

    /// Ids of the `kind` locks live processes hold. Lock files nobody holds (their process
    /// crashed) are removed on the way.
    pub(crate) fn held(data_dir: Option<&Path>, kind: &str) -> Vec<String> {
        let Some(dir) = data_dir.map(|d| d.join(DIR)) else {
            return Vec::new();
        };
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return Vec::new();
        };
        let prefix = format!("{kind}-");
        entries
            .filter_map(Result::ok)
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().into_owned();
                let id = name
                    .strip_prefix(&prefix)?
                    .strip_suffix(".lock")?
                    .to_owned();
                matches!(probe(e.path()), Owner::Alive).then_some(id)
            })
            .collect()
    }
}

fn probe(path: PathBuf) -> Owner {
    let Ok(file) = open(&path, false) else {
        return Owner::Gone(None);
    };
    match file.try_lock() {
        Err(TryLockError::WouldBlock) => Owner::Alive,
        Ok(()) => Owner::Gone(Some(LiveLock {
            file: Some(file),
            path,
        })),
        Err(TryLockError::Error(_)) => Owner::Gone(None),
    }
}

impl Drop for LiveLock {
    fn drop(&mut self) {
        // Removed while still locked where the system allows it, so nobody finds a free
        // file of work that is over; closed first where it does not.
        let removed = std::fs::remove_file(&self.path).is_ok();
        drop(self.file.take());
        if !removed {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn a_held_lock_is_alive_and_a_dropped_or_missing_one_is_gone() {
        let dir = tempfile::tempdir().unwrap();
        let d = Some(dir.path());
        assert!(matches!(LiveLock::owner(d, GROUP, "a"), Owner::Gone(None)));
        let lock = LiveLock::hold(d, GROUP, "a").unwrap();
        assert!(matches!(LiveLock::owner(d, GROUP, "a"), Owner::Alive));
        assert_eq!(LiveLock::held(d, GROUP), vec!["a".to_owned()]);
        assert!(LiveLock::held(d, JOB).is_empty());
        drop(lock);
        assert!(matches!(LiveLock::owner(d, GROUP, "a"), Owner::Gone(None)));
        assert!(
            !path_of(dir.path(), GROUP, "a").exists(),
            "the file goes with the lock"
        );
    }

    #[test]
    fn a_file_left_by_a_dead_process_is_claimed_once_and_swept() {
        let dir = tempfile::tempdir().unwrap();
        let d = Some(dir.path());
        std::fs::create_dir_all(dir.path().join(DIR)).unwrap();
        // What a crashed process leaves: the file, no lock.
        std::fs::write(path_of(dir.path(), JOB, "dead"), "").unwrap();
        let Owner::Gone(Some(claim)) = LiveLock::owner(d, JOB, "dead") else {
            panic!("a free lock file is claimed");
        };
        // While one process repairs, a second one sees the work as taken.
        assert!(matches!(LiveLock::owner(d, JOB, "dead"), Owner::Alive));
        drop(claim);
        std::fs::write(path_of(dir.path(), JOB, "dead2"), "").unwrap();
        let live = LiveLock::hold(d, JOB, "live").unwrap();
        assert_eq!(LiveLock::held(d, JOB), vec!["live".to_owned()]);
        assert!(
            !path_of(dir.path(), JOB, "dead2").exists(),
            "stale files are swept"
        );
        drop(live);
    }

    #[test]
    fn no_folder_means_no_lock() {
        assert!(LiveLock::hold(None, GROUP, "a").is_none());
        assert!(matches!(
            LiveLock::owner(None, GROUP, "a"),
            Owner::Gone(None)
        ));
        assert!(LiveLock::held(None, GROUP).is_empty());
    }
}
