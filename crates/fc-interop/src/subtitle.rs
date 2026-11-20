//! Subtitles (v0.10, docs/02 §25): SubRip (`.srt`) and WebVTT (`.vtt`) → cues.
//!
//! Lenient by design (real subtitle files are messy): any encoding [`crate::text::decode`]
//! understands, `\r\n`, a BOM, missing or wrong cue numbers, `,` or `.` before the
//! milliseconds, hours optional (`01:02.345`), WebVTT headers, `NOTE`/`STYLE`/`REGION`
//! blocks, cue identifiers and cue settings. Cue text loses its markup (`<i>`, `<c.red>`,
//! `<00:01.000>`, ASS-style `{\an8}`), entities are decoded and lines are joined with a space.
//! Cues come back sorted by start time; empty ones are dropped.

use crate::error::{InteropError, InteropResult};
use serde::{Deserialize, Serialize};

const FORMAT: &str = "subtitle";

/// Most cues per file (a feature film has ~1 500).
pub const MAX_CUES: usize = 20_000;

/// One subtitle cue.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cue {
    /// Start, ms from the beginning of the media.
    pub start_ms: u64,
    /// End, ms (≥ start).
    pub end_ms: u64,
    /// Plain text, lines joined with a space.
    pub text: String,
}

/// Subtitle format.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubtitleFormat {
    /// SubRip.
    Srt,
    /// WebVTT.
    Vtt,
}

/// Parses subtitle bytes. The format is detected (`WEBVTT` header → VTT, else SRT; both
/// parse the same way apart from the header). Fails when no cue is found.
pub fn parse_subtitles(bytes: &[u8]) -> InteropResult<(SubtitleFormat, Vec<Cue>)> {
    let text = crate::text::normalize_newlines(&crate::text::decode(bytes).text);
    let format = if text.trim_start().starts_with("WEBVTT") {
        SubtitleFormat::Vtt
    } else {
        SubtitleFormat::Srt
    };
    let mut cues = Vec::new();
    for block in text.split("\n\n") {
        let lines: Vec<&str> = block.lines().filter(|l| !l.trim().is_empty()).collect();
        let Some(timing_at) = lines.iter().position(|l| l.contains("-->")) else {
            continue;
        };
        if lines
            .first()
            .is_some_and(|l| l.starts_with("NOTE") || l.starts_with("STYLE"))
            && timing_at > 0
        {
            continue;
        }
        let Some((start_ms, end_ms)) = parse_timing(lines[timing_at]) else {
            continue;
        };
        let body: Vec<String> = lines[timing_at + 1..]
            .iter()
            .map(|l| clean_line(l))
            .filter(|l| !l.is_empty())
            .collect();
        if body.is_empty() {
            continue;
        }
        cues.push(Cue {
            start_ms,
            end_ms: end_ms.max(start_ms),
            text: body.join(" "),
        });
        if cues.len() > MAX_CUES {
            return Err(InteropError::invalid(
                FORMAT,
                format!("more than {MAX_CUES} cues"),
            ));
        }
    }
    if cues.is_empty() {
        return Err(InteropError::invalid(FORMAT, "no subtitle cues found"));
    }
    cues.sort_by_key(|c| (c.start_ms, c.end_ms));
    Ok((format, cues))
}

/// `00:01:02,345 --> 00:01:04.000 align:start` → (62 345, 64 000).
fn parse_timing(line: &str) -> Option<(u64, u64)> {
    let (a, b) = line.split_once("-->")?;
    let start = parse_time(a.trim())?;
    let end = parse_time(b.split_whitespace().next()?)?;
    Some((start, end))
}

/// `hh:mm:ss,mmm`, `hh:mm:ss.mmm`, `mm:ss.mmm` (milliseconds optional, 1–3 digits).
fn parse_time(s: &str) -> Option<u64> {
    let (clock, frac) = match s.split_once([',', '.']) {
        Some((c, f)) => (c, f),
        None => (s, "0"),
    };
    let parts: Vec<&str> = clock.split(':').collect();
    let num = |p: &str| -> Option<u64> {
        (!p.is_empty() && p.len() <= 3 && p.bytes().all(|b| b.is_ascii_digit()))
            .then(|| p.parse().ok())
            .flatten()
    };
    let (h, m, sec) = match parts.as_slice() {
        [h, m, s] => (num(h)?, num(m)?, num(s)?),
        [m, s] => (0, num(m)?, num(s)?),
        _ => return None,
    };
    if m >= 60 || sec >= 60 || frac.is_empty() || frac.len() > 3 {
        return None;
    }
    let ms: u64 = frac.parse().ok()?;
    let ms = ms * 10u64.pow(3 - u32::try_from(frac.len()).ok()?);
    Some(((h * 60 + m) * 60 + sec) * 1000 + ms)
}

/// Removes `<…>` tags and `{…}` override blocks, decodes common entities, collapses spaces.
fn clean_line(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut depth_angle = false;
    let mut depth_brace = false;
    for c in line.chars() {
        match c {
            '<' if !depth_brace => depth_angle = true,
            '>' if depth_angle => depth_angle = false,
            '{' if !depth_angle => depth_brace = true,
            '}' if depth_brace => depth_brace = false,
            _ if depth_angle || depth_brace => {}
            _ => out.push(c),
        }
    }
    let out = out
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
        .replace("\\N", " ");
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srt_with_markup_crlf_and_bom() {
        let srt = "\u{feff}1\r\n00:00:01,000 --> 00:00:03,500\r\n<i>Hello</i> there,\r\n{\\an8}general Kenobi!\r\n\r\n2\r\n00:00:04,250 --> 00:00:05,000\r\nYou &amp; me.\r\n\r\n3\r\n00:00:06,000 --> 00:00:07,000\r\n<font color=\"red\"> </font>\r\n";
        let (fmt, cues) = parse_subtitles(srt.as_bytes()).unwrap();
        assert_eq!(fmt, SubtitleFormat::Srt);
        assert_eq!(
            cues,
            vec![
                Cue {
                    start_ms: 1000,
                    end_ms: 3500,
                    text: "Hello there, general Kenobi!".into()
                },
                Cue {
                    start_ms: 4250,
                    end_ms: 5000,
                    text: "You & me.".into()
                },
            ]
        );
    }

    #[test]
    fn vtt_with_header_notes_ids_settings_and_short_times() {
        let vtt = "WEBVTT - film\nKind: captions\n\nNOTE this is\na comment\n\nSTYLE\n::cue { color: red }\n\nintro\n01:02.5 --> 01:04.250 align:start line:0\n<v Roger><c.loud>Kediler</c> koşuyor.</v>\n<00:01:03.000>Hızlı!\n\n00:00:10.000 --> 00:00:09.000\nTers zaman\n";
        let (fmt, cues) = parse_subtitles(vtt.as_bytes()).unwrap();
        assert_eq!(fmt, SubtitleFormat::Vtt);
        assert_eq!(cues.len(), 2);
        assert_eq!(cues[0].start_ms, 10_000);
        assert_eq!(cues[0].end_ms, 10_000, "end never before start");
        assert_eq!(cues[1].start_ms, 62_500);
        assert_eq!(cues[1].end_ms, 64_250);
        assert_eq!(cues[1].text, "Kediler koşuyor. Hızlı!");
    }

    #[test]
    fn windows_1254_and_garbage() {
        // "Çok güzel" in Windows-1254.
        let mut bytes = b"1\n00:00:00,000 --> 00:00:01,000\n".to_vec();
        bytes.extend_from_slice(&[0xC7, b'o', b'k', b' ', b'g', 0xFC, b'z', b'e', b'l']);
        let (_, cues) = parse_subtitles(&bytes).unwrap();
        assert_eq!(cues[0].text, "Çok güzel");
        assert!(parse_subtitles(b"just some text\n\nno timings").is_err());
        assert!(parse_subtitles(b"1\n00:99:00,000 --> 00:00:01,000\nbad\n").is_err());
        assert_eq!(parse_time("1:02:03"), Some(3_723_000));
        assert_eq!(parse_time("00:00:01,5"), Some(1_500));
        assert_eq!(parse_time("00:00:01,5000"), None);
        assert_eq!(parse_time("aa:00"), None);
    }
}
