//! App maintenance (v1.0, docs/02 §34): pure helpers behind crash recovery and log export.
//!
//! * [`redact`]: masks secrets (API keys, tokens, passwords, credentials in URLs, emails, the
//!   user's name in home-folder paths) in any text before it is written to a log, a crash
//!   report or an exported log bundle.
//! * [`crash`]: the crash report text format (written by the panic hook, listed and exported
//!   by the app).
//! * [`rotation`]: names and order of rotated log files.
//! * [`recovery`]: what to offer after an unclean shutdown.
//!
//! No I/O here: the app (`fc-app` `maintenance` module) and the desktop shell do the writing.

pub mod crash;
pub mod recovery;
pub mod redact;
pub mod rotation;

use crate::stats::date_of_day;
use crate::Timestamp;

/// `YYYY-MM-DDTHH:MM:SS.mmmZ` (UTC) of a timestamp.
pub fn iso_datetime(t: Timestamp) -> String {
    let ms = t.0;
    let day = ms.div_euclid(86_400_000);
    let in_day = ms.rem_euclid(86_400_000);
    let (y, m, d) = date_of_day(day);
    let (h, rest) = (in_day / 3_600_000, in_day % 3_600_000);
    let (min, rest) = (rest / 60_000, rest % 60_000);
    let (s, milli) = (rest / 1000, rest % 1000);
    format!("{y:04}-{m:02}-{d:02}T{h:02}:{min:02}:{s:02}.{milli:03}Z")
}

/// Compact UTC stamp for file names: `YYYYMMDD-HHMMSS-mmm` (sorts in time order).
pub fn file_stamp(t: Timestamp) -> String {
    let iso = iso_datetime(t);
    // 2026-11-20T10:11:12.123Z → 20261120-101112-123
    let digits: String = iso.chars().filter(char::is_ascii_digit).collect();
    if digits.len() < 17 {
        return digits;
    }
    format!("{}-{}-{}", &digits[..8], &digits[8..14], &digits[14..17])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_datetime_formats_utc_milliseconds() {
        assert_eq!(iso_datetime(Timestamp(0)), "1970-01-01T00:00:00.000Z");
        // 2026-11-20T10:11:12.123Z
        let t = Timestamp(1_795_169_472_123);
        assert_eq!(iso_datetime(t), "2026-11-20T10:11:12.123Z");
        assert_eq!(file_stamp(t), "20261120-101112-123");
        assert_eq!(iso_datetime(Timestamp(-1)), "1969-12-31T23:59:59.999Z");
    }
}
