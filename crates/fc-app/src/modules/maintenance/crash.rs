//! Crash reports: a panic hook writes one redacted text file per crash into
//! `<data dir>/crash/` (format: `fc_core::maintenance::crash`). Nothing is sent anywhere; the
//! next start offers recovery ([`super::recovery`]) and the user may export the reports with
//! the logs.

use super::logging::{FileLog, Level};
use fc_core::maintenance::crash::{parse_header, CrashHeader, CrashReport};
use fc_core::maintenance::file_stamp;
use fc_core::maintenance::redact::Redactor;
use fc_core::{Clock, Timestamp};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Crash reports kept (older ones are removed when a new one is written).
pub const KEEP_REPORTS: usize = 20;
/// File name prefix.
pub const PREFIX: &str = "crash-";

/// One report on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReportFile {
    /// File name.
    pub file: String,
    /// Its header.
    pub header: CrashHeader,
}

/// Writes `report` into `dir` (created if needed), then prunes to [`KEEP_REPORTS`]. Returns
/// the file written.
pub fn write_report(dir: &Path, report: &CrashReport, redactor: &Redactor) -> io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let stamp = file_stamp(report.at);
    let mut path = dir.join(format!("{PREFIX}{stamp}.txt"));
    let mut n = 1;
    while path.exists() {
        path = dir.join(format!("{PREFIX}{stamp}-{n}.txt"));
        n += 1;
    }
    std::fs::write(&path, report.to_text(redactor))?;
    prune(dir, KEEP_REPORTS);
    Ok(path)
}

/// Reports in `dir`, newest first (file names sort by time).
pub fn list_reports(dir: &Path) -> Vec<ReportFile> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out: Vec<ReportFile> = entries
        .filter_map(Result::ok)
        .filter_map(|e| {
            let file = e.file_name().to_string_lossy().into_owned();
            if !file.starts_with(PREFIX) || !file.ends_with(".txt") {
                return None;
            }
            // Headers are short: read at most the first 8 KiB.
            let text = read_prefix(&e.path(), 8 * 1024).ok()?;
            Some(ReportFile {
                header: parse_header(&text)?,
                file,
            })
        })
        .collect();
    // Newest first; reports of the same millisecond carry a `-<n>` suffix (longer = later).
    out.sort_by(|a, b| {
        (&b.header.time, b.file.len(), &b.file).cmp(&(&a.header.time, a.file.len(), &a.file))
    });
    out
}

fn read_prefix(path: &Path, max: usize) -> io::Result<String> {
    use std::io::Read;
    let mut buf = Vec::new();
    std::fs::File::open(path)?
        .take(u64::try_from(max).unwrap_or(u64::MAX))
        .read_to_end(&mut buf)?;
    Ok(String::from_utf8_lossy(&buf).into_owned())
}

/// Removes all but the newest `keep` reports. Returns how many were removed.
pub fn prune(dir: &Path, keep: usize) -> u32 {
    let mut removed = 0;
    for r in list_reports(dir).into_iter().skip(keep) {
        if std::fs::remove_file(dir.join(&r.file)).is_ok() {
            removed += 1;
        }
    }
    removed
}

/// Reports written at or after `since` (the previous run's start), newest first.
pub fn reports_since(dir: &Path, since: Timestamp) -> Vec<ReportFile> {
    let from = fc_core::maintenance::iso_datetime(since);
    list_reports(dir)
        .into_iter()
        .filter(|r| r.header.time >= from)
        .collect()
}

/// `<os> <arch>` of this build.
pub fn os_label() -> String {
    format!("{} {}", std::env::consts::OS, std::env::consts::ARCH)
}

/// Builds the report of a panic (message, location, thread, backtrace).
pub fn report_of(info: &std::panic::PanicHookInfo<'_>, version: &str, at: Timestamp) -> CrashReport {
    let payload = info.payload();
    let message = payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "panic with a non-text payload".to_owned());
    CrashReport {
        version: version.to_owned(),
        at,
        os: os_label(),
        thread: std::thread::current()
            .name()
            .unwrap_or("<unnamed>")
            .to_owned(),
        location: info
            .location()
            .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column())),
        message,
        backtrace: std::backtrace::Backtrace::force_capture().to_string(),
    }
}

/// Installs the process-wide panic hook: writes a crash report into `dir`, logs the panic
/// (when a log is given) and flushes it, then runs the previous hook (stderr in debug builds).
/// The desktop app calls this once at start.
pub fn install_panic_hook(
    dir: PathBuf,
    version: String,
    clock: Arc<dyn Clock>,
    log: Option<Arc<FileLog>>,
) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let report = report_of(info, &version, clock.now());
        let redactor = log
            .as_ref()
            .map_or_else(|| Arc::new(Redactor::new()), |l| l.redactor());
        let written = write_report(&dir, &report, &redactor);
        if let Some(log) = &log {
            let where_ = report.location.clone().unwrap_or_default();
            let file = written
                .as_ref()
                .map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())
                .unwrap_or_else(|e| format!("not written: {e}"));
            log.write_line(
                Level::Error,
                "fc_app::crash",
                &format!("panic at {where_}: {} (report {file})", report.message),
            );
            log.flush();
        }
        previous(info);
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report(at: i64, message: &str) -> CrashReport {
        CrashReport {
            version: "1.0.0".into(),
            at: Timestamp(at),
            os: os_label(),
            thread: "main".into(),
            location: Some("src/x.rs:1:1".into()),
            message: message.into(),
            backtrace: String::new(),
        }
    }

    #[test]
    fn reports_are_listed_newest_first_pruned_and_filtered_by_run() {
        let dir = tempfile::tempdir().unwrap();
        let red = Redactor::new();
        for i in 0..25 {
            write_report(dir.path(), &report(1_000_000 + i * 1000, &format!("boom {i}")), &red).unwrap();
        }
        // Same millisecond twice: no overwrite.
        write_report(dir.path(), &report(1_000_000 + 24_000, "again"), &red).unwrap();
        let all = list_reports(dir.path());
        assert_eq!(all.len(), KEEP_REPORTS);
        assert_eq!(all[0].header.message, "again");
        assert_eq!(all[1].header.message, "boom 24");
        let recent = reports_since(dir.path(), Timestamp(1_000_000 + 23_000));
        assert_eq!(recent.len(), 3);
        assert_eq!(prune(dir.path(), 5), 15);
        assert_eq!(list_reports(dir.path()).len(), 5);
        // Other files are ignored.
        std::fs::write(dir.path().join("crash-notes.txt"), "hello").unwrap();
        std::fs::write(dir.path().join("readme.md"), "x").unwrap();
        assert_eq!(list_reports(dir.path()).len(), 5);
    }
}
