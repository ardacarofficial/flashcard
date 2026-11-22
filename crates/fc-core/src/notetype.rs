//! Note types: a field schema plus card templates. JSON-serializable as stored in
//! `notetypes.fields` / `notetypes.templates`.

use crate::error::{CoreError, CoreResult};
use crate::id::NoteTypeId;
use crate::time::Timestamp;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;
use uuid::Uuid;

/// Stable key of a field inside its note type. Note values are stored as `{field_key: value}`,
/// so renaming a field's display name never touches notes.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct FieldKey(pub String);

impl FieldKey {
    /// Creates a key from a string (no validation; see [`validate_notetype`]).
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// The key as `&str`.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for FieldKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// What a field holds. Drives the editor widget and sanitization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum FieldKind {
    /// Plain text.
    Text,
    /// Markdown / sanitized HTML.
    RichText,
    /// Media reference (image).
    Image,
    /// Media reference (audio).
    Audio,
    /// Media reference (video).
    Video,
    /// Numeric value.
    Number,
    /// List of tags.
    TagList,
    /// Link to another note.
    Link,
}

/// What a field **contains** (v1.3, docs/03 §5.1): optional metadata that lets study, the term
/// list, hints and imports treat vocabulary notes as vocabulary without guessing from field
/// names. A field has at most one role and a role belongs to at most one field of a note type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum FieldRole {
    /// The dictionary form ("go").
    Term,
    /// The form as it was seen ("went").
    Surface,
    /// What the term means ("gitmek").
    Meaning,
    /// A sentence containing the term ("I went home.").
    Example,
    /// The example sentence in the learner's language.
    ExampleTranslation,
    /// Where it came from (title, link, time).
    Source,
    /// Anything else worth keeping (explanation, extra).
    Note,
}

impl FieldRole {
    /// Every role, in the order a term is read (and columns are mapped).
    pub const ALL: [FieldRole; 7] = [
        Self::Term,
        Self::Meaning,
        Self::Example,
        Self::ExampleTranslation,
        Self::Surface,
        Self::Source,
        Self::Note,
    ];

    /// Stable id (the serialized form).
    pub const fn id(self) -> &'static str {
        match self {
            Self::Term => "term",
            Self::Surface => "surface",
            Self::Meaning => "meaning",
            Self::Example => "example",
            Self::ExampleTranslation => "example_translation",
            Self::Source => "source",
            Self::Note => "note",
        }
    }

    /// Parses an id (case-insensitive, surrounding spaces ignored).
    pub fn parse(id: &str) -> Option<Self> {
        let id = id.trim();
        Self::ALL
            .into_iter()
            .find(|r| r.id().eq_ignore_ascii_case(id))
    }
}

/// One field in a note type schema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct FieldDef {
    /// Stable key (unique in the note type).
    pub key: FieldKey,
    /// Display name (user text, not an i18n key).
    pub name: String,
    /// Field kind.
    pub kind: FieldKind,
    /// Must be non-empty.
    pub required: bool,
    /// Participates in duplicate detection (checksum).
    pub unique: bool,
    /// Indexed for full-text search.
    pub searchable: bool,
    /// Pre-filled value for new notes.
    pub default_value: Option<String>,
    /// What the field contains (v1.3; absent in older records = no role).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub role: Option<FieldRole>,
}

/// Whether a note type produces one card per template or one card per cloze deletion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum NoteTypeKind {
    /// One card per template.
    Standard,
    /// One card per cloze number (v0.3).
    Cloze,
}

/// A card template. Template syntax (`{{field}}`, `{{#f}}…{{/f}}`, …) is defined in v0.3.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct CardTemplate {
    /// Stable key (unique in the note type); stored on cards as `template_key`.
    pub key: String,
    /// Display name.
    pub name: String,
    /// Question side.
    pub front: String,
    /// Answer side.
    pub back: String,
    /// Card type engine id (`classic`, `typing`, …) used to render/check this card.
    pub card_type: String,
    /// The field this card asks for (v1.3): typed answers, tests and hints compare with its
    /// text instead of the whole answer side. Choice, cloze and typing engines keep their own
    /// expected answer. Absent in older records = the text of the answer side.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[cfg_attr(feature = "ts", ts(optional = nullable))]
    pub answer_field: Option<FieldKey>,
}

/// A note type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NoteType {
    /// Id.
    pub id: NoteTypeId,
    /// Display name.
    pub name: String,
    /// Standard or cloze.
    pub kind: NoteTypeKind,
    /// Field schema, in display order.
    pub fields: Vec<FieldDef>,
    /// Card templates, in order.
    pub templates: Vec<CardTemplate>,
    /// Shared CSS for templates.
    pub css: String,
    /// Field whose stripped text is shown in lists and used for sorting.
    pub sort_field: FieldKey,
    /// Bumped on every schema change.
    pub version: u32,
    /// Built-in identifier (`classic`, …) for shipped note types; `None` for user types.
    pub builtin: Option<String>,
    /// Creation time.
    pub created_at: Timestamp,
    /// Last modification time.
    pub updated_at: Timestamp,
    /// Soft-delete time.
    pub deleted_at: Option<Timestamp>,
}

impl NoteType {
    /// Looks up a field by key.
    pub fn field(&self, key: &FieldKey) -> Option<&FieldDef> {
        self.fields.iter().find(|f| &f.key == key)
    }

    /// The field with `role`, if any.
    pub fn field_with_role(&self, role: FieldRole) -> Option<&FieldDef> {
        self.fields.iter().find(|f| f.role == Some(role))
    }

    /// Whether any field has a role.
    pub fn has_roles(&self) -> bool {
        self.fields.iter().any(|f| f.role.is_some())
    }
}

/// Validates a note type's internal consistency.
pub fn validate_notetype(nt: &NoteType) -> CoreResult<()> {
    let bad = |reason: &str| {
        Err(CoreError::InvalidNoteType {
            reason: reason.to_owned(),
        })
    };
    if nt.name.trim().is_empty() {
        return bad("empty name");
    }
    if nt.fields.is_empty() {
        return bad("no fields");
    }
    let mut keys = HashSet::new();
    let mut roles = HashSet::new();
    for f in &nt.fields {
        if f.key.0.trim().is_empty() || f.name.trim().is_empty() {
            return bad("empty field key or name");
        }
        if !keys.insert(&f.key) {
            return bad("duplicate field key");
        }
        if let Some(r) = f.role {
            if !roles.insert(r) {
                return bad("two fields have the same role");
            }
        }
    }
    if nt.field(&nt.sort_field).is_none() {
        return bad("sort field does not exist");
    }
    if nt.kind == NoteTypeKind::Standard && nt.templates.is_empty() {
        return bad("no templates");
    }
    let mut tkeys = HashSet::new();
    for t in &nt.templates {
        if t.key.trim().is_empty() || !tkeys.insert(&t.key) {
            return bad("empty or duplicate template key");
        }
        if t.answer_field
            .as_ref()
            .is_some_and(|k| nt.field(k).is_none())
        {
            return bad("answer field does not exist");
        }
    }
    Ok(())
}

/// Built-in note type identifiers and definitions.
pub mod builtin {
    use super::*;

    /// `builtin` marker of the Classic (Front/Back) note type.
    pub const CLASSIC: &str = "classic";

    /// Well-known id of the built-in Classic note type. The same in every collection so imports
    /// and tests can refer to it; seeded by the app on first open.
    pub const CLASSIC_ID: NoteTypeId =
        NoteTypeId(Uuid::from_u128(0x0190_0000_0000_7000_8000_0000_0000_0001));

    /// Field key: question side.
    pub const FRONT: &str = "front";
    /// Field key: answer side.
    pub const BACK: &str = "back";
    /// Template key of the single Classic card.
    pub const FORWARD: &str = "forward";

    /// Classic (Front/Back) note type. `name`s are the localized display names chosen at
    /// seeding time (the core has no i18n catalog; see `docs/03` §7 message keys).
    pub fn classic(now: Timestamp, name: &str, front_name: &str, back_name: &str) -> NoteType {
        let field = |key: &str, display: &str, required: bool, unique: bool, role| FieldDef {
            key: FieldKey::new(key),
            name: display.to_owned(),
            kind: FieldKind::RichText,
            required,
            unique,
            searchable: true,
            default_value: None,
            role: Some(role),
        };
        NoteType {
            id: CLASSIC_ID,
            name: name.to_owned(),
            kind: NoteTypeKind::Standard,
            fields: vec![
                field(FRONT, front_name, true, true, FieldRole::Term),
                field(BACK, back_name, false, false, FieldRole::Meaning),
            ],
            templates: vec![CardTemplate {
                key: FORWARD.to_owned(),
                name: "1".to_owned(),
                front: "{{front}}".to_owned(),
                back: "{{front}}\n<hr id=\"answer\">\n{{back}}".to_owned(),
                card_type: "classic".to_owned(),
                answer_field: Some(FieldKey::new(BACK)),
            }],
            css: String::new(),
            sort_field: FieldKey::new(FRONT),
            version: 1,
            builtin: Some(CLASSIC.to_owned()),
            created_at: now,
            updated_at: now,
            deleted_at: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classic_is_valid_and_json_roundtrips() {
        let nt = builtin::classic(Timestamp(1), "Classic", "Front", "Back");
        validate_notetype(&nt).unwrap();
        assert_eq!(nt.id.0.get_version_num(), 7);
        let json = serde_json::to_string(&nt.fields).unwrap();
        let back: Vec<FieldDef> = serde_json::from_str(&json).unwrap();
        assert_eq!(back, nt.fields);
        assert!(json.contains("\"kind\":\"rich_text\""));
    }

    #[test]
    fn detects_inconsistencies() {
        let base = builtin::classic(Timestamp(1), "Classic", "Front", "Back");

        let mut dup = base.clone();
        dup.fields[1].key = FieldKey::new("front");
        assert!(validate_notetype(&dup).is_err());

        let mut no_sort = base.clone();
        no_sort.sort_field = FieldKey::new("missing");
        assert!(validate_notetype(&no_sort).is_err());

        let mut no_tpl = base.clone();
        no_tpl.templates.clear();
        assert!(validate_notetype(&no_tpl).is_err());

        let mut no_fields = base;
        no_fields.fields.clear();
        assert!(validate_notetype(&no_fields).is_err());
    }

    #[test]
    fn roles_and_answer_fields_are_optional_metadata() {
        // A v1.2 record (no `role`, no `answer_field`) loads unchanged.
        let old_field = r#"{"key":"front","name":"Front","kind":"rich_text","required":true,
            "unique":true,"searchable":true,"default_value":null}"#;
        let f: FieldDef = serde_json::from_str(old_field).unwrap();
        assert_eq!(f.role, None);
        // ...and writes back without the new keys.
        assert!(!serde_json::to_string(&f).unwrap().contains("role"));
        let old_tpl = r#"{"key":"forward","name":"1","front":"{{front}}","back":"{{back}}",
            "card_type":"classic"}"#;
        let t: CardTemplate = serde_json::from_str(old_tpl).unwrap();
        assert_eq!(t.answer_field, None);
        assert!(!serde_json::to_string(&t).unwrap().contains("answer_field"));

        // New records round-trip.
        let nt = builtin::classic(Timestamp(1), "Classic", "Front", "Back");
        assert_eq!(nt.fields[0].role, Some(FieldRole::Term));
        assert_eq!(nt.fields[1].role, Some(FieldRole::Meaning));
        let json = serde_json::to_string(&nt.fields).unwrap();
        assert!(json.contains("\"role\":\"term\""));
        assert_eq!(
            serde_json::from_str::<Vec<FieldDef>>(&json).unwrap(),
            nt.fields
        );
        let json = serde_json::to_string(&nt.templates).unwrap();
        assert!(json.contains("\"answer_field\":\"back\""));
        assert_eq!(
            serde_json::from_str::<Vec<CardTemplate>>(&json).unwrap(),
            nt.templates
        );
        assert_eq!(
            nt.field_with_role(FieldRole::Meaning)
                .map(|f| f.key.as_str()),
            Some("back")
        );
        assert_eq!(nt.field_with_role(FieldRole::Example), None);
        assert!(nt.has_roles());
    }

    #[test]
    fn role_ids_parse() {
        for r in FieldRole::ALL {
            assert_eq!(FieldRole::parse(r.id()), Some(r));
            assert_eq!(
                serde_json::to_string(&r).unwrap(),
                format!("\"{}\"", r.id())
            );
        }
        assert_eq!(
            FieldRole::parse(" Example_Translation "),
            Some(FieldRole::ExampleTranslation)
        );
        assert_eq!(FieldRole::parse("definition"), None);
    }

    #[test]
    fn roles_and_answer_fields_are_validated() {
        let base = builtin::classic(Timestamp(1), "Classic", "Front", "Back");
        let mut twice = base.clone();
        twice.fields[1].role = Some(FieldRole::Term);
        assert!(validate_notetype(&twice).is_err());

        let mut missing = base.clone();
        missing.templates[0].answer_field = Some(FieldKey::new("nope"));
        assert!(validate_notetype(&missing).is_err());

        let mut none = base;
        none.fields[0].role = None;
        none.templates[0].answer_field = None;
        validate_notetype(&none).unwrap();
    }
}
