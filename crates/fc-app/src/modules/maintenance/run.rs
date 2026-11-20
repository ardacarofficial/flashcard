//! Run marker: tells the next start whether this run ended cleanly.
//!
//! The desktop app calls [`RunGuard::begin`] before it opens the collection: it reads what the
//! previous run left (`run.json` still there = that run never finished: crash, kill, power
//! loss) and writes its own `run.json`. A normal exit calls [`RunGuard::finish`], which records
//! `last_run.json` (clean) and removes `run.json`. Only the app that owns the collection uses
//! it — the CLI and `fc mcp serve` are secondary processes.

use fc_core::Timestamp;
use serde::{Deserialize, Serialize};
use std::io;
use std::path::{Path, PathBuf};

/// Marker of the running app.
pub const RUN_FILE: &str = "run.json";
/// Record of the last run that ended cleanly.
pub const LAST_RUN_FILE: &str = "last_run.json";

/// What a run marker holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunInfo {
    /// When the run started.
    pub started_at: Timestamp,
    /// App version.
    pub version: String,
    /// Process id (diagnostics only).
    #[serde(default)]
    pub pid: u32,
    /// Whether it ended cleanly.
    #[serde(default)]
    pub clean: bool,
}

/// What [`RunGuard::begin`] found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunStart {
    /// This run.
    pub current: RunInfo,
    /// The previous run, if one was recorded (`clean = false`: it did not end normally).
    pub previous: Option<RunInfo>,
}

impl RunStart {
    /// Whether the previous run did not end normally.
    pub fn previous_unclean(&self) -> bool {
        self.previous.as_ref().is_some_and(|p| !p.clean)
    }
}

/// The marker of this run (see the module docs). Not removed on drop: only an explicit
/// [`RunGuard::finish`] means "ended cleanly".
#[derive(Debug)]
pub struct RunGuard {
    dir: PathBuf,
    info: RunInfo,
}

fn read_info(path: &Path) -> Option<RunInfo> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// Writes `value` to `path` through a temporary file and a rename (never a half file).
fn write_atomic(path: &Path, value: &RunInfo) -> io::Result<()> {
    let text = serde_json::to_string(value).map_err(io::Error::other)?;
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)
}

impl RunGuard {
    /// Starts a run in `dir` (the collection's folder).
    pub fn begin(dir: &Path, version: &str, now: Timestamp) -> io::Result<(Self, RunStart)> {
        std::fs::create_dir_all(dir)?;
        let run = dir.join(RUN_FILE);
        let previous = if run.exists() {
            // Still there: the previous run never finished. An unreadable marker (power loss
            // mid-write) still means that.
            Some(read_info(&run).map_or(
                RunInfo {
                    started_at: Timestamp::EPOCH,
                    version: String::new(),
                    pid: 0,
                    clean: false,
                },
                |i| RunInfo { clean: false, ..i },
            ))
        } else {
            read_info(&dir.join(LAST_RUN_FILE)).map(|i| RunInfo { clean: true, ..i })
        };
        let info = RunInfo {
            started_at: now,
            version: version.to_owned(),
            pid: std::process::id(),
            clean: false,
        };
        write_atomic(&run, &info)?;
        Ok((
            Self {
                dir: dir.to_path_buf(),
                info: info.clone(),
            },
            RunStart {
                current: info,
                previous,
            },
        ))
    }

    /// Ends the run cleanly.
    pub fn finish(self) -> io::Result<()> {
        write_atomic(
            &self.dir.join(LAST_RUN_FILE),
            &RunInfo {
                clean: true,
                ..self.info.clone()
            },
        )?;
        match std::fs::remove_file(self.dir.join(RUN_FILE)) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    }

    /// This run.
    pub fn info(&self) -> &RunInfo {
        &self.info
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_run_that_never_finished_is_unclean_next_time() {
        let dir = tempfile::tempdir().unwrap();
        let (_g, first) = RunGuard::begin(dir.path(), "1.0.0", Timestamp(10)).unwrap();
        assert_eq!(first.previous, None);
        assert!(!first.previous_unclean());
        // No finish(): the process "died". (Dropping the guard does not count as clean.)
        let (g, second) = RunGuard::begin(dir.path(), "1.0.1", Timestamp(20)).unwrap();
        let prev = second.previous.clone().unwrap();
        assert!(second.previous_unclean());
        assert_eq!((prev.started_at, prev.version.as_str()), (Timestamp(10), "1.0.0"));
        g.finish().unwrap();
        assert!(!dir.path().join(RUN_FILE).exists());
        let (_g, third) = RunGuard::begin(dir.path(), "1.0.1", Timestamp(30)).unwrap();
        assert!(!third.previous_unclean());
        assert_eq!(third.previous.unwrap().started_at, Timestamp(20));
    }

    #[test]
    fn a_garbled_marker_still_counts_as_unclean() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(RUN_FILE), "{\"started_at\": 1").unwrap();
        let (_g, start) = RunGuard::begin(dir.path(), "1.0.0", Timestamp(5)).unwrap();
        assert!(start.previous_unclean());
    }
}
