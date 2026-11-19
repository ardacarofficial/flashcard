//! Our JSON note format (docs/02 §15.2): human-editable, one object per note.
//!
//! ```json
//! { "format": "fc-json", "version": 1,
//!   "notes": [ { "deck": "English::A1", "notetype": "Classic",
//!                "fields": { "front": "dog", "back": "köpek" },
//!                "tags": ["animals"], "attributes": { "level": "A1" } } ] }
//! ```
//!
//! `notetype` is a note type id or name (default: Classic); `fields` are keyed by field key or
//! field name; `attributes` by classification key or name, values are option labels (an array
//! for multi-choice), numbers, booleans or text. A bare array of notes is accepted too.

use crate::error::{InteropError, InteropResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// `format` value.
pub const FORMAT: &str = "fc-json";
/// Current version.
pub const VERSION: u32 = 1;

/// A JSON note file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JsonDocument {
    /// Always [`FORMAT`].
    pub format: String,
    /// Format version.
    pub version: u32,
    /// Notes.
    pub notes: Vec<JsonNote>,
}

/// One note.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct JsonNote {
    /// Deck path (`A::B`); `null` = the import's target deck.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub deck: Option<String>,
    /// Note type id or name; `null` = Classic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notetype: Option<String>,
    /// Field values by field key or name.
    pub fields: BTreeMap<String, String>,
    /// Tags.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Classification values by key or name.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub attributes: BTreeMap<String, Value>,
}

impl JsonDocument {
    /// A document of `notes`.
    pub fn new(notes: Vec<JsonNote>) -> Self {
        Self {
            format: FORMAT.into(),
            version: VERSION,
            notes,
        }
    }
}

/// Parses a JSON note file (object form or bare array).
pub fn parse(text: &str) -> InteropResult<JsonDocument> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let value: Value = serde_json::from_str(text).map_err(|e| InteropError::invalid(FORMAT, e))?;
    if value.is_array() {
        let notes: Vec<JsonNote> =
            serde_json::from_value(value).map_err(|e| InteropError::invalid(FORMAT, e))?;
        return Ok(JsonDocument::new(notes));
    }
    let doc: JsonDocument =
        serde_json::from_value(value).map_err(|e| InteropError::invalid(FORMAT, e))?;
    if doc.format != FORMAT {
        return Err(InteropError::invalid(
            FORMAT,
            format!("format is {:?}", doc.format),
        ));
    }
    if doc.version == 0 || doc.version > VERSION {
        return Err(InteropError::UnsupportedVersion {
            format: FORMAT,
            version: doc.version.to_string(),
        });
    }
    Ok(doc)
}

/// Pretty-printed JSON.
pub fn write(doc: &JsonDocument) -> InteropResult<String> {
    serde_json::to_string_pretty(doc).map_err(|e| InteropError::invalid(FORMAT, e))
}

/// Text of a classification value for the import mapper: labels, numbers, booleans; arrays
/// become `|`-joined labels (multi-choice).
pub fn value_text(v: &Value) -> Option<String> {
    match v {
        Value::Null => None,
        Value::String(s) => Some(s.clone()),
        Value::Bool(b) => Some(b.to_string()),
        Value::Number(n) => Some(n.to_string()),
        Value::Array(items) => Some(
            items
                .iter()
                .filter_map(value_text)
                .collect::<Vec<_>>()
                .join("|"),
        ),
        Value::Object(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_both_forms_and_roundtrips() {
        let doc = JsonDocument::new(vec![JsonNote {
            deck: Some("A::B".into()),
            fields: [("front".to_string(), "dog".to_string())].into(),
            tags: vec!["x".into()],
            attributes: [("level".to_string(), Value::from("A1"))].into(),
            ..JsonNote::default()
        }]);
        let text = write(&doc).unwrap();
        assert_eq!(parse(&text).unwrap(), doc);
        let bare = parse(r#"[{"fields":{"front":"a","back":"b"}}]"#).unwrap();
        assert_eq!(bare.notes.len(), 1);
        assert!(parse(r#"{"format":"other","version":1,"notes":[]}"#).is_err());
        assert!(matches!(
            parse(r#"{"format":"fc-json","version":9,"notes":[]}"#),
            Err(InteropError::UnsupportedVersion { .. })
        ));
        assert_eq!(
            value_text(&serde_json::json!(["a", "b"])).as_deref(),
            Some("a|b")
        );
        assert_eq!(value_text(&serde_json::json!(3)).as_deref(), Some("3"));
    }
}
