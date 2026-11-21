//! Validation of the capture record (schema 1 of the contract's JSON Schema). Unknown fields
//! are ignored anywhere; a failure is `VALIDATION_SUBTITLE_CAPTURE` whose message says which
//! field and why (the extension shows it to the user), with `details.field`.

use fc_api::v1::subtitle::*;
use fc_api::{ApiError, ErrorCode};
use serde_json::{Map, Value};

type Checked<T> = Result<T, ApiError>;

/// Longest lemma / surface (characters).
const MAX_TERM: usize = 200;
/// Longest definition, sentence or translation (characters).
const MAX_TEXT: usize = 5_000;
/// Longest title, site, id, URL or version (characters).
const MAX_SHORT: usize = 2_000;
/// Most tags; longest tag.
const MAX_TAGS: usize = 50;
const MAX_TAG: usize = 100;

fn fail(field: &str, message: String) -> ApiError {
    ApiError::new(ErrorCode::ValidationSubtitleCapture, message).with_detail("field", field)
}

fn path(parent: &str, key: &str) -> String {
    if parent.is_empty() {
        key.to_owned()
    } else {
        format!("{parent}.{key}")
    }
}

fn object<'a>(v: &'a Value, at: &str) -> Checked<&'a Map<String, Value>> {
    v.as_object()
        .ok_or_else(|| fail(at, format!("{at} must be an object")))
}

fn field<'a>(o: &'a Map<String, Value>, parent: &str, key: &str) -> Checked<&'a Value> {
    o.get(key).ok_or_else(|| {
        let p = path(parent, key);
        fail(&p, format!("{p} is required"))
    })
}

fn long(p: &str, s: &str, max: usize) -> Checked<()> {
    if s.chars().count() > max {
        return Err(fail(p, format!("{p} is longer than {max} characters")));
    }
    Ok(())
}

fn string(o: &Map<String, Value>, parent: &str, key: &str, max: usize) -> Checked<String> {
    let p = path(parent, key);
    let s = field(o, parent, key)?
        .as_str()
        .ok_or_else(|| fail(&p, format!("{p} must be a string")))?;
    long(&p, s, max)?;
    Ok(s.to_owned())
}

fn non_empty(o: &Map<String, Value>, parent: &str, key: &str, min: usize, max: usize) -> Checked<String> {
    let s = string(o, parent, key, max)?;
    if s.trim().chars().count() < min {
        let p = path(parent, key);
        return Err(fail(
            &p,
            if min == 1 {
                format!("{p} must not be empty")
            } else {
                format!("{p} must have at least {min} characters")
            },
        ));
    }
    Ok(s)
}

fn nullable(o: &Map<String, Value>, parent: &str, key: &str, max: usize) -> Checked<Option<String>> {
    let p = path(parent, key);
    match field(o, parent, key)? {
        Value::Null => Ok(None),
        Value::String(s) => {
            long(&p, s, max)?;
            Ok(Some(s.clone()))
        }
        _ => Err(fail(&p, format!("{p} must be a string or null"))),
    }
}

fn one_of<'a>(o: &Map<String, Value>, parent: &str, key: &str, allowed: &[&'a str]) -> Checked<&'a str> {
    let p = path(parent, key);
    let s = string(o, parent, key, 64)?;
    allowed
        .iter()
        .find(|a| **a == s)
        .copied()
        .ok_or_else(|| fail(&p, format!("{p} must be one of {}", allowed.join(", "))))
}

/// A non-negative whole number of milliseconds; anything else (a video time is never a reason
/// to lose the word) reads as unknown.
fn millis(o: &Map<String, Value>, key: &str) -> Option<u64> {
    match o.get(key)? {
        Value::Number(n) => n.as_u64().or_else(|| {
            n.as_f64()
                .filter(|f| *f >= 0.0 && f.fract() == 0.0 && *f < 9.0e15)
                // Checked just above: a whole number below 2^53.
                .map(|f| {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let ms = f as u64;
                    ms
                })
        }),
        _ => None,
    }
}

/// Whether `s` is a lowercase UUIDv7 (`^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-
/// [0-9a-f]{12}$`).
pub fn is_uuid_v7(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 36
        && b.iter().enumerate().all(|(i, &c)| match i {
            8 | 13 | 18 | 23 => c == b'-',
            14 => c == b'7',
            19 => matches!(c, b'8' | b'9' | b'a' | b'b'),
            _ => c.is_ascii_digit() || (b'a'..=b'f').contains(&c),
        })
}

/// RFC 3339 date-time (`YYYY-MM-DDTHH:MM:SS[.fff](Z|±HH:MM)`).
fn is_date_time(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() < 20 {
        return false;
    }
    let digits = |r: std::ops::Range<usize>| b[r].iter().all(u8::is_ascii_digit);
    let head = digits(0..4)
        && b[4] == b'-'
        && digits(5..7)
        && b[7] == b'-'
        && digits(8..10)
        && matches!(b[10], b'T' | b't')
        && digits(11..13)
        && b[13] == b':'
        && digits(14..16)
        && b[16] == b':'
        && digits(17..19);
    if !head {
        return false;
    }
    let mut rest = &s[19..];
    if let Some(frac) = rest.strip_prefix('.') {
        let n = frac.bytes().take_while(u8::is_ascii_digit).count();
        if n == 0 {
            return false;
        }
        rest = &frac[n..];
    }
    match rest.as_bytes() {
        [b'Z' | b'z'] => true,
        [b'+' | b'-', h1, h2, b':', m1, m2] => [h1, h2, m1, m2].iter().all(|c| c.is_ascii_digit()),
        _ => false,
    }
}

/// `scheme:rest`: the minimum of JSON Schema's `format: uri`.
fn is_uri(s: &str) -> bool {
    match s.split_once(':') {
        Some((scheme, rest)) => {
            !rest.is_empty()
                && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
                && scheme
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
                && !s.chars().any(char::is_whitespace)
        }
        None => false,
    }
}

fn media_ref(v: &Value, at: &str) -> Checked<SubtitleMediaRef> {
    let o = object(v, at)?;
    Ok(SubtitleMediaRef {
        upload_id: non_empty(o, at, "upload_id", 1, 200)?,
        mime: string(o, at, "mime", 200)?,
    })
}

/// The capture's `capture_id` if it has a string one (for batch results), else `""`.
pub fn capture_id_of(v: &Value) -> String {
    v.get("capture_id")
        .and_then(Value::as_str)
        .map(|s| s.chars().take(64).collect())
        .unwrap_or_default()
}

/// Checks a capture against schema 1 and reads it.
pub fn parse(v: &Value) -> Checked<SubtitleCapture> {
    let root = object(v, "capture")?;
    match field(root, "", "schema")? {
        Value::Number(n) if n.as_u64() == Some(1) => {}
        _ => return Err(fail("schema", "schema must be 1".into())),
    }
    let capture_id = string(root, "", "capture_id", 64)?;
    if !is_uuid_v7(&capture_id) {
        return Err(fail(
            "capture_id",
            "capture_id must be a lowercase UUIDv7".into(),
        ));
    }
    let kind = match one_of(root, "", "kind", &["word", "phrase", "sentence"])? {
        "word" => SubtitleCaptureKind::Word,
        "phrase" => SubtitleCaptureKind::Phrase,
        _ => SubtitleCaptureKind::Sentence,
    };

    let l = object(field(root, "", "lang")?, "lang")?;
    let lang = SubtitleCaptureLang {
        target: non_empty(l, "lang", "target", 2, 35)?,
        native: non_empty(l, "lang", "native", 2, 35)?,
    };

    let t = object(field(root, "", "term")?, "term")?;
    let term = SubtitleCaptureTerm {
        surface: non_empty(t, "term", "surface", 1, MAX_TERM)?,
        lemma: non_empty(t, "term", "lemma", 1, MAX_TERM)?,
        pos: nullable(t, "term", "pos", 64)?,
    };

    let d = object(field(root, "", "definition")?, "definition")?;
    let definition = SubtitleCaptureDefinition {
        text: string(d, "definition", "text", MAX_TEXT)?,
        source: match one_of(d, "definition", "source", &["dict", "ai", "mt"])? {
            "dict" => SubtitleDefinitionSource::Dict,
            "ai" => SubtitleDefinitionSource::Ai,
            _ => SubtitleDefinitionSource::Mt,
        },
    };

    let c = object(field(root, "", "context")?, "context")?;
    let term_offset = match field(c, "context", "term_offset")? {
        Value::Null => None,
        Value::Array(items) if items.len() == 2 => match (items[0].as_i64(), items[1].as_i64()) {
            (Some(s), Some(e)) => Some((s, e)),
            _ => {
                return Err(fail(
                    "context.term_offset",
                    "context.term_offset must be null or [start, end] integers".into(),
                ))
            }
        },
        _ => {
            return Err(fail(
                "context.term_offset",
                "context.term_offset must be null or [start, end] integers".into(),
            ))
        }
    };
    let context = SubtitleCaptureContext {
        sentence: nullable(c, "context", "sentence", MAX_TEXT)?,
        translation: nullable(c, "context", "translation", MAX_TEXT)?,
        term_offset,
    };

    let s = object(field(root, "", "source")?, "source")?;
    one_of(s, "source", "type", &["video"])?;
    let url = nullable(s, "source", "url", MAX_SHORT)?;
    if url.as_deref().is_some_and(|u| !is_uri(u)) {
        return Err(fail("source.url", "source.url must be a URI or null".into()));
    }
    let source = SubtitleCaptureSource {
        kind: "video".into(),
        url,
        title: nullable(s, "source", "title", MAX_SHORT)?,
        site: string(s, "source", "site", MAX_SHORT)?,
        video_id: nullable(s, "source", "video_id", MAX_SHORT)?,
        t_start_ms: millis(s, "t_start_ms"),
        t_end_ms: millis(s, "t_end_ms"),
    };

    let m = object(field(root, "", "media")?, "media")?;
    // Kinds of media this server does not know are ignored, like unknown fields.
    let media = SubtitleCaptureMedia {
        screenshot: m
            .get("screenshot")
            .map(|v| media_ref(v, "media.screenshot"))
            .transpose()?,
        audio: m.get("audio").map(|v| media_ref(v, "media.audio")).transpose()?,
    };

    let deck_id = match root.get("deck_id") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) if s.chars().count() <= 64 => Some(s.clone()),
        _ => return Err(fail("deck_id", "deck_id must be a string or null".into())),
    };
    let tags = match field(root, "", "tags")? {
        Value::Array(items) if items.len() <= MAX_TAGS => items
            .iter()
            .map(|t| match t {
                Value::String(s) if s.chars().count() <= MAX_TAG => Ok(s.clone()),
                Value::String(_) => Err(fail(
                    "tags",
                    format!("tags must be at most {MAX_TAG} characters each"),
                )),
                _ => Err(fail("tags", "tags must be an array of strings".into())),
            })
            .collect::<Checked<Vec<_>>>()?,
        Value::Array(_) => {
            return Err(fail("tags", format!("at most {MAX_TAGS} tags")));
        }
        _ => return Err(fail("tags", "tags must be an array of strings".into())),
    };
    let created_at = string(root, "", "created_at", 64)?;
    if !is_date_time(&created_at) {
        return Err(fail(
            "created_at",
            "created_at must be an ISO 8601 date-time".into(),
        ));
    }
    let cl = object(field(root, "", "client")?, "client")?;
    one_of(cl, "client", "name", &["subtitle-extension"])?;
    let client = SubtitleCaptureClient {
        name: "subtitle-extension".into(),
        ext_version: string(cl, "client", "ext_version", 64)?,
    };

    Ok(SubtitleCapture {
        schema: 1,
        capture_id,
        kind,
        lang,
        term,
        definition,
        context,
        source,
        media,
        deck_id,
        tags,
        created_at,
        client,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// The example of the contract (docs/flashcards-api.md, "Example").
    pub(crate) fn example() -> Value {
        json!({
          "schema": 1,
          "capture_id": "0192812a-6f3c-7a41-9c2e-5d1f0b8e4a77",
          "kind": "word",
          "lang": { "target": "en", "native": "tr" },
          "term": { "surface": "keys", "lemma": "key", "pos": "noun" },
          "definition": { "text": "anahtar", "source": "dict" },
          "context": {
            "sentence": "Almost. I just need to pick up my keys.",
            "translation": "Neredeyse. Sadece anahtarlarımı almam lazım.",
            "term_offset": [34, 38]
          },
          "source": {
            "type": "video",
            "url": "https://www.youtube.com/watch?v=dQw4w9WgXcQ&t=83s",
            "title": "Never Gonna Give You Up",
            "site": "youtube",
            "video_id": "dQw4w9WgXcQ",
            "t_start_ms": 83450,
            "t_end_ms": 86120
          },
          "media": {
            "screenshot": { "upload_id": "up_01J9ZQ4", "mime": "image/jpeg" },
            "audio": { "upload_id": "up_01J9ZQ5", "mime": "audio/wav" }
          },
          "deck_id": null,
          "tags": ["subtitle"],
          "created_at": "2026-10-12T10:00:00.000Z",
          "client": { "name": "subtitle-extension", "ext_version": "3.1.3" }
        })
    }

    fn with(mut v: Value, pointer: &str, new: Value) -> Value {
        *v.pointer_mut(pointer).unwrap() = new;
        v
    }

    fn without(mut v: Value, parent: &str, key: &str) -> Value {
        let o = if parent.is_empty() {
            v.as_object_mut().unwrap()
        } else {
            v.pointer_mut(parent).unwrap().as_object_mut().unwrap()
        };
        o.remove(key);
        v
    }

    fn message(v: &Value) -> String {
        parse(v).unwrap_err().message
    }

    #[test]
    fn the_contract_example_is_valid() {
        let c = parse(&example()).unwrap();
        assert_eq!(c.capture_id, "0192812a-6f3c-7a41-9c2e-5d1f0b8e4a77");
        assert_eq!(c.term.lemma, "key");
        assert_eq!(c.context.term_offset, Some((34, 38)));
        assert_eq!(c.source.t_start_ms, Some(83_450));
        assert_eq!(
            c.media.audio.map(|m| m.upload_id).as_deref(),
            Some("up_01J9ZQ5")
        );
        assert_eq!(c.tags, ["subtitle"]);
    }

    #[test]
    fn unknown_fields_and_media_kinds_are_ignored() {
        let mut v = example();
        v["future_field"] = json!({"x": 1});
        v["term"]["level"] = json!("B1");
        v["media"]["video"] = json!({"upload_id": "x", "mime": "video/mp4"});
        v = without(v, "", "deck_id");
        v = with(v, "/media", json!({}));
        v = with(v, "/source/t_start_ms", json!(-5));
        v = with(v, "/source/t_end_ms", json!("soon"));
        let c = parse(&v).unwrap();
        assert_eq!(c.deck_id, None);
        assert_eq!(c.media, SubtitleCaptureMedia::default());
        assert_eq!((c.source.t_start_ms, c.source.t_end_ms), (None, None));
    }

    #[test]
    fn invalid_captures_say_why() {
        let e = example;
        for (v, msg) in [
            (with(e(), "/schema", json!(2)), "schema must be 1"),
            (
                with(e(), "/capture_id", json!("not-a-uuid")),
                "capture_id must be a lowercase UUIDv7",
            ),
            (
                with(
                    e(),
                    "/capture_id",
                    json!("0192812a-6f3c-4a41-9c2e-5d1f0b8e4a77"),
                ),
                "capture_id must be a lowercase UUIDv7",
            ),
            (with(e(), "/kind", json!("idiom")), "kind must be one of word, phrase, sentence"),
            (with(e(), "/term/lemma", json!("  ")), "term.lemma must not be empty"),
            (without(e(), "/term", "lemma"), "term.lemma is required"),
            (without(e(), "", "lang"), "lang is required"),
            (with(e(), "/lang/target", json!("e")), "lang.target must have at least 2 characters"),
            (with(e(), "/term/pos", json!(3)), "term.pos must be a string or null"),
            (
                with(e(), "/definition/source", json!("web")),
                "definition.source must be one of dict, ai, mt",
            ),
            (
                with(e(), "/context/term_offset", json!([1])),
                "context.term_offset must be null or [start, end] integers",
            ),
            (with(e(), "/source/type", json!("audio")), "source.type must be one of video"),
            (with(e(), "/source/url", json!("not a url")), "source.url must be a URI or null"),
            (
                with(e(), "/media/audio", json!({"mime": "audio/wav"})),
                "media.audio.upload_id is required",
            ),
            (with(e(), "/media", json!([])), "media must be an object"),
            (with(e(), "/tags", json!("subtitle")), "tags must be an array of strings"),
            (with(e(), "/tags", json!([1])), "tags must be an array of strings"),
            (with(e(), "/deck_id", json!(5)), "deck_id must be a string or null"),
            (
                with(e(), "/created_at", json!("yesterday")),
                "created_at must be an ISO 8601 date-time",
            ),
            (
                with(e(), "/client/name", json!("other")),
                "client.name must be one of subtitle-extension",
            ),
            (json!([1, 2]), "capture must be an object"),
        ] {
            assert_eq!(message(&v), msg, "{v}");
        }
        let err = parse(&without(e(), "/term", "lemma")).unwrap_err();
        assert_eq!(err.code, ErrorCode::ValidationSubtitleCapture);
        assert_eq!(err.details["field"], "term.lemma");
        assert!(message(&with(e(), "/term/lemma", json!("x".repeat(201)))).contains("longer"));
    }

    #[test]
    fn date_times_and_uris() {
        for ok in [
            "2026-10-12T10:00:00Z",
            "2026-10-12T10:00:00.123Z",
            "2026-10-12T10:00:00+03:00",
        ] {
            assert!(is_date_time(ok), "{ok}");
        }
        for bad in ["2026-10-12", "2026-10-12T10:00:00", "2026-10-12T10:00:00.Z"] {
            assert!(!is_date_time(bad), "{bad}");
        }
        assert!(is_uri("https://x.y/z?a=1"));
        assert!(!is_uri("no scheme"));
        assert!(!is_uri("1http://x"));
        assert_eq!(capture_id_of(&json!({"capture_id": "abc"})), "abc");
        assert_eq!(capture_id_of(&json!({"capture_id": 5})), "");
    }
}
