//! Crash report text format. The desktop app's panic hook writes one file per crash
//! (`<data dir>/crash/crash-<stamp>.txt`); nothing is ever sent anywhere (docs/02 §34). The
//! app lists them ([`parse_header`]), offers recovery at the next start and puts them in the
//! exported log bundle — all already redacted ([`super::redact`]) when written.
//!
//! ```text
//! Flashcard crash report
//! version: 1.0.0
//! time: 2026-11-20T10:11:12.123Z
//! os: windows x86_64
//! thread: main
//! location: crates/fc-app/src/x.rs:10:5
//! message: index out of bounds\nsecond line
//!
//! backtrace:
//! …
//! ```

use super::iso_datetime;
use super::redact::Redactor;
use crate::Timestamp;

/// First line of every report (also how a report file is recognised).
pub const MAGIC: &str = "Flashcard crash report";
/// Longest message kept (characters); a panic message can embed arbitrary data.
pub const MAX_MESSAGE: usize = 2_000;
/// Longest backtrace kept (characters).
pub const MAX_BACKTRACE: usize = 30_000;

/// One crash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrashReport {
    /// App version.
    pub version: String,
    /// When it happened.
    pub at: Timestamp,
    /// `<os> <arch>`.
    pub os: String,
    /// Name of the thread that panicked (`<unnamed>` when it had none).
    pub thread: String,
    /// `file:line:column` of the panic, if known.
    pub location: Option<String>,
    /// Panic message.
    pub message: String,
    /// Backtrace text (may be empty).
    pub backtrace: String,
}

/// What a report's header says (listing without reading the backtrace).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrashHeader {
    /// App version.
    pub version: String,
    /// Time as written (`YYYY-MM-DDTHH:MM:SS.mmmZ`).
    pub time: String,
    /// Thread.
    pub thread: String,
    /// Location, if any.
    pub location: Option<String>,
    /// First line of the message.
    pub message: String,
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_owned();
    }
    let mut out: String = s.chars().take(max).collect();
    out.push_str(" …");
    out
}

/// One header value on one line (newlines escaped).
fn one_line(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('\r', "")
        .replace('\n', "\\n")
}

impl CrashReport {
    /// The report text, with every field redacted and the message/backtrace bounded.
    pub fn to_text(&self, redactor: &Redactor) -> String {
        let clean = |s: &str| redactor.clean(s);
        let mut out = String::new();
        out.push_str(MAGIC);
        out.push('\n');
        out.push_str(&format!("version: {}\n", one_line(&self.version)));
        out.push_str(&format!("time: {}\n", iso_datetime(self.at)));
        out.push_str(&format!("os: {}\n", one_line(&self.os)));
        out.push_str(&format!("thread: {}\n", one_line(&clean(&self.thread))));
        if let Some(loc) = &self.location {
            out.push_str(&format!("location: {}\n", one_line(&clean(loc))));
        }
        let message = truncate(&clean(&self.message), MAX_MESSAGE);
        out.push_str(&format!("message: {}\n", one_line(&message)));
        out.push('\n');
        out.push_str("backtrace:\n");
        out.push_str(&truncate(&clean(&self.backtrace), MAX_BACKTRACE));
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out
    }
}

/// Reads the header of a report written by [`CrashReport::to_text`]; `None` when `text` is
/// not a crash report.
pub fn parse_header(text: &str) -> Option<CrashHeader> {
    let mut lines = text.lines();
    if lines.next()?.trim_end() != MAGIC {
        return None;
    }
    let mut h = CrashHeader {
        version: String::new(),
        time: String::new(),
        thread: String::new(),
        location: None,
        message: String::new(),
    };
    for line in lines {
        if line.trim().is_empty() {
            break;
        }
        let Some((key, value)) = line.split_once(": ") else {
            continue;
        };
        let value = value.to_owned();
        match key {
            "version" => h.version = value,
            "time" => h.time = value,
            "thread" => h.thread = value,
            "location" => h.location = Some(value),
            "message" => {
                h.message = value.split("\\n").next().unwrap_or_default().to_owned();
            }
            _ => {}
        }
    }
    Some(h)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report() -> CrashReport {
        CrashReport {
            version: "1.0.0".into(),
            at: Timestamp(1_795_169_472_123),
            os: "windows x86_64".into(),
            thread: "main".into(),
            location: Some("crates/fc-app/src/x.rs:10:5".into()),
            message: "boom with api_key=sk-ant-api03-AAAABBBBCCCCDDDDEEEEFFFF\nsecond line".into(),
            backtrace: "0: std::panicking\n1: C:\\Users\\arda\\src\\main.rs".into(),
        }
    }

    #[test]
    fn report_round_trips_its_header_and_is_redacted() {
        let text = report().to_text(&Redactor::new());
        assert!(text.starts_with(MAGIC));
        assert!(!text.contains("sk-ant"), "{text}");
        assert!(!text.contains("arda"), "{text}");
        assert!(text.contains("backtrace:\n0: std::panicking"));
        let h = parse_header(&text).unwrap();
        assert_eq!(h.version, "1.0.0");
        assert_eq!(h.time, "2026-11-20T10:11:12.123Z");
        assert_eq!(h.thread, "main");
        assert_eq!(h.location.as_deref(), Some("crates/fc-app/src/x.rs:10:5"));
        assert_eq!(h.message, "boom with api_key=[REDACTED]");
    }

    #[test]
    fn long_messages_are_bounded_and_foreign_text_is_not_a_report() {
        let mut r = report();
        r.message = "x".repeat(MAX_MESSAGE * 3);
        let text = r.to_text(&Redactor::new());
        assert!(text.len() < MAX_MESSAGE + 500 + r.backtrace.len());
        assert_eq!(parse_header("hello\nversion: 1"), None);
        assert_eq!(parse_header(""), None);
    }
}
