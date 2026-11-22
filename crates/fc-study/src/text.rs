//! Plain text of HTML: what the answer checkers and hints compare against. Card rendering
//! itself (template language, sanitized HTML, cloze, media) is v0.3's `CardTypeEngine`; the
//! app gives this crate the engine's expected answer or answer side as text.

/// Text of an HTML fragment: tags removed (`<br>`, `<div>`, `<p>` and `<hr>` become line
/// breaks), the common named and numeric entities decoded, runs of blank lines collapsed and
/// the result trimmed.
pub fn html_to_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(i) = rest.find(['<', '&']) {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        if rest.starts_with('<') {
            match rest.find('>') {
                Some(end) => {
                    let tag = rest[1..end].trim_start_matches('/').to_ascii_lowercase();
                    let name: String = tag
                        .chars()
                        .take_while(|c| c.is_ascii_alphanumeric())
                        .collect();
                    if matches!(name.as_str(), "br" | "div" | "p" | "hr" | "li" | "tr") {
                        out.push('\n');
                    }
                    rest = &rest[end + 1..];
                }
                None => {
                    out.push_str(rest);
                    rest = "";
                }
            }
        } else {
            let (text, used) = decode_entity(rest);
            out.push_str(&text);
            rest = &rest[used..];
        }
    }
    out.push_str(rest);
    tidy(&out)
}

/// Decodes one entity at the start of `s` (which starts with `&`). Returns the text and the
/// bytes consumed; an unknown entity is kept literally.
fn decode_entity(s: &str) -> (String, usize) {
    // Look at most 12 bytes ahead, cut on a character boundary ("& çok" must not panic).
    let mut cut = s.len().min(12);
    while !s.is_char_boundary(cut) {
        cut -= 1;
    }
    let Some(end) = s[..cut].find(';') else {
        return ("&".to_owned(), 1);
    };
    let body = &s[1..end];
    let decoded = match body {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" | "#39" => Some('\''),
        "nbsp" => Some(' '),
        _ => body.strip_prefix('#').and_then(|n| {
            let code = match n.strip_prefix(['x', 'X']) {
                Some(hex) => u32::from_str_radix(hex, 16).ok(),
                None => n.parse().ok(),
            };
            code.and_then(char::from_u32)
        }),
    };
    match decoded {
        Some(c) => (c.to_string(), end + 1),
        None => ("&".to_owned(), 1),
    }
}

/// Trims each line's trailing spaces, collapses blank-line runs, trims the whole.
fn tidy(s: &str) -> String {
    let mut lines: Vec<&str> = Vec::new();
    for line in s.lines().map(str::trim_end) {
        if line.trim().is_empty() && lines.last().is_none_or(|l| l.trim().is_empty()) {
            continue;
        }
        lines.push(line);
    }
    lines.join("\n").trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A bare `&` followed by multi-byte letters within the entity look-ahead (v1.3 fix: it
    /// panicked on a byte slice inside a character).
    #[test]
    fn a_bare_ampersand_before_turkish_letters_is_text() {
        assert_eq!(html_to_text("Tom & ğğğğğğ"), "Tom & ğğğğğğ");
        assert_eq!(html_to_text("a&\u{E000}b"), "a&\u{E000}b");
    }

    #[test]
    fn html_becomes_text() {
        assert_eq!(html_to_text("<b>elma</b> &amp; armut"), "elma & armut");
        assert_eq!(html_to_text("a<br>b<div>c</div>"), "a\nb\nc");
        assert_eq!(
            html_to_text("&lt;script&gt;x&#39;&#x41;&nbsp;"),
            "<script>x'A"
        );
        assert_eq!(html_to_text("1 &unknown; 2 & 3"), "1 &unknown; 2 & 3");
        assert_eq!(html_to_text("a <unclosed"), "a <unclosed");
    }
}
