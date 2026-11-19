//! Inheritance of attribute definitions down the deck tree, and preparation of the value set
//! for a note being created/edited.

use super::{validate_attribute_value, AttributeDef, AttributeTarget, AttributeValue};
use crate::error::{CoreError, CoreResult};
use crate::id::{AttributeDefId, DeckId};

/// A definition that applies to a deck, with where it came from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EffectiveAttribute<'a> {
    /// The definition.
    pub def: &'a AttributeDef,
    /// `0` = collection scope, `1` = defined on the root ancestor, …, `ancestry.len()` =
    /// defined on the deck itself.
    pub depth: usize,
}

impl EffectiveAttribute<'_> {
    /// Whether the definition comes from a deck above the target deck (or the collection).
    pub fn is_inherited(&self, ancestry_len: usize) -> bool {
        self.depth < ancestry_len
    }
}

/// Definitions that apply to the deck whose ancestry is `ancestry` (root first, the deck
/// itself last; empty = collection only).
///
/// Rules:
/// * collection-scoped definitions apply everywhere; deck-scoped ones apply to that deck and
///   all descendants;
/// * soft-deleted definitions are ignored;
/// * if the same `key` is defined at several levels, the **deepest** wins (a sub-deck may
///   specialize an inherited classification);
/// * output order: by depth (collection first), then `position`, then `key`.
pub fn effective_attribute_defs<'a>(
    ancestry: &[DeckId],
    defs: &'a [AttributeDef],
) -> Vec<EffectiveAttribute<'a>> {
    let depth_of = |scope: Option<DeckId>| -> Option<usize> {
        match scope {
            None => Some(0),
            Some(d) => ancestry.iter().position(|a| *a == d).map(|i| i + 1),
        }
    };
    let mut applicable: Vec<EffectiveAttribute<'a>> = defs
        .iter()
        .filter(|d| d.deleted_at.is_none())
        .filter_map(|def| {
            depth_of(def.scope_deck_id).map(|depth| EffectiveAttribute { def, depth })
        })
        .collect();
    // Deepest-wins shadowing by key.
    let shadowed: Vec<bool> = applicable
        .iter()
        .map(|a| {
            applicable
                .iter()
                .any(|b| b.def.key == a.def.key && b.depth > a.depth)
        })
        .collect();
    let mut i = 0;
    applicable.retain(|_| {
        let keep = !shadowed[i];
        i += 1;
        keep
    });
    applicable.sort_by(|a, b| {
        a.depth
            .cmp(&b.depth)
            .then(a.def.position.cmp(&b.def.position))
            .then_with(|| a.def.key.cmp(&b.def.key))
    });
    applicable
}

/// Computes the final values for an entity of kind `target`.
///
/// * `provided`: values from the form; `(id, None)` explicitly clears a value (suppresses the
///   default).
/// * Every provided id must be among `effective` definitions for `target`
///   ([`CoreError::AttributeNotApplicable`] otherwise); every value is validated/normalized.
/// * Definitions without a provided entry receive their `default_value`, if any.
/// * Required definitions without a value fail with [`CoreError::AttributeRequired`].
///
/// Output is in `effective` order.
pub fn prepare_attribute_values(
    effective: &[EffectiveAttribute<'_>],
    target: AttributeTarget,
    provided: &[(AttributeDefId, Option<AttributeValue>)],
) -> CoreResult<Vec<(AttributeDefId, AttributeValue)>> {
    let relevant: Vec<&AttributeDef> = effective
        .iter()
        .map(|e| e.def)
        .filter(|d| d.target == target)
        .collect();
    if let Some((id, _)) = provided
        .iter()
        .find(|(id, _)| !relevant.iter().any(|d| d.id == *id))
    {
        return Err(CoreError::AttributeNotApplicable { id: id.to_string() });
    }
    let mut out = Vec::new();
    for def in relevant {
        let value = match provided.iter().rev().find(|(id, _)| *id == def.id) {
            Some((_, v)) => v.clone(),
            None => def.default_value.clone(),
        };
        match value {
            Some(v) => out.push((def.id, validate_attribute_value(def, &v)?)),
            None if def.required => {
                return Err(CoreError::AttributeRequired {
                    key: def.key.clone(),
                })
            }
            None => {}
        }
    }
    Ok(out)
}
