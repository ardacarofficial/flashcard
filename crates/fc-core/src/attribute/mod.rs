//! User-defined classifications (docs/02 §8.1): "Level = A1…C2", "Exam = TYT/AYT/YDT", …
//!
//! An [`AttributeDef`] is defined at collection scope (`scope_deck_id = None`) or on a deck,
//! and is inherited by all descendant decks ([`resolve`]). Values ([`AttributeValue`]) attach
//! to notes (or cards). Options have stable ids, so renaming or reordering never touches
//! values.

pub mod resolve;
pub mod templates;
pub mod validate;

pub use resolve::{effective_attribute_defs, prepare_attribute_values, EffectiveAttribute};
pub use templates::{builtin_templates, find_template, instantiate_template, AttributeTemplate};
pub use validate::{
    slugify_key, validate_attribute_def, validate_attribute_key, validate_attribute_value,
};

use crate::error::{AttributeDefProblem, CoreError, CoreResult};
use crate::id::{AttributeDefId, AttributeOptionId, CardId, DeckId, NoteId};
use crate::time::Timestamp;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

/// Maximum length of an option label / attribute name.
pub const MAX_LABEL_LEN: usize = 100;
/// Maximum length of a text value.
pub const MAX_TEXT_VALUE_LEN: usize = 1000;

/// Value type of an attribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum AttributeKind {
    /// Single choice from ordered options (`level >= B1` is meaningful).
    OrderedEnum,
    /// Single choice from unordered options.
    Enum,
    /// Several choices.
    MultiEnum,
    /// Number.
    Number,
    /// Free text.
    Text,
    /// Calendar date (`YYYY-MM-DD`).
    Date,
    /// Yes / no.
    Bool,
}

impl AttributeKind {
    /// Whether the kind uses an option list.
    pub const fn has_options(self) -> bool {
        matches!(self, Self::OrderedEnum | Self::Enum | Self::MultiEnum)
    }

    /// Storage string (`attribute_defs.kind`).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OrderedEnum => "ordered_enum",
            Self::Enum => "enum",
            Self::MultiEnum => "multi_enum",
            Self::Number => "number",
            Self::Text => "text",
            Self::Date => "date",
            Self::Bool => "bool",
        }
    }

    /// Parses the storage string.
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "ordered_enum" => Self::OrderedEnum,
            "enum" => Self::Enum,
            "multi_enum" => Self::MultiEnum,
            "number" => Self::Number,
            "text" => Self::Text,
            "date" => Self::Date,
            "bool" => Self::Bool,
            _ => return None,
        })
    }
}

/// What an attribute attaches to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum AttributeTarget {
    /// Notes (default).
    Note,
    /// Individual cards.
    Card,
}

impl AttributeTarget {
    /// Storage string (`attribute_defs.target`, `attribute_values.entity_kind`).
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Note => "note",
            Self::Card => "card",
        }
    }
}

/// The entity an attribute value is attached to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum EntityRef {
    /// A note.
    Note(NoteId),
    /// A card.
    Card(CardId),
}

impl EntityRef {
    /// Target kind (`attribute_values.entity_kind`).
    pub const fn target(&self) -> AttributeTarget {
        match self {
            Self::Note(_) => AttributeTarget::Note,
            Self::Card(_) => AttributeTarget::Card,
        }
    }

    /// Entity id as UUID (`attribute_values.entity_id`).
    pub const fn uuid(&self) -> &uuid::Uuid {
        match self {
            Self::Note(id) => &id.0,
            Self::Card(id) => &id.0,
        }
    }
}

/// One choice of an enum-like attribute. Stored inside `attribute_defs.options` (JSON).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct AttributeOption {
    /// Stable id: values reference this, never the label.
    pub id: AttributeOptionId,
    /// Display label (user text).
    pub label: String,
    /// Optional color token or `#rrggbb`.
    pub color: Option<String>,
    /// Sort order (ascending). For ordered enums this *is* the ordering (`A1 < A2`).
    pub order: i32,
}

/// An attribute value. JSON shape: `{"type": "choice", "value": "<option id>"}` etc.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum AttributeValue {
    /// For `ordered_enum` / `enum`.
    Choice(AttributeOptionId),
    /// For `multi_enum` (non-empty, no duplicates; normalized to option order).
    Choices(Vec<AttributeOptionId>),
    /// For `number` (finite).
    Number(f64),
    /// For `text` (trimmed, non-empty).
    Text(String),
    /// For `date`: `YYYY-MM-DD`.
    Date(String),
    /// For `bool`.
    Bool(bool),
}

/// A classification definition (`attribute_defs` row).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AttributeDef {
    /// Id.
    pub id: AttributeDefId,
    /// Deck the definition is attached to; `None` = whole collection.
    pub scope_deck_id: Option<DeckId>,
    /// Machine key used in queries (`level:B1`), `[a-z][a-z0-9_]*`.
    pub key: String,
    /// Display name.
    pub name: String,
    /// Value type.
    pub kind: AttributeKind,
    /// Options (enum kinds only).
    pub options: Vec<AttributeOption>,
    /// Note or card.
    pub target: AttributeTarget,
    /// A value must be present when creating/updating a note.
    pub required: bool,
    /// Pre-selected value for new notes.
    pub default_value: Option<AttributeValue>,
    /// Display order among definitions of the same scope.
    pub position: i32,
    /// Built-in template this was copied from (informational only).
    pub template_id: Option<String>,
    /// Creation time.
    pub created_at: Timestamp,
    /// Last modification time.
    pub updated_at: Timestamp,
    /// Soft-delete time.
    pub deleted_at: Option<Timestamp>,
}

impl AttributeDef {
    /// Option by id.
    pub fn option(&self, id: AttributeOptionId) -> Option<&AttributeOption> {
        self.options.iter().find(|o| o.id == id)
    }

    /// Option by label (case-insensitive), e.g. to resolve `level:b1` in queries/imports.
    pub fn option_by_label(&self, label: &str) -> Option<&AttributeOption> {
        let wanted = label.trim().to_lowercase();
        self.options
            .iter()
            .find(|o| o.label.to_lowercase() == wanted)
    }

    /// Options sorted by `order` (then label for ties).
    pub fn sorted_options(&self) -> Vec<&AttributeOption> {
        let mut v: Vec<_> = self.options.iter().collect();
        v.sort_by(|a, b| a.order.cmp(&b.order).then_with(|| a.label.cmp(&b.label)));
        v
    }

    /// Compares two options of an ordered enum. `None` if the kind is not ordered or an id is
    /// unknown.
    pub fn compare_options(&self, a: AttributeOptionId, b: AttributeOptionId) -> Option<Ordering> {
        if self.kind != AttributeKind::OrderedEnum {
            return None;
        }
        Some(self.option(a)?.order.cmp(&self.option(b)?.order))
    }

    /// Adds an option. With `after = None` it goes last; otherwise right after `after`.
    /// Orders are renumbered `0..n`.
    pub fn add_option(
        &mut self,
        option: AttributeOption,
        after: Option<AttributeOptionId>,
    ) -> CoreResult<()> {
        if !self.kind.has_options() {
            return Err(def_err(AttributeDefProblem::OptionsNotAllowed));
        }
        let label = validate::normalize_label(&option.label)
            .ok_or(def_err(AttributeDefProblem::InvalidOptionLabel))?;
        self.ensure_label_free(&label, None)?;
        if self.option(option.id).is_some() {
            return Err(def_err(AttributeDefProblem::DuplicateOptionId));
        }
        let mut ordered: Vec<AttributeOption> =
            self.sorted_options().into_iter().cloned().collect();
        let idx = match after {
            None => ordered.len(),
            Some(a) => {
                ordered
                    .iter()
                    .position(|o| o.id == a)
                    .ok_or_else(|| self.unknown_option(a))?
                    + 1
            }
        };
        ordered.insert(idx, AttributeOption { label, ..option });
        self.options = renumber(ordered);
        Ok(())
    }

    /// Inserts `option` (keeping its id) at `index` of the sorted option list (clamped to the
    /// end). Orders are renumbered `0..n`. Used to bring a removed option back exactly.
    pub fn insert_option_at(&mut self, option: AttributeOption, index: usize) -> CoreResult<()> {
        let ordered: Vec<AttributeOptionId> = self.sorted_options().iter().map(|o| o.id).collect();
        let after = match index.min(ordered.len()) {
            0 => None,
            i => Some(ordered[i - 1]),
        };
        let first = ordered.first().copied();
        let id = option.id;
        self.add_option(option, after)?;
        if after.is_none() && first.is_some() {
            // `add_option(None)` appends; move it to the front.
            let mut order: Vec<AttributeOptionId> = vec![id];
            order.extend(ordered);
            self.reorder_options(&order)?;
        }
        Ok(())
    }

    /// Renames an option (id unchanged). Returns the previous label (for the inverse).
    pub fn rename_option(&mut self, id: AttributeOptionId, label: &str) -> CoreResult<String> {
        let label = validate::normalize_label(label)
            .ok_or(def_err(AttributeDefProblem::InvalidOptionLabel))?;
        self.ensure_label_free(&label, Some(id))?;
        let unknown = self.unknown_option(id);
        let opt = self
            .options
            .iter_mut()
            .find(|o| o.id == id)
            .ok_or(unknown)?;
        Ok(std::mem::replace(&mut opt.label, label))
    }

    /// Sets an option's color. Returns the previous color.
    pub fn set_option_color(
        &mut self,
        id: AttributeOptionId,
        color: Option<String>,
    ) -> CoreResult<Option<String>> {
        let unknown = self.unknown_option(id);
        let opt = self
            .options
            .iter_mut()
            .find(|o| o.id == id)
            .ok_or(unknown)?;
        Ok(std::mem::replace(&mut opt.color, color))
    }

    /// Reorders options. `order` must be a permutation of all option ids. Returns the previous
    /// order (for the inverse).
    pub fn reorder_options(
        &mut self,
        order: &[AttributeOptionId],
    ) -> CoreResult<Vec<AttributeOptionId>> {
        let previous: Vec<_> = self.sorted_options().iter().map(|o| o.id).collect();
        let mut a = previous.clone();
        let mut b = order.to_vec();
        a.sort();
        b.sort();
        b.dedup();
        if a != b || order.len() != previous.len() {
            return Err(def_err(AttributeDefProblem::InvalidOrder));
        }
        for opt in &mut self.options {
            opt.order = order
                .iter()
                .position(|id| *id == opt.id)
                .map_or(i32::MAX, |p| i32::try_from(p).unwrap_or(i32::MAX));
        }
        Ok(previous)
    }

    /// Removes an option and returns it with its index in the sorted list (for the inverse).
    /// Callers must first move or delete values referencing it.
    pub fn remove_option(&mut self, id: AttributeOptionId) -> CoreResult<(AttributeOption, usize)> {
        let ordered: Vec<AttributeOption> = self.sorted_options().into_iter().cloned().collect();
        let idx = ordered
            .iter()
            .position(|o| o.id == id)
            .ok_or_else(|| self.unknown_option(id))?;
        let mut ordered = ordered;
        let removed = ordered.remove(idx);
        self.options = renumber(ordered);
        Ok((removed, idx))
    }

    fn ensure_label_free(&self, label: &str, except: Option<AttributeOptionId>) -> CoreResult<()> {
        let key = label.to_lowercase();
        if self
            .options
            .iter()
            .any(|o| Some(o.id) != except && o.label.to_lowercase() == key)
        {
            return Err(def_err(AttributeDefProblem::DuplicateOptionLabel(
                label.to_owned(),
            )));
        }
        Ok(())
    }

    fn unknown_option(&self, id: AttributeOptionId) -> CoreError {
        CoreError::UnknownAttributeOption {
            key: self.key.clone(),
            option: id.to_string(),
        }
    }
}

fn renumber(ordered: Vec<AttributeOption>) -> Vec<AttributeOption> {
    ordered
        .into_iter()
        .enumerate()
        .map(|(i, o)| AttributeOption {
            order: i32::try_from(i).unwrap_or(i32::MAX),
            ..o
        })
        .collect()
}

fn def_err(reason: AttributeDefProblem) -> CoreError {
    CoreError::InvalidAttributeDef { reason }
}

/// One `attribute_values` row's value columns. A value maps to one row, except
/// [`AttributeValue::Choices`], which maps to one row per option.
#[derive(Debug, Clone, PartialEq)]
pub struct StoredValue {
    /// `attribute_values.value` (TEXT): option id, canonical number, text, date or
    /// `"true"`/`"false"`.
    pub value: String,
    /// `attribute_values.value_num` (REAL): set for numbers (the number) and bools (1/0) so SQL
    /// can compare numerically.
    pub value_num: Option<f64>,
}

impl AttributeValue {
    /// Storage rows for this value (the storage contract shared by `fc-storage` and `fc-app`).
    pub fn to_stored(&self) -> Vec<StoredValue> {
        let row = |value: String, value_num| StoredValue { value, value_num };
        match self {
            Self::Choice(id) => vec![row(id.to_string(), None)],
            Self::Choices(ids) => ids.iter().map(|id| row(id.to_string(), None)).collect(),
            Self::Number(n) => vec![row(n.to_string(), Some(*n))],
            Self::Text(s) | Self::Date(s) => vec![row(s.clone(), None)],
            Self::Bool(b) => vec![row(b.to_string(), Some(if *b { 1.0 } else { 0.0 }))],
        }
    }

    /// Rebuilds a value of `def` from its storage rows. Returns `None` for no rows or malformed
    /// rows. `multi_enum` choices come back in the definition's current option order (rows
    /// carry no order of their own, and options may have been reordered since the write);
    /// ids no longer among the options keep their row order, after the known ones.
    pub fn from_stored(def: &AttributeDef, rows: &[StoredValue]) -> Option<Self> {
        let first = rows.first()?;
        Some(match def.kind {
            AttributeKind::OrderedEnum | AttributeKind::Enum => {
                Self::Choice(first.value.parse().ok()?)
            }
            AttributeKind::MultiEnum => {
                let mut ids = rows
                    .iter()
                    .map(|r| r.value.parse().ok())
                    .collect::<Option<Vec<AttributeOptionId>>>()?;
                let order: Vec<AttributeOptionId> =
                    def.sorted_options().iter().map(|o| o.id).collect();
                // Stable sort: unknown ids (usize::MAX) keep their relative order at the end.
                ids.sort_by_key(|id| order.iter().position(|o| o == id).unwrap_or(usize::MAX));
                Self::Choices(ids)
            }
            AttributeKind::Number => Self::Number(first.value.parse().ok()?),
            AttributeKind::Text => Self::Text(first.value.clone()),
            AttributeKind::Date => Self::Date(first.value.clone()),
            AttributeKind::Bool => Self::Bool(first.value.parse().ok()?),
        })
    }
}

#[cfg(test)]
mod tests;
