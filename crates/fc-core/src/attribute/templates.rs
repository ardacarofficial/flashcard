//! Built-in classification templates (CEFR, YKS, JLPT, HSK, school grade, difficulty).
//!
//! Templates are pure data. [`instantiate_template`] copies one into a regular
//! [`AttributeDef`] with fresh ids; from then on it belongs to the user (rename, reorder, add
//! options freely). Updating a template never changes existing definitions.

use super::{AttributeDef, AttributeKind, AttributeOption, AttributeTarget};
use crate::i18n::{Locale, LocalizedText};
use crate::id::{AttributeDefId, AttributeOptionId, DeckId, IdGen};
use crate::time::Timestamp;

/// A built-in template.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AttributeTemplate {
    /// Stable template id (stored as `attribute_defs.template_id`).
    pub id: &'static str,
    /// Suggested key.
    pub key: &'static str,
    /// Display name.
    pub name: LocalizedText,
    /// Value type.
    pub kind: AttributeKind,
    /// Options in order.
    pub options: &'static [LocalizedText],
}

const fn same(s: &'static str) -> LocalizedText {
    LocalizedText::same(s)
}

const CEFR: &[LocalizedText] = &[
    same("A1"),
    same("A2"),
    same("B1"),
    same("B2"),
    same("C1"),
    same("C2"),
];
const YKS: &[LocalizedText] = &[same("TYT"), same("AYT"), same("YDT")];
const JLPT: &[LocalizedText] = &[same("N5"), same("N4"), same("N3"), same("N2"), same("N1")];
const HSK: &[LocalizedText] = &[
    same("HSK 1"),
    same("HSK 2"),
    same("HSK 3"),
    same("HSK 4"),
    same("HSK 5"),
    same("HSK 6"),
];
const GRADES: &[LocalizedText] = &[
    LocalizedText {
        en: "Grade 1",
        tr: "1. sınıf",
    },
    LocalizedText {
        en: "Grade 2",
        tr: "2. sınıf",
    },
    LocalizedText {
        en: "Grade 3",
        tr: "3. sınıf",
    },
    LocalizedText {
        en: "Grade 4",
        tr: "4. sınıf",
    },
    LocalizedText {
        en: "Grade 5",
        tr: "5. sınıf",
    },
    LocalizedText {
        en: "Grade 6",
        tr: "6. sınıf",
    },
    LocalizedText {
        en: "Grade 7",
        tr: "7. sınıf",
    },
    LocalizedText {
        en: "Grade 8",
        tr: "8. sınıf",
    },
    LocalizedText {
        en: "Grade 9",
        tr: "9. sınıf",
    },
    LocalizedText {
        en: "Grade 10",
        tr: "10. sınıf",
    },
    LocalizedText {
        en: "Grade 11",
        tr: "11. sınıf",
    },
    LocalizedText {
        en: "Grade 12",
        tr: "12. sınıf",
    },
];
const DIFFICULTY: &[LocalizedText] = &[
    LocalizedText {
        en: "Easy",
        tr: "Kolay",
    },
    LocalizedText {
        en: "Medium",
        tr: "Orta",
    },
    LocalizedText {
        en: "Hard",
        tr: "Zor",
    },
];

const TEMPLATES: &[AttributeTemplate] = &[
    AttributeTemplate {
        id: "cefr",
        key: "level",
        name: LocalizedText {
            en: "Level",
            tr: "Seviye",
        },
        kind: AttributeKind::OrderedEnum,
        options: CEFR,
    },
    AttributeTemplate {
        id: "yks",
        key: "exam",
        name: LocalizedText {
            en: "Exam",
            tr: "Sınav",
        },
        kind: AttributeKind::Enum,
        options: YKS,
    },
    AttributeTemplate {
        id: "jlpt",
        key: "jlpt",
        name: same("JLPT"),
        kind: AttributeKind::OrderedEnum,
        options: JLPT,
    },
    AttributeTemplate {
        id: "hsk",
        key: "hsk",
        name: same("HSK"),
        kind: AttributeKind::OrderedEnum,
        options: HSK,
    },
    AttributeTemplate {
        id: "school_grade",
        key: "grade",
        name: LocalizedText {
            en: "Grade",
            tr: "Sınıf",
        },
        kind: AttributeKind::OrderedEnum,
        options: GRADES,
    },
    AttributeTemplate {
        id: "difficulty",
        key: "difficulty",
        name: LocalizedText {
            en: "Difficulty",
            tr: "Zorluk",
        },
        kind: AttributeKind::OrderedEnum,
        options: DIFFICULTY,
    },
];

/// All built-in templates.
pub fn builtin_templates() -> &'static [AttributeTemplate] {
    TEMPLATES
}

/// Finds a built-in template by id.
pub fn find_template(id: &str) -> Option<&'static AttributeTemplate> {
    TEMPLATES.iter().find(|t| t.id == id)
}

/// Copies `template` into a new definition with fresh ids and labels in `locale`.
pub fn instantiate_template(
    template: &AttributeTemplate,
    scope_deck_id: Option<DeckId>,
    locale: Locale,
    position: i32,
    ids: &mut dyn IdGen,
    now: Timestamp,
) -> AttributeDef {
    let options = template
        .options
        .iter()
        .enumerate()
        .map(|(i, label)| AttributeOption {
            id: AttributeOptionId::generate(ids),
            label: label.get(locale).to_owned(),
            color: None,
            order: i32::try_from(i).unwrap_or(i32::MAX),
        })
        .collect();
    AttributeDef {
        id: AttributeDefId::generate(ids),
        scope_deck_id,
        key: template.key.to_owned(),
        name: template.name.get(locale).to_owned(),
        kind: template.kind,
        options,
        target: AttributeTarget::Note,
        required: false,
        default_value: None,
        position,
        template_id: Some(template.id.to_owned()),
        created_at: now,
        updated_at: now,
        deleted_at: None,
    }
}
