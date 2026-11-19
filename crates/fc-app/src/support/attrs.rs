//! Attribute (classification) building blocks: lookups, applicability of definitions to a
//! deck / note / card, validated value writes, and option replacement inside values.

use super::decks::DeckIndex;
use super::{deleted, err_id};
use crate::error::AppResult;
use fc_api::v1::attributes::AttributeAssignment;
use fc_api::ErrorCode;
use fc_core::attribute::{
    effective_attribute_defs, validate_attribute_value, AttributeDef, AttributeValue, EntityRef,
};
use fc_core::error::CoreError;
use fc_core::{AttributeDefId, AttributeOptionId, DeckId};
use fc_storage::repo::CardFilter;
use fc_storage::{Reads, WriteTx};
use std::collections::HashMap;

/// Definition by id (trash included) or `NOT_FOUND_ATTRIBUTE`.
pub(crate) fn require_def(reads: &dyn Reads, id: AttributeDefId) -> AppResult<AttributeDef> {
    reads.get_attribute_def(id)?.ok_or_else(|| {
        err_id(
            ErrorCode::NotFoundAttribute,
            format!("attribute {id} not found"),
            id,
        )
    })
}

/// Live definition by id (`NOT_FOUND_ATTRIBUTE` / `CONFLICT_DELETED`).
pub(crate) fn require_live_def(reads: &dyn Reads, id: AttributeDefId) -> AppResult<AttributeDef> {
    let def = require_def(reads, id)?;
    if def.deleted_at.is_some() {
        return Err(deleted("attribute", id));
    }
    Ok(def)
}

/// Definitions applying to `deck` (inheritance resolved, deepest key wins), each with
/// whether it is inherited. `None` = collection-scoped definitions only.
pub(crate) fn effective_for_deck(
    reads: &dyn Reads,
    index: &DeckIndex,
    deck: Option<DeckId>,
) -> AppResult<Vec<(AttributeDef, bool)>> {
    let ancestry = match deck {
        Some(d) => index.ancestry(d)?,
        None => Vec::new(),
    };
    let defs = reads.list_attribute_defs(false)?;
    Ok(effective_attribute_defs(&ancestry, &defs)
        .into_iter()
        .map(|e| (e.def.clone(), e.is_inherited(ancestry.len())))
        .collect())
}

/// Decks an entity lives in: a card's deck, or the decks of a note's cards (trashed cards
/// included, so values of trashed notes stay addressable). Empty if the entity has no cards.
pub(crate) fn entity_decks(reads: &dyn Reads, entity: EntityRef) -> AppResult<Vec<DeckId>> {
    match entity {
        EntityRef::Card(id) => {
            let card = reads.get_card(id)?.ok_or_else(|| {
                err_id(ErrorCode::NotFoundCard, format!("card {id} not found"), id)
            })?;
            Ok(vec![card.deck_id])
        }
        EntityRef::Note(id) => {
            if reads.get_note(id)?.is_none() {
                return Err(err_id(
                    ErrorCode::NotFoundNote,
                    format!("note {id} not found"),
                    id,
                ));
            }
            let mut cards = reads.list_cards(&CardFilter {
                deck_ids: None,
                note_ids: Some(vec![id]),
                include_deleted: true,
            })?;
            // Live cards first so their decks win the "first deck" (suggestions etc.).
            cards.sort_by_key(|c| (c.deleted_at.is_some(), c.created_at, c.id));
            let mut decks = Vec::new();
            for c in cards {
                if !decks.contains(&c.deck_id) {
                    decks.push(c.deck_id);
                }
            }
            Ok(decks)
        }
    }
}

/// Live definitions applicable to `entity` (union over its decks; collection scope if it has
/// none), filtered to the entity's target.
pub(crate) fn applicable(reads: &dyn Reads, entity: EntityRef) -> AppResult<Vec<AttributeDef>> {
    let index = DeckIndex::load(reads)?;
    let decks = entity_decks(reads, entity)?;
    let scopes: Vec<Option<DeckId>> = if decks.is_empty() {
        vec![None]
    } else {
        decks.into_iter().map(Some).collect()
    };
    let mut out: Vec<AttributeDef> = Vec::new();
    for scope in scopes {
        for (def, _) in effective_for_deck(reads, &index, scope)? {
            if def.target == entity.target() && !out.iter().any(|d| d.id == def.id) {
                out.push(def);
            }
        }
    }
    Ok(out)
}

/// Validates and writes `assignments` on `entity`; returns the previous values of the touched
/// attributes (the inverse assignments).
///
/// `strict` (normal commands): the definition must apply to the entity and a required value
/// cannot be cleared. Undo/redo batches pass `false`: they restore an earlier state verbatim,
/// even one that later rules would reject. Values are always validated against the definition.
pub(crate) fn apply_assignments(
    tx: &mut dyn WriteTx,
    strict: bool,
    entity: EntityRef,
    assignments: &[AttributeAssignment],
) -> AppResult<Vec<AttributeAssignment>> {
    let reads: &dyn Reads = &*tx;
    let applicable = applicable(reads, entity)?;
    let current: HashMap<AttributeDefId, AttributeValue> =
        reads.get_attribute_values(entity)?.into_iter().collect();
    let mut writes: Vec<(AttributeDefId, Option<AttributeValue>)> = Vec::new();
    let mut previous: Vec<AttributeAssignment> = Vec::new();
    for a in assignments {
        let not_applicable = || CoreError::AttributeNotApplicable {
            id: a.attr_id.to_string(),
        };
        let def = match applicable.iter().find(|d| d.id == a.attr_id) {
            Some(d) => d.clone(),
            None if !strict => require_def(reads, a.attr_id)?,
            None => return Err(not_applicable().into()),
        };
        if def.target != entity.target() {
            return Err(not_applicable().into());
        }
        let value = match &a.value {
            Some(v) => Some(validate_attribute_value(&def, v)?),
            None if strict && def.required => {
                return Err(CoreError::AttributeRequired { key: def.key }.into())
            }
            None => None,
        };
        writes.retain(|(id, _)| *id != def.id);
        writes.push((def.id, value));
        if !previous.iter().any(|p| p.attr_id == def.id) {
            previous.push(AttributeAssignment {
                attr_id: def.id,
                value: current.get(&def.id).cloned(),
            });
        }
    }
    tx.set_attribute_values(entity, &writes)?;
    Ok(previous)
}

/// `value` with `option` replaced by `replacement` (or removed). `None` = no value left.
pub(crate) fn replace_option(
    value: &AttributeValue,
    option: AttributeOptionId,
    replacement: Option<AttributeOptionId>,
) -> Option<AttributeValue> {
    match value {
        AttributeValue::Choice(o) if *o == option => replacement.map(AttributeValue::Choice),
        AttributeValue::Choices(list) => {
            let mut out: Vec<AttributeOptionId> = Vec::new();
            for o in list {
                let mapped = if *o == option { replacement } else { Some(*o) };
                if let Some(m) = mapped {
                    if !out.contains(&m) {
                        out.push(m);
                    }
                }
            }
            (!out.is_empty()).then_some(AttributeValue::Choices(out))
        }
        other => Some(other.clone()),
    }
}

/// Whether `value` references `option`.
pub(crate) fn uses_option(value: &AttributeValue, option: AttributeOptionId) -> bool {
    match value {
        AttributeValue::Choice(o) => *o == option,
        AttributeValue::Choices(list) => list.contains(&option),
        _ => false,
    }
}
