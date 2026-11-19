//! Notes: the data a user enters. Cards are derived from notes via their note type.

use crate::error::{CoreError, CoreResult};
use crate::id::{DeckId, NoteId, NoteTypeId};
use crate::notetype::{FieldKey, NoteType};
use crate::tag::TagName;
use crate::time::Timestamp;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Field values keyed by [`FieldKey`]. Stored as JSON in `notes.fields`.
pub type FieldValues = BTreeMap<FieldKey, String>;

/// A note.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Note {
    /// Id.
    pub id: NoteId,
    /// Note type.
    pub notetype_id: NoteTypeId,
    /// Home deck (`notes.home_deck_id`): note-level classifications resolve from here.
    /// Set on create, changeable on update; cards keep their own `deck_id`.
    pub home_deck_id: DeckId,
    /// Values for every field of the note type (missing keys are filled by
    /// [`normalize_note_fields`]).
    pub fields: FieldValues,
    /// Derived: stripped text of the sort field ([`sort_field_text`]).
    pub sort_field: String,
    /// Derived: duplicate-detection checksum ([`note_checksum`]).
    pub checksum: String,
    /// Tags (source of truth is the `note_tags` relation; loaded alongside).
    pub tags: Vec<TagName>,
    /// Creation time.
    pub created_at: Timestamp,
    /// Last modification time.
    pub updated_at: Timestamp,
    /// Soft-delete time.
    pub deleted_at: Option<Timestamp>,
}

/// Validates `input` against the note type and returns a complete value map:
/// unknown keys are rejected, missing fields get their default (or `""`), required fields must
/// have visible text.
pub fn normalize_note_fields(nt: &NoteType, input: &FieldValues) -> CoreResult<FieldValues> {
    if let Some(unknown) = input.keys().find(|k| nt.field(k).is_none()) {
        return Err(CoreError::UnknownField {
            field: unknown.0.clone(),
        });
    }
    let mut out = FieldValues::new();
    for f in &nt.fields {
        let value = input
            .get(&f.key)
            .cloned()
            .or_else(|| f.default_value.clone())
            .unwrap_or_default();
        if f.required && strip_html(&value).is_empty() {
            return Err(CoreError::RequiredFieldEmpty {
                field: f.key.0.clone(),
            });
        }
        out.insert(f.key.clone(), value);
    }
    Ok(out)
}

/// Stripped text of the note type's sort field.
pub fn sort_field_text(nt: &NoteType, fields: &FieldValues) -> String {
    fields
        .get(&nt.sort_field)
        .map(|v| strip_html(v))
        .unwrap_or_default()
}

/// Duplicate-detection checksum: FNV-1a 64 (16 hex chars) of the normalized text of the
/// note type's `unique` fields (falling back to the sort field). Normalization: HTML stripped,
/// whitespace collapsed, Unicode lowercase. Two notes of the same type with equal checksums are
/// duplicate candidates (`CONFLICT_NOTE_DUPLICATE`).
pub fn note_checksum(nt: &NoteType, fields: &FieldValues) -> String {
    let mut keys: Vec<&FieldKey> = nt
        .fields
        .iter()
        .filter(|f| f.unique)
        .map(|f| &f.key)
        .collect();
    if keys.is_empty() {
        keys.push(&nt.sort_field);
    }
    let joined = keys
        .iter()
        .map(|k| {
            fields
                .get(*k)
                .map(|v| strip_html(v).to_lowercase())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join("\u{1f}");
    format!("{:016x}", fnv1a64(joined.as_bytes()))
}

/// Text indexed in `notes_fts`: stripped values of `searchable` fields, newline-separated.
pub fn searchable_text(nt: &NoteType, fields: &FieldValues) -> String {
    nt.fields
        .iter()
        .filter(|f| f.searchable)
        .filter_map(|f| fields.get(&f.key))
        .map(|v| strip_html(v))
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Removes HTML tags, decodes the common entities and collapses whitespace. Not a sanitizer:
/// only for derived text (sort field, checksum, search index).
pub fn strip_html(input: &str) -> String {
    let mut text = String::with_capacity(input.len());
    let mut in_tag = false;
    for c in input.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => {
                in_tag = false;
                text.push(' ');
            }
            _ if !in_tag => text.push(c),
            _ => {}
        }
    }
    let decoded = text
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&");
    decoded.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// 64-bit FNV-1a hash. Stable across platforms and releases (stored in the database).
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notetype::builtin;

    fn nt() -> NoteType {
        builtin::classic(Timestamp(1), "Classic", "Front", "Back")
    }

    fn fv(pairs: &[(&str, &str)]) -> FieldValues {
        pairs
            .iter()
            .map(|(k, v)| (FieldKey::new(*k), (*v).to_owned()))
            .collect()
    }

    #[test]
    fn strip_html_basics() {
        assert_eq!(
            strip_html("<b>Hello</b>&nbsp;&amp; <i>world</i>"),
            "Hello & world"
        );
        assert_eq!(strip_html("a<br>b"), "a b");
        assert_eq!(strip_html("  x \n\t y "), "x y");
        assert_eq!(strip_html("1 &lt; 2"), "1 < 2");
        assert_eq!(strip_html("<img src=x>"), "");
    }

    #[test]
    fn normalize_fills_and_validates() {
        let out = normalize_note_fields(&nt(), &fv(&[("front", "dog")])).unwrap();
        assert_eq!(out.get(&FieldKey::new("back")).unwrap(), "");
        assert!(matches!(
            normalize_note_fields(&nt(), &fv(&[("front", " <br> ")])),
            Err(CoreError::RequiredFieldEmpty { .. })
        ));
        assert!(matches!(
            normalize_note_fields(&nt(), &fv(&[("front", "x"), ("nope", "y")])),
            Err(CoreError::UnknownField { .. })
        ));
    }

    #[test]
    fn checksum_ignores_markup_case_and_spacing() {
        let a = note_checksum(&nt(), &fv(&[("front", "<b>Dog</b>"), ("back", "köpek")]));
        let b = note_checksum(&nt(), &fv(&[("front", " dog "), ("back", "other")]));
        let c = note_checksum(&nt(), &fv(&[("front", "cat")]));
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(a.len(), 16);
    }

    #[test]
    fn fnv_reference_values() {
        assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
    }

    #[test]
    fn sort_and_search_text() {
        let f = fv(&[("front", "<i>run</i>"), ("back", "koşmak")]);
        assert_eq!(sort_field_text(&nt(), &f), "run");
        assert_eq!(searchable_text(&nt(), &f), "run\nkoşmak");
    }
}
