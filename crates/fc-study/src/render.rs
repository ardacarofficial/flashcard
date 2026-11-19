//! Minimal card rendering for v0.2: `{{field}}` substitution producing **plain text**.
//!
//! Field HTML is reduced to text (tags dropped, common entities decoded) and the frontend
//! shows the result as text, never as HTML — so nothing here can inject markup. Rich
//! rendering (template language, sanitized HTML, cloze, media) belongs to the card type
//! engines of v0.3 and replaces this module's use.

use std::collections::BTreeMap;

/// Marker separating the repeated question from the answer in a back template.
pub const ANSWER_MARKER: &str = "<hr id=\"answer\">";

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
    let Some(end) = s[..s.len().min(12)].find(';') else {
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

/// A rendered side: plain text plus the expected typed answer, if the template asks for one.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RenderedSide {
    /// Plain text.
    pub text: String,
    /// `{{type:field}}`: the field's text.
    pub type_answer: Option<String>,
}

/// Renders `template` with `fields` (key → HTML value). `{{FrontSide}}` inserts
/// `front_side`; `{{type:key}}` is removed from the text and reported as `type_answer`;
/// unknown fields render empty. The output is plain text.
pub fn render(
    template: &str,
    fields: &BTreeMap<String, String>,
    front_side: Option<&str>,
) -> RenderedSide {
    let mut out = String::with_capacity(template.len());
    let mut type_answer = None;
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            out.push_str(&rest[start..]);
            rest = "";
            break;
        };
        let name = after[..end].trim();
        if name == "FrontSide" {
            // Already text: protect it from the final HTML pass by escaping.
            out.push_str(&escape_html(front_side.unwrap_or_default()));
        } else if let Some(key) = name.strip_prefix("type:") {
            let value = fields.get(key.trim()).map(|v| html_to_text(v));
            type_answer = type_answer.or(value);
        } else {
            let value = fields
                .get(name)
                .map(|v| html_to_text(v))
                .unwrap_or_default();
            out.push_str(&escape_html(&value));
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    RenderedSide {
        text: html_to_text(&out),
        type_answer,
    }
}

/// Escapes `&`, `<`, `>` (so text survives a later [`html_to_text`] pass unchanged).
pub fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// The answer part of a back template: what follows [`ANSWER_MARKER`] (the whole template
/// when there is no marker).
pub fn answer_part(back_template: &str) -> &str {
    back_template
        .find(ANSWER_MARKER)
        .map_or(back_template, |i| &back_template[i + ANSWER_MARKER.len()..])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
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

    #[test]
    fn renders_classic_sides_as_text() {
        let f = fields(&[
            ("front", "<i>apple</i>"),
            ("back", "elma <script>x</script>"),
        ]);
        let q = render("{{front}}", &f, None);
        assert_eq!(q.text, "apple");
        let back = "{{front}}\n<hr id=\"answer\">\n{{back}}";
        let a = render(answer_part(back), &f, Some(&q.text));
        assert_eq!(a.text, "elma x");
        // Literal angle brackets in a field stay text and never become markup.
        let f = fields(&[("front", "&lt;b&gt;bold&lt;/b&gt;")]);
        assert_eq!(render("{{front}}", &f, None).text, "<b>bold</b>");
    }

    #[test]
    fn type_and_frontside() {
        let f = fields(&[("front", "dog"), ("back", "köpek")]);
        let q = render("{{front}} {{type:back}}", &f, None);
        assert_eq!(q.text, "dog");
        assert_eq!(q.type_answer.as_deref(), Some("köpek"));
        let a = render("{{FrontSide}} = {{back}} {{missing}}", &f, Some("a<b"));
        assert_eq!(a.text, "a<b = köpek");
        assert_eq!(render("{{front", &f, None).text, "{{front");
    }
}
