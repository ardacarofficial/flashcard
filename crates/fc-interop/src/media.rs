//! Media references inside field HTML.
//!
//! Inside the collection a field refers to media by content hash: `<img src="fcmedia://<sha256>">`
//! and `[sound:fcmedia://<sha256>]` (docs/03 §13: the frontend resolves `fcmedia://`, it never
//! sees file paths). Foreign formats use file names (`<img src="cat.jpg">`, `[sound:a.mp3]`);
//! importers and exporters translate with [`rewrite`].

use sha2::{Digest, Sha256};

/// URL scheme of collection media.
pub const SCHEME: &str = "fcmedia://";

/// Lowercase hex SHA-256 of `bytes` (the media id).
pub fn sha256_hex(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    const H: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(H[(b >> 4) as usize] as char);
        s.push(H[(b & 15) as usize] as char);
    }
    s
}

/// `fcmedia://<hash>`.
pub fn media_url(hash: &str) -> String {
    format!("{SCHEME}{hash}")
}

/// The hash of an `fcmedia://` reference.
pub fn hash_of(reference: &str) -> Option<&str> {
    reference.strip_prefix(SCHEME)
}

/// Best-effort MIME type from a file name's extension.
pub fn guess_mime(name: &str) -> &'static str {
    let ext = name
        .rsplit_once('.')
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        "avif" => "image/avif",
        "mp3" => "audio/mpeg",
        "ogg" | "oga" => "audio/ogg",
        "wav" => "audio/wav",
        "m4a" => "audio/mp4",
        "flac" => "audio/flac",
        "opus" => "audio/opus",
        "mp4" => "video/mp4",
        "webm" => "video/webm",
        "pdf" => "application/pdf",
        _ => "application/octet-stream",
    }
}

/// A file extension for `mime` (used when exporting media that has no original name).
pub fn extension_for(mime: &str) -> &'static str {
    match mime {
        "image/jpeg" => "jpg",
        "image/png" => "png",
        "image/gif" => "gif",
        "image/webp" => "webp",
        "image/svg+xml" => "svg",
        "audio/mpeg" => "mp3",
        "audio/ogg" => "ogg",
        "audio/wav" => "wav",
        "audio/mp4" => "m4a",
        "video/mp4" => "mp4",
        "video/webm" => "webm",
        _ => "bin",
    }
}

/// Every media reference in `html`: `src="…"` attribute values and `[sound:…]` tags, in order.
pub fn references(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    let _ = rewrite(html, |r| {
        out.push(r.to_owned());
        None
    });
    out
}

/// Replaces media references: `f` gets each reference and returns its replacement (`None` =
/// keep). Handles `src=` with double, single or no quotes (any tag) and `[sound:…]`.
pub fn rewrite(html: &str, mut f: impl FnMut(&str) -> Option<String>) -> String {
    let out = rewrite_src(html, &mut f);
    rewrite_sound(&out, &mut f)
}

fn rewrite_src(html: &str, f: &mut impl FnMut(&str) -> Option<String>) -> String {
    let lower = html.to_ascii_lowercase();
    let mut out = String::with_capacity(html.len());
    let mut i = 0;
    while let Some(pos) = lower[i..].find("src=") {
        let at = i + pos;
        let preceded_ok = at == 0
            || html[..at]
                .chars()
                .last()
                .is_some_and(|c| c.is_whitespace() || c == '<');
        let inside_tag = html[..at].rfind('<') > html[..at].rfind('>');
        let value_start = at + 4;
        if !preceded_ok || !inside_tag || value_start > html.len() {
            out.push_str(&html[i..value_start.min(html.len())]);
            i = value_start.min(html.len());
            continue;
        }
        out.push_str(&html[i..value_start]);
        let rest = &html[value_start..];
        let (quote, body_start) = match rest.chars().next() {
            Some(q @ ('"' | '\'')) => (Some(q), 1),
            _ => (None, 0),
        };
        let body = &rest[body_start..];
        let end = match quote {
            Some(q) => body.find(q),
            None => body.find(|c: char| c.is_whitespace() || c == '>'),
        }
        .unwrap_or(body.len());
        let value = &body[..end];
        if let Some(q) = quote {
            out.push(q);
        }
        match f(value) {
            Some(new) => out.push_str(&new),
            None => out.push_str(value),
        }
        i = value_start + body_start + end;
    }
    out.push_str(&html[i..]);
    out
}

fn rewrite_sound(html: &str, f: &mut impl FnMut(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(pos) = rest.find("[sound:") {
        let after = &rest[pos + 7..];
        let Some(end) = after.find(']') else {
            break;
        };
        out.push_str(&rest[..pos + 7]);
        let value = &after[..end];
        match f(value) {
            Some(new) => out.push_str(&new),
            None => out.push_str(value),
        }
        out.push(']');
        rest = &after[end + 1..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_and_rewrites_references() {
        let html = r#"<div><img src="cat.jpg"> <IMG class=x SRC='dog 1.png'><img src=bird.gif>[sound:hi.mp3] src="not.tag"</div>"#;
        assert_eq!(
            references(html),
            vec!["cat.jpg", "dog 1.png", "bird.gif", "hi.mp3"]
        );
        let out = rewrite(html, |r| Some(format!("X{r}")));
        assert_eq!(
            out,
            r#"<div><img src="Xcat.jpg"> <IMG class=x SRC='Xdog 1.png'><img src=Xbird.gif>[sound:Xhi.mp3] src="not.tag"</div>"#
        );
        assert_eq!(rewrite("no media", |_| Some("y".into())), "no media");
        assert_eq!(rewrite("[sound:open", |_| Some("y".into())), "[sound:open");
    }

    #[test]
    fn hashes_and_mimes() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(guess_mime("A.JPG"), "image/jpeg");
        assert_eq!(guess_mime("x"), "application/octet-stream");
        assert_eq!(hash_of(&media_url("ab")), Some("ab"));
    }
}
