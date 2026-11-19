//! Wire error model (docs/03 §7).
//!
//! ```json
//! { "code": "CONFLICT_NOTE_DUPLICATE", "message_key": "errors.conflict.note_duplicate",
//!   "message": "…", "details": { "existing_note_id": "…" }, "retryable": false }
//! ```
//!
//! The frontend branches on `code` (stable, documented) and renders `message_key` through
//! i18n with `details` as parameters. `message` is English developer text for logs.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::fmt;
use ts_rs::TS;

/// Category prefix of an [`ErrorCode`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCategory {
    /// Input is malformed or violates a rule. Fix the input.
    Validation,
    /// Referenced entity/method does not exist.
    NotFound,
    /// Request is valid but conflicts with current state.
    Conflict,
    /// Caller lacks permission (plugins, local API tokens).
    Permission,
    /// External provider (AI, TTS, …) failed.
    Provider,
    /// Disk/database failure.
    Io,
    /// Bug or unimplemented feature.
    Internal,
}

macro_rules! error_codes {
    ($( $(#[$m:meta])* $variant:ident => $cat:ident, $key:literal; )*) => {
        /// Stable error codes. **Never rename or reuse a code**; add new ones instead.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
        #[serde(rename_all = "SCREAMING_SNAKE_CASE")]
        #[non_exhaustive]
        pub enum ErrorCode {
            $( $(#[$m])* $variant, )*
        }

        impl ErrorCode {
            /// All codes (for docs, tests and i18n catalog checks).
            pub const ALL: &'static [ErrorCode] = &[ $( ErrorCode::$variant, )* ];

            /// Category of the code.
            pub const fn category(self) -> ErrorCategory {
                match self { $( ErrorCode::$variant => ErrorCategory::$cat, )* }
            }

            /// i18n key of the user-facing message (`errors.<category>.<name>`).
            pub const fn message_key(self) -> &'static str {
                match self { $( ErrorCode::$variant => $key, )* }
            }
        }
    };
}

error_codes! {
    /// Request params do not match the method's request type.
    ValidationInvalidParams => Validation, "errors.validation.invalid_params";
    /// Deck name/path rejected (`details.reason`).
    ValidationDeckName => Validation, "errors.validation.deck_name";
    /// Tag name rejected (`details.reason`).
    ValidationTagName => Validation, "errors.validation.tag_name";
    /// A required note field is empty (`details.field`).
    ValidationFieldRequired => Validation, "errors.validation.field_required";
    /// Unknown note field (`details.field`).
    ValidationUnknownField => Validation, "errors.validation.unknown_field";
    /// Note type definition rejected.
    ValidationNoteType => Validation, "errors.validation.note_type";
    /// Attribute definition rejected (`details.reason`).
    ValidationAttributeDef => Validation, "errors.validation.attribute_def";
    /// Attribute value rejected (`details.key`, `details.reason`).
    ValidationAttributeValue => Validation, "errors.validation.attribute_value";
    /// Required attribute missing (`details.key`).
    ValidationAttributeRequired => Validation, "errors.validation.attribute_required";
    /// Attribute does not apply to this deck/target (`details.id`).
    ValidationAttributeNotApplicable => Validation, "errors.validation.attribute_not_applicable";
    /// Setting value rejected (`details.key`).
    ValidationSettingValue => Validation, "errors.validation.setting_value";
    /// Unknown method name.
    NotFoundMethod => NotFound, "errors.not_found.method";
    /// Deck not found (`details.id`).
    NotFoundDeck => NotFound, "errors.not_found.deck";
    /// Note not found (`details.id`).
    NotFoundNote => NotFound, "errors.not_found.note";
    /// Card not found (`details.id`).
    NotFoundCard => NotFound, "errors.not_found.card";
    /// Note type not found (`details.id`).
    NotFoundNoteType => NotFound, "errors.not_found.note_type";
    /// Tag not found (`details.name`).
    NotFoundTag => NotFound, "errors.not_found.tag";
    /// Attribute definition not found (`details.id`).
    NotFoundAttribute => NotFound, "errors.not_found.attribute";
    /// Attribute option not found (`details.id`).
    NotFoundAttributeOption => NotFound, "errors.not_found.attribute_option";
    /// Attribute template not found (`details.id`).
    NotFoundAttributeTemplate => NotFound, "errors.not_found.attribute_template";
    /// Module not found (`details.id`).
    NotFoundModule => NotFound, "errors.not_found.module";
    /// Setting key not in any schema (`details.key`).
    NotFoundSetting => NotFound, "errors.not_found.setting";
    /// Job not found (`details.id`).
    NotFoundJob => NotFound, "errors.not_found.job";
    /// A sibling deck already has this name (`details.existing_deck_id`).
    ConflictDeckExists => Conflict, "errors.conflict.deck_exists";
    /// Moving a deck under itself/its descendant.
    ConflictDeckCycle => Conflict, "errors.conflict.deck_cycle";
    /// Duplicate note (`details.existing_note_id`).
    ConflictNoteDuplicate => Conflict, "errors.conflict.note_duplicate";
    /// Attribute key already used in this scope chain (`details.existing_id`).
    ConflictAttributeKeyExists => Conflict, "errors.conflict.attribute_key_exists";
    /// Option still in use and no replacement given (`details.usage_count`).
    ConflictAttributeOptionInUse => Conflict, "errors.conflict.attribute_option_in_use";
    /// Nothing to undo.
    ConflictNothingToUndo => Conflict, "errors.conflict.nothing_to_undo";
    /// Nothing to redo.
    ConflictNothingToRedo => Conflict, "errors.conflict.nothing_to_redo";
    /// The method's module is disabled (`details.module`).
    ConflictModuleDisabled => Conflict, "errors.conflict.module_disabled";
    /// The module cannot be disabled (`details.module`).
    ConflictModuleRequired => Conflict, "errors.conflict.module_required";
    /// Entity is in the trash; restore it first (`details.id`).
    ConflictDeleted => Conflict, "errors.conflict.deleted";
    /// Caller may not invoke this method (`details.permission`).
    PermissionDenied => Permission, "errors.permission.denied";
    /// Provider not configured/reachable (`details.provider`).
    ProviderUnavailable => Provider, "errors.provider.unavailable";
    /// Provider returned an error (`details.provider`).
    ProviderFailed => Provider, "errors.provider.failed";
    /// Database error.
    IoDatabase => Io, "errors.io.database";
    /// File system error.
    IoFile => Io, "errors.io.file";
    /// Unexpected internal error (a bug).
    Internal => Internal, "errors.internal.unexpected";
    /// The method exists in the contract but is not implemented yet.
    InternalNotImplemented => Internal, "errors.internal.not_implemented";
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Serialized form, e.g. `NOT_FOUND_DECK`.
        match serde_json::to_value(self) {
            Ok(Value::String(s)) => f.write_str(&s),
            _ => write!(f, "{self:?}"),
        }
    }
}

/// An error as returned over every transport.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, thiserror::Error)]
#[error("{code}: {message}")]
pub struct ApiError {
    /// Stable machine-readable code.
    pub code: ErrorCode,
    /// i18n key for the user-facing message (normally `code.message_key()`).
    pub message_key: String,
    /// Developer-facing English message. Never shown to users verbatim.
    pub message: String,
    /// Structured context; also used as i18n parameters.
    #[ts(type = "Record<string, unknown>")]
    pub details: Map<String, Value>,
    /// Whether retrying the same request may succeed.
    pub retryable: bool,
}

impl ApiError {
    /// Creates an error with the code's default message key and no details.
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message_key: code.message_key().to_owned(),
            message: message.into(),
            details: Map::new(),
            retryable: matches!(code.category(), ErrorCategory::Io | ErrorCategory::Provider),
        }
    }

    /// Adds a detail entry.
    pub fn with_detail(mut self, key: &str, value: impl Into<Value>) -> Self {
        self.details.insert(key.to_owned(), value.into());
        self
    }

    /// Overrides `retryable`.
    pub fn retryable(mut self, retryable: bool) -> Self {
        self.retryable = retryable;
        self
    }

    /// `INTERNAL_NOT_IMPLEMENTED` for `method`.
    pub fn not_implemented(method: &str) -> Self {
        Self::new(
            ErrorCode::InternalNotImplemented,
            format!("{method} is not implemented yet"),
        )
        .with_detail("method", method)
    }

    /// Category shortcut.
    pub fn category(&self) -> ErrorCategory {
        self.code.category()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_serialize_with_category_prefix() {
        for code in ErrorCode::ALL {
            let s = code.to_string();
            let prefix = match code.category() {
                ErrorCategory::Validation => "VALIDATION_",
                ErrorCategory::NotFound => "NOT_FOUND_",
                ErrorCategory::Conflict => "CONFLICT_",
                ErrorCategory::Permission => "PERMISSION_",
                ErrorCategory::Provider => "PROVIDER_",
                ErrorCategory::Io => "IO_",
                ErrorCategory::Internal => "INTERNAL",
            };
            assert!(s.starts_with(prefix), "{s} should start with {prefix}");
            assert!(code.message_key().starts_with("errors."), "{s}");
        }
    }

    #[test]
    fn message_keys_are_unique() {
        let mut keys: Vec<_> = ErrorCode::ALL.iter().map(|c| c.message_key()).collect();
        keys.sort_unstable();
        let n = keys.len();
        keys.dedup();
        assert_eq!(keys.len(), n);
    }

    #[test]
    fn wire_shape() {
        let e = ApiError::new(ErrorCode::ConflictNoteDuplicate, "dup")
            .with_detail("existing_note_id", "x");
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["code"], "CONFLICT_NOTE_DUPLICATE");
        assert_eq!(v["message_key"], "errors.conflict.note_duplicate");
        assert_eq!(v["details"]["existing_note_id"], "x");
        assert_eq!(v["retryable"], false);
        assert!(ApiError::new(ErrorCode::IoDatabase, "busy").retryable);
        let back: ApiError = serde_json::from_value(v).unwrap();
        assert_eq!(back, e);
    }
}
