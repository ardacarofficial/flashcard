//! Subtitle extension captures (v1.2, docs/06 §5.1): the pure parts.
//!
//! The Subtitle browser extension sends a *capture* (a word picked in a video subtitle, with
//! its meaning, the subtitle line, its translation, a screenshot and an audio clip) to the
//! app; the app turns it into a note of the built-in "Subtitle card" note type. This module
//! holds what needs no storage:
//!
//! * the **dedup key** of the contract (`lang.target + lemma + sha256(lower(collapse_spaces(
//!   sentence)))`), [`dedup_key`];
//! * the **note fields** of a capture ([`note_fields`]): HTML-escaped text, the term in bold
//!   inside the sentence (from its UTF-16 `term_offset`), the source as a link with the time;
//! * the **known word** rule ([`KNOWN_INTERVAL_DAYS`]): the statistics' "mature" interval.

use crate::content::html::escape_html;
use crate::reading::format_timestamp;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// A word whose card reached this interval (days) is *known*; any other card is *learning*
/// (the statistics' mature interval, [`crate::stats::MATURE_DAYS`]).
pub const KNOWN_INTERVAL_DAYS: u32 = crate::stats::MATURE_DAYS;

/// `text` with every run of whitespace turned into one space, trimmed.
pub fn collapse_spaces(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The lemma as it is compared and stored: spaces collapsed, lowercase.
pub fn normalize_lemma(lemma: &str) -> String {
    collapse_spaces(lemma).to_lowercase()
}

/// The language code as it is compared and stored: trimmed, lowercase.
pub fn normalize_lang(lang: &str) -> String {
    lang.trim().to_lowercase()
}

fn sha256_hex(text: &str) -> String {
    Sha256::digest(text.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// `sha256(lower(collapse_spaces(sentence)))` in hex (no sentence = the empty one).
pub fn sentence_hash(sentence: Option<&str>) -> String {
    sha256_hex(&collapse_spaces(sentence.unwrap_or_default()).to_lowercase())
}

/// The contract's dedup key: `<lang>|<lemma>|<sentence hash>`. Two captures with the same key
/// are the same card, whatever their `capture_id`.
pub fn dedup_key(lang: &str, lemma: &str, sentence: Option<&str>) -> String {
    format!(
        "{}|{}|{}",
        normalize_lang(lang),
        normalize_lemma(lemma),
        sentence_hash(sentence)
    )
}

/// The byte range of the UTF-16 range `[start, end)` of `text`, if it is a non-empty range
/// inside the text on character boundaries.
pub fn utf16_byte_range(text: &str, start: i64, end: i64) -> Option<(usize, usize)> {
    if start < 0 || start >= end {
        return None;
    }
    let (start, end) = (u64::try_from(start).ok()?, u64::try_from(end).ok()?);
    let (mut unit, mut bs, mut be) = (0u64, None, None);
    for (byte, c) in text.char_indices() {
        if unit == start {
            bs = Some(byte);
        }
        if unit == end {
            be = Some(byte);
        }
        unit += c.len_utf16() as u64;
    }
    if unit == end {
        be = Some(text.len());
    }
    match (bs, be) {
        (Some(s), Some(e)) if s < e => Some((s, e)),
        _ => None,
    }
}

/// Plain text → text for a Markdown (rich text) field: HTML escaped, and the few Markdown
/// marks that subtitle lines really contain neutralized (a dialogue dash `- Where?` would
/// otherwise become a list, `*word*` emphasis).
pub fn rich_text(text: &str) -> String {
    escape_md(&escape_html(&collapse_spaces(text)), true)
}

/// Backslash-escapes Markdown marks in `html` (already HTML-escaped); `line_start`: the text
/// begins a line, where `-`/`+`/`#`/`1.` would start a block.
fn escape_md(html: &str, line_start: bool) -> String {
    let mut out = String::with_capacity(html.len() + 4);
    for (i, c) in html.chars().enumerate() {
        let leading = line_start && i == 0 && matches!(c, '-' | '+' | '#');
        if leading || matches!(c, '\\' | '*' | '_' | '`' | '~' | '[' | ']') {
            out.push('\\');
        }
        out.push(c);
    }
    if line_start {
        // "1. Hello" would be an ordered list.
        if let Some(dot) = out.find(['.', ')']) {
            if dot > 0 && out[..dot].chars().all(|c| c.is_ascii_digit()) {
                out.insert(dot, '\\');
            }
        }
    }
    out
}

/// The sentence as field text with the term in bold: by its UTF-16 `term_offset`, else (no
/// or a bad offset) the first case-insensitive occurrence of `surface`, else plain.
pub fn highlight(sentence: &str, term_offset: Option<(i64, i64)>, surface: &str) -> String {
    let range = term_offset
        .and_then(|(s, e)| utf16_byte_range(sentence, s, e))
        .filter(|&(s, e)| !sentence[s..e].trim().is_empty())
        .or_else(|| find_ignore_case(sentence, surface.trim()));
    let Some((s, e)) = range else {
        return rich_text(sentence);
    };
    let (before, term, after) = (&sentence[..s], &sentence[s..e], &sentence[e..]);
    let part = |t: &str, start: bool| escape_md(&escape_html(&collapse_spaces(t)), start);
    // Spaces next to the term survive collapsing each side.
    let lead = if before.ends_with(char::is_whitespace) && !before.trim().is_empty() {
        " "
    } else {
        ""
    };
    let trail = if after.starts_with(char::is_whitespace) && !after.trim().is_empty() {
        " "
    } else {
        ""
    };
    let first = before.trim().is_empty();
    format!(
        "{}{lead}<b>{}</b>{trail}{}",
        part(before, true),
        part(term, first),
        part(after, false)
    )
}

fn find_ignore_case(text: &str, needle: &str) -> Option<(usize, usize)> {
    if needle.is_empty() {
        return None;
    }
    let lower = text.to_lowercase();
    // Only when lowercasing keeps byte offsets (ASCII-like texts); else give up.
    if lower.len() != text.len() {
        return text.find(needle).map(|s| (s, s + needle.len()));
    }
    let at = lower.find(&needle.to_lowercase())?;
    let end = at + needle.len();
    (text.is_char_boundary(at) && text.is_char_boundary(end)).then_some((at, end))
}

/// Where a capture came from, as field HTML: `Title · 1:23` (or the site when there is no
/// title), a link when the page address is an `http(s)` URL.
pub fn source_html(
    title: Option<&str>,
    site: &str,
    url: Option<&str>,
    t_start_ms: Option<u64>,
) -> String {
    let name = title
        .map(collapse_spaces)
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| collapse_spaces(site));
    let mut label = rich_text(&name);
    if let Some(ms) = t_start_ms {
        if !label.is_empty() {
            label.push_str(" · ");
        }
        label.push_str(&format_timestamp(ms));
    }
    let url = url.map(str::trim).filter(|u| {
        let l = u.to_ascii_lowercase();
        (l.starts_with("https://") || l.starts_with("http://"))
            && !u.chars().any(|c| c.is_whitespace() || c.is_control())
    });
    match url {
        Some(u) if !label.is_empty() => format!("<a href=\"{}\">{label}</a>", escape_html(u)),
        Some(u) => format!("<a href=\"{0}\">{0}</a>", escape_html(u)),
        None => label,
    }
}

/// What a note is made from (the validated capture, with media already in the store).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CaptureText<'a> {
    /// Dictionary form (`term.lemma`).
    pub lemma: &'a str,
    /// As clicked (`term.surface`).
    pub surface: &'a str,
    /// Part of speech (`term.pos`).
    pub pos: Option<&'a str>,
    /// The meaning the user picked (`definition.text`).
    pub meaning: &'a str,
    /// The subtitle line (`context.sentence`).
    pub sentence: Option<&'a str>,
    /// The line in the user's language (`context.translation`).
    pub translation: Option<&'a str>,
    /// UTF-16 `[start, end)` of the term in the sentence (`context.term_offset`).
    pub term_offset: Option<(i64, i64)>,
    /// Video title (`source.title`).
    pub title: Option<&'a str>,
    /// Site (`source.site`).
    pub site: &'a str,
    /// Page address at the line (`source.url`).
    pub url: Option<&'a str>,
    /// When the line starts (`source.t_start_ms`).
    pub t_start_ms: Option<u64>,
    /// Media hash of the screenshot.
    pub screenshot: Option<&'a str>,
    /// Media hash of the audio clip.
    pub audio: Option<&'a str>,
}

/// The fields of the "Subtitle card" note made from a capture (empty ones left out).
pub fn note_fields(c: &CaptureText<'_>) -> BTreeMap<String, String> {
    fn some(t: Option<&str>) -> Option<&str> {
        t.map(str::trim).filter(|t| !t.is_empty())
    }
    let plain = collapse_spaces;
    let media = |h: Option<&str>| {
        h.filter(|h| crate::content::refs::is_media_hash(h))
            .map(|h| format!("{}{h}", crate::content::refs::MEDIA_SCHEME))
    };
    let pairs = [
        ("word", Some(rich_text(c.lemma))),
        ("surface", Some(plain(c.surface))),
        ("pos", some(c.pos).map(plain)),
        ("meaning", Some(rich_text(c.meaning))),
        (
            "sentence",
            some(c.sentence).map(|s| highlight(s, c.term_offset, c.surface)),
        ),
        ("translation", some(c.translation).map(rich_text)),
        ("image", media(c.screenshot)),
        ("audio", media(c.audio)),
        (
            "source",
            Some(source_html(c.title, c.site, c.url, c.t_start_ms)),
        ),
    ];
    pairs
        .into_iter()
        .filter_map(|(k, v)| v.filter(|v| !v.is_empty()).map(|v| (k.to_owned(), v)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedup_key_normalizes_lang_lemma_and_sentence() {
        let a = dedup_key(
            "EN",
            " Pick  Up ",
            Some("I just  need to PICK up\tmy keys."),
        );
        let b = dedup_key("en", "pick up", Some("i just need to pick up my keys."));
        assert_eq!(a, b);
        assert!(a.starts_with("en|pick up|"));
        assert_ne!(a, dedup_key("en", "pick up", Some("Another line.")));
        assert_eq!(dedup_key("en", "x", None), dedup_key("en", "x", Some("  ")));
        // sha256("") is the empty-sentence hash.
        assert_eq!(
            sentence_hash(None),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn utf16_ranges() {
        let s = "Almost. I just need to pick up my keys.";
        assert_eq!(utf16_byte_range(s, 34, 38), Some((34, 38)));
        assert_eq!(&s[34..38], "keys");
        // Astral characters count two units.
        let e = "🙂 key";
        assert_eq!(utf16_byte_range(e, 3, 6), Some((5, 8)));
        assert_eq!(utf16_byte_range(e, 1, 3), None, "inside a surrogate pair");
        assert_eq!(utf16_byte_range(e, 3, 7), None, "past the end");
        assert_eq!(utf16_byte_range(e, 4, 4), None);
        assert_eq!(utf16_byte_range(e, -1, 2), None);
        assert_eq!(utf16_byte_range("ab", 0, 2), Some((0, 2)));
    }

    #[test]
    fn sentences_get_the_term_in_bold() {
        let s = "Almost. I just need to pick up my keys.";
        assert_eq!(
            highlight(s, Some((34, 38)), "keys"),
            "Almost. I just need to pick up my <b>keys</b>."
        );
        // No offset: the surface is looked up (case-insensitively).
        assert_eq!(
            highlight("Keys, please.", None, "keys"),
            "<b>Keys</b>, please."
        );
        // A bad offset falls back too; nothing found → plain.
        assert_eq!(highlight("a b", Some((5, 9)), "zzz"), "a b");
        assert_eq!(
            highlight("<i>x</i> & y", Some((11, 12)), "y"),
            "&lt;i&gt;x&lt;/i&gt; &amp; <b>y</b>"
        );
        assert_eq!(highlight("a -b", Some((2, 4)), ""), "a <b>-b</b>");
        // Dialogue dashes and emphasis marks stay text.
        assert_eq!(highlight("- Where?", None, "where"), "\\- <b>Where</b>?");
        assert_eq!(rich_text("1. Go *now*"), "1\\. Go \\*now\\*");
        assert_eq!(rich_text("#1 fan"), "\\#1 fan");
    }

    #[test]
    fn sources_are_links_with_the_time() {
        assert_eq!(
            source_html(
                Some("Never Gonna Give You Up"),
                "youtube",
                Some("https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=83s"),
                Some(83_450)
            ),
            "<a href=\"https://www.youtube.com/watch?v=dQw4w9WgXcQ&amp;t=83s\">Never Gonna Give You Up · 1:23</a>"
        );
        assert_eq!(source_html(None, "netflix", None, None), "netflix");
        // Only http(s) addresses become links.
        assert_eq!(
            source_html(Some("T"), "x", Some("javascript:alert(1)"), None),
            "T"
        );
    }

    #[test]
    fn note_fields_leave_out_empty_ones() {
        let hash = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";
        let f = note_fields(&CaptureText {
            lemma: "key",
            surface: "keys",
            pos: Some("noun"),
            meaning: "anahtar",
            sentence: Some("Almost. I just need to pick up my keys."),
            translation: Some("Neredeyse. Sadece anahtarlarımı almam lazım."),
            term_offset: Some((34, 38)),
            title: Some("T"),
            site: "youtube",
            url: None,
            t_start_ms: Some(1000),
            screenshot: Some(hash),
            audio: Some("not-a-hash"),
        });
        assert_eq!(f["word"], "key");
        assert_eq!(f["surface"], "keys");
        assert_eq!(f["pos"], "noun");
        assert_eq!(f["image"], format!("media:{hash}"));
        assert!(!f.contains_key("audio"));
        assert_eq!(f["source"], "T · 0:01");
        assert!(f["sentence"].contains("<b>keys</b>"));
        let bare = note_fields(&CaptureText {
            lemma: "x",
            surface: "x",
            site: "s",
            ..Default::default()
        });
        assert_eq!(
            bare.keys().collect::<Vec<_>>(),
            ["source", "surface", "word"]
        );
    }
}
