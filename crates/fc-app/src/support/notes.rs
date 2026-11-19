//! Note building blocks: lookups, field/tag normalization, derived columns.

use super::{deleted, err_id};
use crate::error::AppResult;
use crate::handler::{inverse_call, CommandCtx};
use fc_api::v1::methods::NotesUpdate;
use fc_api::v1::notes::UpdateNote;
use fc_api::{ErrorCode, InvokeRequest};
use fc_core::note::{note_checksum, searchable_text, sort_field_text, FieldValues, Note};
use fc_core::notetype::{FieldKey, NoteType};
use fc_core::tag::{dedup_tags, Tag, TagName};
use fc_core::{NoteId, NoteTypeId, TagId};
use fc_storage::Reads;
use std::collections::BTreeMap;

/// Note by id (trash included) or `NOT_FOUND_NOTE`.
pub(crate) fn require_note(reads: &dyn Reads, id: NoteId) -> AppResult<Note> {
    reads
        .get_note(id)?
        .ok_or_else(|| err_id(ErrorCode::NotFoundNote, format!("note {id} not found"), id))
}

/// Live note by id (`NOT_FOUND_NOTE` / `CONFLICT_DELETED`).
pub(crate) fn require_live_note(reads: &dyn Reads, id: NoteId) -> AppResult<Note> {
    let n = require_note(reads, id)?;
    if n.deleted_at.is_some() {
        return Err(deleted("note", id));
    }
    Ok(n)
}

/// Note type by id or `NOT_FOUND_NOTE_TYPE`.
pub(crate) fn require_notetype(reads: &dyn Reads, id: NoteTypeId) -> AppResult<NoteType> {
    reads.get_notetype(id)?.ok_or_else(|| {
        err_id(
            ErrorCode::NotFoundNoteType,
            format!("note type {id} not found"),
            id,
        )
    })
}

/// Wire field map → model field map.
pub(crate) fn field_values(input: &BTreeMap<String, String>) -> FieldValues {
    input
        .iter()
        .map(|(k, v)| (FieldKey::new(k.clone()), v.clone()))
        .collect()
}

/// Normalizes and de-duplicates (case-insensitively) a tag list, keeping order.
pub(crate) fn parse_tags(input: &[String]) -> AppResult<Vec<TagName>> {
    let tags = input
        .iter()
        .map(|t| TagName::parse(t))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(dedup_tags(tags))
}

/// Recomputes derived columns from `note.fields`.
pub(crate) fn derive(nt: &NoteType, note: &mut Note) {
    note.sort_field = sort_field_text(nt, &note.fields);
    note.checksum = note_checksum(nt, &note.fields);
}

/// Makes sure every tag of `tags` has a `tags` row, creating missing ones with an id from the
/// app's [`IdGen`](fc_core::IdGen) (storage never mints ids). Call before writing a note.
pub(crate) fn ensure_tag_rows(ctx: &mut CommandCtx<'_>, tags: &[TagName]) -> AppResult<()> {
    for name in tags {
        if ctx.reads().get_tag_by_name(name)?.is_none() {
            ctx.tx.upsert_tag(&Tag {
                id: TagId::generate(ctx.ids),
                name: name.clone(),
                color: None,
                created_at: ctx.now,
                updated_at: ctx.now,
            })?;
        }
    }
    Ok(())
}

/// Inserts a new note (tag rows resolved first, search text computed).
pub(crate) fn insert_note(ctx: &mut CommandCtx<'_>, nt: &NoteType, note: &Note) -> AppResult<()> {
    ensure_tag_rows(ctx, &note.tags)?;
    ctx.tx
        .insert_note(note, &searchable_text(nt, &note.fields))?;
    Ok(())
}

/// Writes an existing note (derived columns and search text recomputed, tag rows resolved).
pub(crate) fn write_note(
    ctx: &mut CommandCtx<'_>,
    nt: &NoteType,
    note: &mut Note,
) -> AppResult<()> {
    derive(nt, note);
    ensure_tag_rows(ctx, &note.tags)?;
    ctx.tx
        .update_note(note, &searchable_text(nt, &note.fields))?;
    Ok(())
}

/// A `notes.update` call restoring the fields and tags of `note` (attributes and home deck
/// untouched).
pub(crate) fn restore_content_call(note: &Note) -> AppResult<InvokeRequest> {
    inverse_call::<NotesUpdate>(&UpdateNote {
        id: note.id,
        fields: note
            .fields
            .iter()
            .map(|(k, v)| (k.0.clone(), v.clone()))
            .collect(),
        tags: note.tags.iter().map(|t| t.as_str().to_owned()).collect(),
        attributes: Vec::new(),
        deck_id: None,
    })
}
