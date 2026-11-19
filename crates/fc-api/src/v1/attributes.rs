//! Classification (attribute) DTOs — docs/02 §8.1.

use super::common::EntityRef;
use fc_core::attribute::{AttributeKind, AttributeOption, AttributeTarget, AttributeValue};
use fc_core::i18n::Locale;
use fc_core::{AttributeDefId, AttributeOptionId, DeckId, Timestamp};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// An attribute definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct AttributeDefDto {
    /// Id.
    pub id: AttributeDefId,
    /// Deck it is defined on (`null` = whole collection).
    pub scope_deck_id: Option<DeckId>,
    /// Query key (`level`).
    pub key: String,
    /// Display name.
    pub name: String,
    /// Value type.
    pub kind: AttributeKind,
    /// Options sorted by order (enum kinds).
    pub options: Vec<AttributeOption>,
    /// Note or card.
    pub target: AttributeTarget,
    /// Required on notes.
    pub required: bool,
    /// Default value.
    pub default_value: Option<AttributeValue>,
    /// Display order within its scope.
    pub position: i32,
    /// Template it was copied from.
    pub template_id: Option<String>,
    /// Last modification time.
    pub updated_at: Timestamp,
}

/// A definition as it applies to a particular deck (`attributes.effective`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct EffectiveAttributeDto {
    /// The definition.
    pub def: AttributeDefDto,
    /// `true` if defined on an ancestor deck or the collection.
    pub inherited: bool,
    /// Value the note form should preselect: the default, or the last used value if the user
    /// enabled "remember last" (resolved server-side).
    pub suggested_value: Option<AttributeValue>,
}

/// Response of `attributes.list`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct AttributeDefList {
    /// Definitions.
    pub defs: Vec<AttributeDefDto>,
}

/// Response of `attributes.effective` (in form order).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct EffectiveAttributeList {
    /// Applicable definitions.
    pub attributes: Vec<EffectiveAttributeDto>,
}

/// A value assigned to an attribute. `value: null` clears (suppresses the default).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct AttributeAssignment {
    /// Attribute definition.
    pub attr_id: AttributeDefId,
    /// Value.
    pub value: Option<AttributeValue>,
}

/// New option when defining an attribute.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct NewAttributeOption {
    /// Label.
    pub label: String,
    /// Color.
    pub color: Option<String>,
}

/// `attributes.list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct ListAttributes {
    /// Only definitions on this deck (`null` = all definitions).
    pub scope_deck_id: Option<DeckId>,
    /// Include soft-deleted definitions (default `false`).
    pub include_deleted: Option<bool>,
}

/// `attributes.effective`: definitions applicable to a deck (inheritance resolved).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct EffectiveAttributes {
    /// Deck.
    pub deck_id: DeckId,
    /// Note or card.
    pub target: AttributeTarget,
}

/// `attributes.define`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct DefineAttribute {
    /// Deck scope (`null` = collection).
    pub scope_deck_id: Option<DeckId>,
    /// Key (`null` = derived from `name`).
    pub key: Option<String>,
    /// Display name.
    pub name: String,
    /// Value type.
    pub kind: AttributeKind,
    /// Target (default note).
    pub target: Option<AttributeTarget>,
    /// Required (default `false`).
    pub required: Option<bool>,
    /// Options for enum kinds, in order.
    #[serde(default)]
    pub options: Vec<NewAttributeOption>,
}

/// `attributes.define_from_template`: copies a built-in template.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct DefineAttributeFromTemplate {
    /// Template id (`cefr`, `yks`, `jlpt`, `hsk`, `school_grade`, `difficulty`).
    pub template_id: String,
    /// Deck scope (`null` = collection).
    pub scope_deck_id: Option<DeckId>,
    /// Language for names/labels (default: app locale).
    pub locale: Option<Locale>,
}

/// `attributes.update`: mutable properties of a definition (key and kind are immutable).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct UpdateAttribute {
    /// Definition id.
    pub id: AttributeDefId,
    /// Display name.
    pub name: String,
    /// Required.
    pub required: bool,
    /// Default value (`null` = none).
    pub default_value: Option<AttributeValue>,
    /// Position within scope.
    pub position: i32,
}

/// `attributes.get`, `attributes.delete`, `attributes.restore`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct AttributeRef {
    /// Definition id.
    pub id: AttributeDefId,
}

/// `attributes.option.add`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct AddAttributeOption {
    /// Definition.
    pub attr_id: AttributeDefId,
    /// Label.
    pub label: String,
    /// Color.
    pub color: Option<String>,
    /// Insert after this option (`null` = last).
    pub after: Option<AttributeOptionId>,
    /// Id for the new option (`null` = generated). Lets the inverse of `option.delete` /
    /// `option.merge` bring an option back under its old id, so stored values match again.
    #[serde(default)]
    pub id: Option<AttributeOptionId>,
}

/// `attributes.option.rename`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct RenameAttributeOption {
    /// Definition.
    pub attr_id: AttributeDefId,
    /// Option.
    pub option_id: AttributeOptionId,
    /// New label.
    pub label: String,
}

/// `attributes.option.set_color`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SetAttributeOptionColor {
    /// Definition.
    pub attr_id: AttributeDefId,
    /// Option.
    pub option_id: AttributeOptionId,
    /// Color (`null` clears).
    pub color: Option<String>,
}

/// `attributes.option.reorder`: `order` must list every option id exactly once.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ReorderAttributeOptions {
    /// Definition.
    pub attr_id: AttributeDefId,
    /// New order.
    pub order: Vec<AttributeOptionId>,
}

/// `attributes.option.merge`: moves all values from `from` to `into`, then removes `from`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct MergeAttributeOptions {
    /// Definition.
    pub attr_id: AttributeDefId,
    /// Option to remove.
    pub from: AttributeOptionId,
    /// Option to keep.
    pub into: AttributeOptionId,
}

/// `attributes.option.delete`: values using the option move to `replace_with`, or are removed
/// if it is `null`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct DeleteAttributeOption {
    /// Definition.
    pub attr_id: AttributeDefId,
    /// Option.
    pub option_id: AttributeOptionId,
    /// Replacement option.
    pub replace_with: Option<AttributeOptionId>,
}

/// Internal `attributes.option.restore` (the inverse of `option.delete` / `option.merge`):
/// re-inserts `option` under its old id at `index` of the sorted option list, then puts each
/// entity's value back exactly as it was. Its own inverse removes the option again, moving
/// values to `replaced_by` (what the original delete/merge did).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct RestoreAttributeOption {
    /// Definition.
    pub attr_id: AttributeDefId,
    /// The option as it was (id, label, color; `order` is ignored).
    pub option: AttributeOption,
    /// Index in the sorted option list (clamped to the end).
    pub index: u32,
    /// Option the values had been moved to (`null` = they had been removed).
    pub replaced_by: Option<AttributeOptionId>,
    /// Entity values before the delete/merge (full values, written verbatim).
    pub values: Vec<EntityAttributeValue>,
}

/// One entity's value of one attribute.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct EntityAttributeValue {
    /// Note or card.
    pub entity: EntityRef,
    /// Value.
    pub value: AttributeValue,
}

/// `attributes.values.get`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct GetAttributeValues {
    /// Note or card.
    pub entity: EntityRef,
}

/// `attributes.values.set`: sets (or clears with `null`) values on one entity; other
/// attributes are untouched.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SetAttributeValues {
    /// Note or card.
    pub entity: EntityRef,
    /// Values.
    pub values: Vec<AttributeAssignment>,
}

/// Values on an entity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct AttributeValues {
    /// Entity.
    pub entity: EntityRef,
    /// Values (only attributes that have one).
    pub values: Vec<AttributeAssignment>,
}

/// Result of option merge/delete.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct OptionChangeResult {
    /// Values that were moved or removed.
    pub values_affected: u32,
}

/// A built-in template (`attributes.templates`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct AttributeTemplateDto {
    /// Template id.
    pub id: String,
    /// Suggested key.
    pub key: String,
    /// Name in the requested locale.
    pub name: String,
    /// Value type.
    pub kind: AttributeKind,
    /// Option labels in order, in the requested locale.
    pub options: Vec<String>,
}

/// `attributes.templates`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct ListAttributeTemplates {
    /// Locale (default: app locale).
    pub locale: Option<Locale>,
}

/// Response of `attributes.templates`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct AttributeTemplateList {
    /// Templates.
    pub templates: Vec<AttributeTemplateDto>,
}
