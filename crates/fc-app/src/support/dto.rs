//! Model → DTO conversion.

use crate::error::AppResult;
use fc_api::v1::attributes::{AttributeAssignment, AttributeDefDto};
use fc_api::v1::notes::{NoteCardDto, NoteDto};
use fc_api::v1::notetypes::NoteTypeDto;
use fc_core::attribute::{AttributeDef, AttributeValue, EntityRef};
use fc_core::card::Card;
use fc_core::note::Note;
use fc_core::notetype::NoteType;
use fc_core::AttributeDefId;
use fc_storage::repo::CardFilter;
use fc_storage::Reads;

/// Attribute definition DTO (options sorted by order).
pub(crate) fn attr_def(d: &AttributeDef) -> AttributeDefDto {
    AttributeDefDto {
        id: d.id,
        scope_deck_id: d.scope_deck_id,
        key: d.key.clone(),
        name: d.name.clone(),
        kind: d.kind,
        options: d.sorted_options().into_iter().cloned().collect(),
        target: d.target,
        required: d.required,
        default_value: d.default_value.clone(),
        position: d.position,
        template_id: d.template_id.clone(),
        updated_at: d.updated_at,
    }
}

/// Note type DTO.
pub(crate) fn notetype(nt: &NoteType) -> NoteTypeDto {
    NoteTypeDto {
        id: nt.id,
        name: nt.name.clone(),
        kind: nt.kind,
        fields: nt.fields.clone(),
        templates: nt.templates.clone(),
        css: nt.css.clone(),
        sort_field: nt.sort_field.clone(),
        version: nt.version,
        builtin: nt.builtin.clone(),
        updated_at: nt.updated_at,
    }
}

/// Values as assignments.
pub(crate) fn assignments(
    values: Vec<(AttributeDefId, AttributeValue)>,
) -> Vec<AttributeAssignment> {
    values
        .into_iter()
        .map(|(attr_id, v)| AttributeAssignment {
            attr_id,
            value: Some(v),
        })
        .collect()
}

/// Cards of a note: live ones, or (for a trashed note) those trashed with it.
pub(crate) fn note_cards(reads: &dyn Reads, note: &Note) -> AppResult<Vec<Card>> {
    let mut cards = reads.list_cards(&CardFilter {
        deck_ids: None,
        note_ids: Some(vec![note.id]),
        include_deleted: note.deleted_at.is_some(),
    })?;
    if note.deleted_at.is_some() {
        cards.retain(|c| c.deleted_at == note.deleted_at);
    }
    cards.sort_by(|a, b| {
        a.template_key
            .cmp(&b.template_key)
            .then(a.created_at.cmp(&b.created_at))
            .then(a.id.cmp(&b.id))
    });
    Ok(cards)
}

/// Full note DTO (cards and attribute values included).
pub(crate) fn note(reads: &dyn Reads, note: &Note) -> AppResult<NoteDto> {
    let cards = note_cards(reads, note)?
        .into_iter()
        .map(|c| NoteCardDto {
            id: c.id,
            deck_id: c.deck_id,
            template_key: c.template_key,
            state: c.scheduling.state,
            due: c.scheduling.due,
            suspended: c.suspended,
        })
        .collect();
    let attributes = assignments(reads.get_attribute_values(EntityRef::Note(note.id))?);
    Ok(NoteDto {
        id: note.id,
        notetype_id: note.notetype_id,
        fields: note
            .fields
            .iter()
            .map(|(k, v)| (k.0.clone(), v.clone()))
            .collect(),
        tags: note.tags.clone(),
        attributes,
        cards,
        sort_field: note.sort_field.clone(),
        created_at: note.created_at,
        updated_at: note.updated_at,
    })
}
