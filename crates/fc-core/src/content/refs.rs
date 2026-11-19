//! References inside field content: `[[note links]]` and `media:<sha256>` media references.

use super::html::escape_html;
use crate::note::strip_html;
use std::collections::BTreeSet;

/// A `[[target]]` / `[[target|label]]` link found in a field.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct LinkRef {
    /// Normalized target ([`link_key`]).
    pub key: String,
    /// Text shown for the link (the label, else the target as written).
    pub label: String,
}

/// Normalized form of a link target or note title: HTML stripped, whitespace collapsed,
/// Unicode lowercase. `[[Porsuk]]` links to the note whose sort field is "porsuk".
pub fn link_key(text: &str) -> String {
    strip_html(text).to_lowercase()
}

/// Calls `f(start, end, target, label)` for every well-formed `[[…]]` in `text`.
fn scan_links(text: &str, mut f: impl FnMut(usize, usize, &str, Option<&str>)) {
    let mut pos = 0;
    while let Some(rel) = text[pos..].find("[[") {
        let start = pos + rel;
        let body_start = start + 2;
        let Some(close) = text[body_start..].find("]]") else {
            break;
        };
        let body = &text[body_start..body_start + close];
        let end = body_start + close + 2;
        if body.contains('[') || body.contains('\n') || body.trim().is_empty() {
            pos = start + 2;
            continue;
        }
        let (target, label) = match body.split_once('|') {
            Some((t, l)) => (t, Some(l)),
            None => (body, None),
        };
        if target.trim().is_empty() {
            pos = end;
            continue;
        }
        f(start, end, target.trim(), label.map(str::trim));
        pos = end;
    }
}

/// Every link in `text` (deduplicated by key, first label wins).
pub fn extract_links(text: &str) -> Vec<LinkRef> {
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    scan_links(text, |_, _, target, label| {
        let key = link_key(target);
        if !key.is_empty() && seen.insert(key.clone()) {
            out.push(LinkRef {
                key,
                label: label.unwrap_or(target).to_owned(),
            });
        }
    });
    out
}

/// Replaces `[[links]]` with HTML: `<a class="fc-link" href="note:<id>">label</a>` when
/// `resolve(key)` finds the note, else `<span class="fc-link fc-link-missing">label</span>`.
pub fn render_links(text: &str, resolve: &dyn Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut pos = 0;
    scan_links(text, |start, end, target, label| {
        out.push_str(&text[pos..start]);
        let shown = escape_html(label.unwrap_or(target));
        match resolve(&link_key(target)) {
            Some(id) => {
                out.push_str("<a class=\"fc-link\" href=\"note:");
                out.push_str(&escape_html(&id));
                out.push_str("\">");
                out.push_str(&shown);
                out.push_str("</a>");
            }
            None => {
                out.push_str("<span class=\"fc-link fc-link-missing\">");
                out.push_str(&shown);
                out.push_str("</span>");
            }
        }
        pos = end;
    });
    out.push_str(&text[pos..]);
    out
}

/// Prefix of a media reference (`media:<sha256 hex>`).
pub const MEDIA_SCHEME: &str = "media:";

/// Whether `s` is a lowercase SHA-256 hex digest.
pub fn is_media_hash(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Every `media:<hash>` referenced in `text`.
pub fn extract_media(text: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut pos = 0;
    while let Some(rel) = text[pos..].find(MEDIA_SCHEME) {
        let start = pos + rel + MEDIA_SCHEME.len();
        let candidate = text.get(start..start + 64).unwrap_or("");
        let boundary = text.as_bytes()[start..]
            .get(64)
            .is_none_or(|b| !b.is_ascii_alphanumeric());
        if is_media_hash(candidate) && boundary {
            out.insert(candidate.to_owned());
            pos = start + 64;
        } else {
            pos = start;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: &str = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";

    #[test]
    fn links_are_extracted_and_rendered() {
        let t = "see [[Porsuk]] and [[ porsuk |the badger]], [[Kedi|cat]] but not [[]] or [[a\nb]]";
        let links = extract_links(t);
        assert_eq!(
            links,
            vec![
                LinkRef {
                    key: "porsuk".into(),
                    label: "Porsuk".into()
                },
                LinkRef {
                    key: "kedi".into(),
                    label: "cat".into()
                }
            ]
        );
        let html = render_links(t, &|k| (k == "porsuk").then(|| "ID".to_owned()));
        assert!(html.starts_with("see <a class=\"fc-link\" href=\"note:ID\">Porsuk</a> and <a class=\"fc-link\" href=\"note:ID\">the badger</a>"), "{html}");
        assert!(html.contains("<span class=\"fc-link fc-link-missing\">cat</span>"));
        assert!(html.ends_with("[[]] or [[a\nb]]"));
        assert_eq!(link_key(" <b>Big</b>  Dog "), "big dog");
        assert_eq!(
            render_links("[[<img src=x onerror=alert(1)>]]", &|_| None),
            "<span class=\"fc-link fc-link-missing\">&lt;img src=x onerror=alert(1)&gt;</span>"
        );
    }

    #[test]
    fn media_refs() {
        let t = format!("![a](media:{H}) <audio src=\"media:{H}\"> media:{H}x media:abc media:");
        let refs = extract_media(&t);
        assert_eq!(refs.len(), 1);
        assert!(refs.contains(H));
        assert!(is_media_hash(H));
        assert!(!is_media_hash(&H.to_uppercase()));
    }
}
