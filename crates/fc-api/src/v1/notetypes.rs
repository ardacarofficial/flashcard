//! Note type DTOs (read-only in v0.1; the editor arrives in v0.3).

use fc_core::notetype::{CardTemplate, FieldDef, FieldKey, NoteTypeKind};
use fc_core::{NoteTypeId, Timestamp};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A note type.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct NoteTypeDto {
    /// Id.
    pub id: NoteTypeId,
    /// Display name.
    pub name: String,
    /// Standard or cloze.
    pub kind: NoteTypeKind,
    /// Field schema in display order.
    pub fields: Vec<FieldDef>,
    /// Card templates.
    pub templates: Vec<CardTemplate>,
    /// Shared CSS.
    pub css: String,
    /// Field used for sorting/lists.
    pub sort_field: FieldKey,
    /// Schema version.
    pub version: u32,
    /// Built-in marker (`classic`, …) or `null`.
    pub builtin: Option<String>,
    /// Last modification time.
    pub updated_at: Timestamp,
}

/// Response of `notetypes.list`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct NoteTypeList {
    /// Note types sorted by name.
    pub notetypes: Vec<NoteTypeDto>,
}

/// `notetypes.get`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct NoteTypeRef {
    /// Note type id.
    pub id: NoteTypeId,
}
