//! Domain errors produced by pure core functions.
//!
//! These are *not* the wire error model; `fc-app` maps each variant to an `fc_api::ApiError`
//! (`VALIDATION_*` etc.). Every variant carries enough data to build the error `details`.

use thiserror::Error;

/// Result alias for core functions.
pub type CoreResult<T> = Result<T, CoreError>;

/// A domain rule was violated.
#[derive(Debug, Clone, PartialEq, Error)]
#[non_exhaustive]
pub enum CoreError {
    /// A string could not be parsed as the given id type.
    #[error("invalid {kind}: {value:?}")]
    InvalidId {
        /// Id type name, e.g. `DeckId`.
        kind: &'static str,
        /// Offending input.
        value: String,
    },
    /// Deck name / path is empty, too long, or contains forbidden characters.
    #[error("invalid deck name {name:?}: {reason}")]
    InvalidDeckName {
        /// Offending input.
        name: String,
        /// Machine-readable reason.
        reason: NameProblem,
    },
    /// Tag name is empty, too long, or contains forbidden characters.
    #[error("invalid tag {name:?}: {reason}")]
    InvalidTagName {
        /// Offending input.
        name: String,
        /// Machine-readable reason.
        reason: NameProblem,
    },
    /// A note field is not defined by its note type.
    #[error("unknown field {field:?}")]
    UnknownField {
        /// Field key.
        field: String,
    },
    /// A required note field is empty.
    #[error("required field {field:?} is empty")]
    RequiredFieldEmpty {
        /// Field key.
        field: String,
    },
    /// Note type definition is inconsistent (duplicate keys, no templates, …).
    #[error("invalid note type: {reason}")]
    InvalidNoteType {
        /// Human-readable reason (for logs; the code is what matters).
        reason: String,
    },
    /// Attribute definition is inconsistent.
    #[error("invalid attribute definition: {reason}")]
    InvalidAttributeDef {
        /// Machine-readable reason.
        reason: AttributeDefProblem,
    },
    /// An attribute value does not fit its definition.
    #[error("invalid value for attribute {key:?}: {reason}")]
    InvalidAttributeValue {
        /// Attribute key.
        key: String,
        /// Machine-readable reason.
        reason: AttributeValueProblem,
    },
    /// A value was given for an attribute that does not apply to the entity's deck/target.
    #[error("attribute {id} does not apply here")]
    AttributeNotApplicable {
        /// Attribute definition id (string form).
        id: String,
    },
    /// A required attribute has no value.
    #[error("attribute {key:?} is required")]
    AttributeRequired {
        /// Attribute key.
        key: String,
    },
    /// An option id does not belong to the attribute.
    #[error("unknown option {option} for attribute {key:?}")]
    UnknownAttributeOption {
        /// Attribute key.
        key: String,
        /// Option id (string form).
        option: String,
    },
    /// A setting value does not satisfy its schema.
    #[error("invalid value for setting {key:?}: {reason}")]
    InvalidSettingValue {
        /// Setting key.
        key: String,
        /// Human-readable reason.
        reason: String,
    },
    /// Moving a deck under itself or one of its descendants.
    #[error("deck move would create a cycle")]
    DeckCycle,
}

/// Why a name (deck segment, tag segment, attribute key) was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameProblem {
    /// Empty after trimming.
    Empty,
    /// Exceeds the maximum length.
    TooLong,
    /// Contains a control character or a forbidden character.
    ForbiddenChar,
    /// Has an empty `::` segment (e.g. `A::::B`, `::A`).
    EmptySegment,
}

impl std::fmt::Display for NameProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Empty => "empty",
            Self::TooLong => "too_long",
            Self::ForbiddenChar => "forbidden_char",
            Self::EmptySegment => "empty_segment",
        })
    }
}

/// Why an attribute definition was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttributeDefProblem {
    /// `key` is not a valid slug (`[a-z0-9_]`, starts with a letter, ≤ 64 chars).
    InvalidKey,
    /// Display name is empty or too long.
    InvalidName,
    /// An enum kind has no options.
    NoOptions,
    /// A non-enum kind has options.
    OptionsNotAllowed,
    /// Two options share an id.
    DuplicateOptionId,
    /// Two options share a label (case-insensitive).
    DuplicateOptionLabel(String),
    /// An option label is empty or too long.
    InvalidOptionLabel,
    /// The default value is not valid for the definition.
    InvalidDefault,
    /// A reorder list is not a permutation of the existing option ids.
    InvalidOrder,
}

impl std::fmt::Display for AttributeDefProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidKey => f.write_str("invalid_key"),
            Self::InvalidName => f.write_str("invalid_name"),
            Self::NoOptions => f.write_str("no_options"),
            Self::OptionsNotAllowed => f.write_str("options_not_allowed"),
            Self::DuplicateOptionId => f.write_str("duplicate_option_id"),
            Self::DuplicateOptionLabel(l) => write!(f, "duplicate_option_label({l})"),
            Self::InvalidOptionLabel => f.write_str("invalid_option_label"),
            Self::InvalidDefault => f.write_str("invalid_default"),
            Self::InvalidOrder => f.write_str("invalid_order"),
        }
    }
}

/// Why an attribute value was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttributeValueProblem {
    /// Value variant does not match the attribute kind.
    KindMismatch,
    /// Multi-select with no options or duplicate options.
    BadSelection,
    /// Number is NaN/infinite.
    NotFinite,
    /// Text is empty or too long.
    BadText,
    /// Date is not a valid `YYYY-MM-DD` calendar date.
    BadDate,
}

impl std::fmt::Display for AttributeValueProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::KindMismatch => "kind_mismatch",
            Self::BadSelection => "bad_selection",
            Self::NotFinite => "not_finite",
            Self::BadText => "bad_text",
            Self::BadDate => "bad_date",
        })
    }
}
