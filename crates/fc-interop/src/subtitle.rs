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
    // Line by line rather than by blank-line blocks: real files run cues together without
    // the blank line, and a timing line always starts a new cue.
    let lines: Vec<&str> = text.lines().map(str::trim).collect();
    let is_timing = |l: &str| l.contains("-->") && parse_timing(l).is_some();
    let mut cues = Vec::new();
    let mut current: Option<(u64, u64, Vec<String>)> = None;
    let mut block_start = true;
    let mut skipping = false;
    let mut flush = |current: &mut Option<(u64, u64, Vec<String>)>| -> InteropResult<()> {
        if let Some((start_ms, end_ms, body)) = current.take() {
            if !body.is_empty() {
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
        }
        Ok(())
    };
    for (i, &line) in lines.iter().enumerate() {
        if line.is_empty() {
            flush(&mut current)?;
            block_start = true;
            skipping = false;
            continue;
        }
        if is_timing(line) {
            flush(&mut current)?;
            if let Some((start_ms, end_ms)) = parse_timing(line) {
                current = Some((start_ms, end_ms, Vec::new()));
            }
            block_start = false;
            skipping = false;
            continue;
        }
        if skipping {
            continue;
        }
        let first_word = line.split_whitespace().next().unwrap_or("");
        if block_start && matches!(first_word, "WEBVTT" | "NOTE" | "STYLE" | "REGION") {
            // Header and NOTE/STYLE/REGION blocks: up to the next blank line.
            flush(&mut current)?;
            skipping = true;
            continue;
        }
        let before_timing = lines.get(i + 1).is_some_and(|n| is_timing(n));
        if before_timing && (block_start || line.bytes().all(|b| b.is_ascii_digit())) {
            // A cue number / identifier: ends the previous cue even without a blank line.
            flush(&mut current)?;
            block_start = false;
            continue;
        }
        block_start = false;
        if let Some((_, _, body)) = current.as_mut() {
            let clean = clean_line(line);
            if !clean.is_empty() {
                body.push(clean);
            }
        }
    }
    flush(&mut current)?;
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

/// Whether `inner` (what is between `<` and `>`) is markup: a tag (`i`, `/i`, `c.red`,
/// `v Roger`, `font color="red"`) or a VTT timestamp (`00:01:03.000`).
fn is_tag(inner: &str) -> bool {
    let mut chars = inner.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '/' => !inner.contains('<'),
        Some(c) if c.is_ascii_digit() => inner
            .chars()
            .all(|c| c.is_ascii_digit() || c == ':' || c == '.'),
        _ => false,
    }
}

/// Removes `<…>` tags and ASS `{\…}` override blocks, decodes common entities, collapses
/// spaces. A `<` or `{` that does not open markup is text (`a < b`, `I <3 you`, `{sic}`).
fn clean_line(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(c) = rest.chars().next() {
        let markup_end = match c {
            '<' => rest[1..]
                .find('>')
                .filter(|&j| is_tag(&rest[1..1 + j]))
                .map(|j| j + 2),
            '{' if rest[1..].starts_with('\\') => rest.find('}').map(|j| j + 1),
            _ => None,
        };
        match markup_end {
            Some(n) => rest = &rest[n..],
            None => {
                out.push(c);
                rest = &rest[c.len_utf8()..];
            }
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

    #[test]
    fn cues_without_blank_lines_stay_apart() {
        let srt = "1\n00:00:01,000 --> 00:00:02,000\nFirst line\nsecond line\n2\n00:00:03,000 --> 00:00:04,000\nNext cue\n00:00:05,000 --> 00:00:06,000\nNo number either\n\n\n4\n00:00:07,000 --> 00:00:08,000\n42\n";
        let (_, cues) = parse_subtitles(srt.as_bytes()).unwrap();
        let texts: Vec<&str> = cues.iter().map(|c| c.text.as_str()).collect();
        assert_eq!(
            texts,
            [
                "First line second line",
                "Next cue",
                "No number either",
                "42"
            ]
        );
        assert_eq!(cues[1].start_ms, 3_000);
        // A VTT header directly followed by a cue.
        let vtt = "WEBVTT\n00:00:01.000 --> 00:00:02.000\nHi\nNOTE not a note: inside a cue\n";
        let (_, cues) = parse_subtitles(vtt.as_bytes()).unwrap();
        assert_eq!(cues.len(), 1);
        assert_eq!(cues[0].text, "Hi NOTE not a note: inside a cue");
    }

    #[test]
    fn a_lone_angle_bracket_is_text() {
        assert_eq!(clean_line("if a < b then <i>c</i>"), "if a < b then c");
        assert_eq!(clean_line("I <3 you"), "I <3 you");
        assert_eq!(clean_line("x <- y > z"), "x <- y > z");
        assert_eq!(clean_line("5 < 6 and 7 > 3"), "5 < 6 and 7 > 3");
        assert_eq!(clean_line("<"), "<");
        assert_eq!(clean_line("{sic} {\\an8}top"), "{sic} top");
        assert_eq!(
            clean_line("<00:01:03.000>Hızlı! <c.red>kırmızı</c>"),
            "Hızlı! kırmızı"
        );
        assert_eq!(clean_line("<font color=\"red\">R</font> &lt;3"), "R <3");
    }
}
