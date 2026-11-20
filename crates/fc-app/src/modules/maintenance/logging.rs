//! Local log files with rotation (docs/02 §34, docs/03 §18). Nothing leaves the machine; the
//! user can export them (`maintenance.logs.export`).
//!
//! * One file, `<log dir>/flashcard.log`, rotated by size: at `max_bytes` it becomes
//!   `flashcard.log.1` (older ones shift; `keep` are kept) — `fc_core::maintenance::rotation`.
//! * Every line is redacted **before** it is written (`fc_core::maintenance::redact`): the
//!   app never logs params, card content, keys or tokens on purpose; this catches what an
//!   error message or a library line carries anyway.
//! * Lines: `<UTC time> <LEVEL> <target>: <message>`.
//! * [`FileLog`] is usable on its own (tests, the smoke run); [`install`] makes it the
//!   process-wide `log` backend (the desktop app; Tauri's own `log` lines land there too).

use fc_core::maintenance::redact::Redactor;
use fc_core::maintenance::{iso_datetime, rotation};
use fc_core::{Clock, Timestamp};
use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};

/// Name of the current log file.
pub const LOG_FILE: &str = "flashcard.log";
/// Default size at which the file rotates.
pub const DEFAULT_MAX_BYTES: u64 = 1024 * 1024;
/// Default number of rotated files kept.
pub const DEFAULT_KEEP: usize = 4;

/// Severity (the `log` crate's, re-exported for callers without the dependency).
pub use log::Level;
pub use log::LevelFilter;

struct Open {
    file: Option<File>,
    size: u64,
}

/// A rotating, redacting log file.
pub struct FileLog {
    dir: PathBuf,
    open: Mutex<Open>,
    max_bytes: AtomicU64,
    keep: AtomicUsize,
    level: RwLock<LevelFilter>,
    redactor: RwLock<Arc<Redactor>>,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for FileLog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileLog").field("dir", &self.dir).finish()
    }
}

impl FileLog {
    /// Opens (creating) the log in `dir`, appending to the current file.
    pub fn open(dir: &Path, clock: Arc<dyn Clock>) -> io::Result<Arc<Self>> {
        std::fs::create_dir_all(dir)?;
        let log = Self {
            dir: dir.to_path_buf(),
            open: Mutex::new(Open {
                file: None,
                size: 0,
            }),
            max_bytes: AtomicU64::new(DEFAULT_MAX_BYTES),
            keep: AtomicUsize::new(DEFAULT_KEEP),
            level: RwLock::new(LevelFilter::Info),
            redactor: RwLock::new(Arc::new(Redactor::new())),
            clock,
        };
        {
            let mut open = log.lock();
            log.reopen(&mut open)?;
        }
        Ok(Arc::new(log))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Open> {
        self.open
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn reopen(&self, open: &mut Open) -> io::Result<()> {
        let path = self.dir.join(LOG_FILE);
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        open.size = file.metadata().map(|m| m.len()).unwrap_or(0);
        open.file = Some(file);
        Ok(())
    }

    /// Folder of the log files.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Rotation limits (`max_bytes` ≥ 16 KiB, `keep` ≥ 1).
    pub fn set_limits(&self, max_bytes: u64, keep: usize) {
        self.max_bytes.store(max_bytes.max(16 * 1024), Ordering::SeqCst);
        self.keep.store(keep.max(1), Ordering::SeqCst);
    }

    /// Most verbose level written.
    pub fn set_level(&self, level: LevelFilter) {
        if let Ok(mut l) = self.level.write() {
            *l = level;
        }
        // The `log` facade filters before calling the backend.
        if log::max_level() != LevelFilter::Off {
            log::set_max_level(level.max(LevelFilter::Warn));
        }
    }

    /// Current level.
    pub fn level(&self) -> LevelFilter {
        self.level.read().map_or(LevelFilter::Info, |l| *l)
    }

    /// Adds exact secrets to mask (e.g. API keys the app holds) on top of the built-in rules.
    pub fn add_secrets<I: IntoIterator<Item = String>>(&self, secrets: I) {
        if let Ok(mut r) = self.redactor.write() {
            *r = Arc::new((**r).clone().with_secrets(secrets));
        }
    }

    /// The redactor every line goes through.
    pub fn redactor(&self) -> Arc<Redactor> {
        self.redactor
            .read()
            .map_or_else(|_| Arc::new(Redactor::new()), |r| Arc::clone(&r))
    }

    /// Whether a line of `level` from `target` would be written: the app's own crates
    /// (`fc_*`) at the configured level, everything else (Tauri, libraries) at warnings.
    pub fn enabled(&self, level: Level, target: &str) -> bool {
        let max = if target.starts_with("fc_") {
            self.level()
        } else {
            self.level().min(LevelFilter::Warn)
        };
        level <= max
    }

    /// Writes one line (redacted), rotating first when the file is full.
    pub fn write_line(&self, level: Level, target: &str, message: &str) {
        let line = format!(
            "{} {:<5} {}: {}\n",
            iso_datetime(self.clock.now()),
            level,
            target,
            message.replace('\n', "\n    ")
        );
        let line = self.redactor().clean(&line);
        let mut open = self.lock();
        let incoming = u64::try_from(line.len()).unwrap_or(u64::MAX);
        if rotation::must_rotate(open.size, incoming, self.max_bytes.load(Ordering::SeqCst)) {
            // Best effort: logging must never take the app down.
            let _ = self.rotate(&mut open);
        }
        if open.file.is_none() {
            let _ = self.reopen(&mut open);
        }
        if let Some(f) = open.file.as_mut() {
            if f.write_all(line.as_bytes()).is_ok() {
                open.size += incoming;
            }
        }
    }

    fn rotate(&self, open: &mut Open) -> io::Result<()> {
        if let Some(f) = open.file.as_mut() {
            let _ = f.flush();
        }
        open.file = None;
        let plan = rotation::plan(LOG_FILE, self.keep.load(Ordering::SeqCst));
        match std::fs::remove_file(self.dir.join(&plan.delete)) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
            _ => {}
        }
        for (from, to) in &plan.renames {
            let from = self.dir.join(from);
            if from.exists() {
                std::fs::rename(&from, self.dir.join(to))?;
            }
        }
        self.reopen(open)
    }

    /// Flushes the current file.
    pub fn flush(&self) {
        if let Some(f) = self.lock().file.as_mut() {
            let _ = f.flush();
        }
    }

    /// Log files, current first, with their sizes.
    pub fn files(&self) -> Vec<(String, u64)> {
        list_files(&self.dir)
    }

    /// Time of the log's clock (crash reports use the same).
    pub fn now(&self) -> Timestamp {
        self.clock.now()
    }
}

/// Log files in `dir`, current first then `.1`, `.2`, … with sizes.
pub fn list_files(dir: &Path) -> Vec<(String, u64)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<(usize, String, u64)> = entries
        .filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let generation = rotation::generation(LOG_FILE, &name)?;
            let size = e.metadata().ok()?.len();
            Some((generation, name, size))
        })
        .collect();
    files.sort();
    files.into_iter().map(|(_, n, s)| (n, s)).collect()
}

/// `log` backend over a [`FileLog`].
struct Backend(Arc<FileLog>);

impl log::Log for Backend {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        self.0.enabled(metadata.level(), metadata.target())
    }

    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            self.0
                .write_line(record.level(), record.target(), &record.args().to_string());
        }
    }

    fn flush(&self) {
        self.0.flush();
    }
}

/// Makes `log` the process-wide backend of the `log` facade. Once per process; a second call
/// returns `false` and changes nothing.
pub fn install(log: Arc<FileLog>) -> bool {
    let level = log.level();
    if log::set_boxed_logger(Box::new(Backend(log))).is_err() {
        return false;
    }
    log::set_max_level(level.max(LevelFilter::Warn));
    true
}

/// Parses a level setting value (`error`, `warn`, `info`, `debug`).
pub fn parse_level(s: &str) -> Option<LevelFilter> {
    match s {
        "error" => Some(LevelFilter::Error),
        "warn" => Some(LevelFilter::Warn),
        "info" => Some(LevelFilter::Info),
        "debug" => Some(LevelFilter::Debug),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fc_core::time::FixedClock;

    fn log_in(dir: &Path) -> Arc<FileLog> {
        FileLog::open(dir, Arc::new(FixedClock::new(Timestamp(1_795_169_472_123)))).unwrap()
    }

    #[test]
    fn lines_are_redacted_before_they_reach_the_disk() {
        let dir = tempfile::tempdir().unwrap();
        let log = log_in(dir.path());
        log.add_secrets(["my-provider-key-without-shape".to_owned()]);
        log.write_line(Level::Error, "fc_app::ai", "request failed: api_key=sk-ant-api03-ABCDEFGHIJKLMNOPQRSTUVWX");
        log.write_line(Level::Warn, "fc_app::ai", "retrying with my-provider-key-without-shape");
        log.write_line(Level::Info, "fc_http", "Authorization: Bearer 0123456789abcdef0123456789abcdef");
        log.flush();
        let text = std::fs::read_to_string(dir.path().join(LOG_FILE)).unwrap();
        for secret in ["sk-ant", "my-provider-key", "0123456789abcdef0123"] {
            assert!(!text.contains(secret), "{secret} in {text}");
        }
        assert!(text.starts_with("2026-11-20T10:11:12.123Z ERROR fc_app::ai: request failed: api_key=[REDACTED]"));
        assert_eq!(text.lines().count(), 3);
    }

    #[test]
    fn files_rotate_by_size_and_keep_a_bounded_number() {
        let dir = tempfile::tempdir().unwrap();
        let log = log_in(dir.path());
        log.set_limits(16 * 1024, 2);
        let line = "x".repeat(1000);
        for i in 0..100 {
            log.write_line(Level::Info, "fc_app", &format!("{i} {line}"));
        }
        log.flush();
        let files = log.files();
        let names: Vec<&str> = files.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["flashcard.log", "flashcard.log.1", "flashcard.log.2"]);
        for (name, size) in &files {
            assert!(*size <= 16 * 1024, "{name} is {size} bytes");
        }
        // The newest line is in the current file, the oldest kept ones in `.2`.
        let current = std::fs::read_to_string(dir.path().join(LOG_FILE)).unwrap();
        assert!(current.contains(" 99 x"));
        let oldest = std::fs::read_to_string(dir.path().join("flashcard.log.2")).unwrap();
        assert!(!oldest.contains(" 99 x") && !oldest.contains(" 0 x"), "line 0 rotated away");
        // Reopening appends.
        drop(log);
        let again = log_in(dir.path());
        again.write_line(Level::Info, "fc_app", "after restart");
        again.flush();
        let current2 = std::fs::read_to_string(dir.path().join(LOG_FILE)).unwrap();
        assert!(current2.starts_with(&current));
    }

    #[test]
    fn levels_filter_the_app_and_libraries_separately() {
        let dir = tempfile::tempdir().unwrap();
        let log = log_in(dir.path());
        assert!(log.enabled(Level::Info, "fc_app::study"));
        assert!(!log.enabled(Level::Debug, "fc_app::study"));
        assert!(!log.enabled(Level::Info, "tauri::manager"));
        assert!(log.enabled(Level::Warn, "tauri::manager"));
        log.set_level(LevelFilter::Debug);
        assert!(log.enabled(Level::Debug, "fc_app::study"));
        assert!(!log.enabled(Level::Info, "wry"));
        log.set_level(LevelFilter::Error);
        assert!(!log.enabled(Level::Warn, "fc_app"));
        assert!(!log.enabled(Level::Warn, "tauri"));
        assert_eq!(parse_level("debug"), Some(LevelFilter::Debug));
        assert_eq!(parse_level("trace"), None);
    }
}
