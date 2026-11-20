//! Applying AI results: adding reviewed drafts as notes and applying a suggested rewrite.
//! Both run the ordinary note commands nested, so validation, cards, classifications and undo
//! are exactly those of `notes.create` / `notes.update` — and the whole apply is one undo step.

use super::context::task_error;
use crate::error::{AppError, AppResult};
use crate::handler::{inverse_call, CommandCtx, CommandOutcome};
use crate::registry::Registry;
use crate::support::notes::{require_live_note, require_notetype};
use crate::support::summary;
use fc_api::v1::ai::{AddAiNotes, AiNotesAdded, UpdateAiNote};
use fc_api::v1::methods::{AiNotesAdd, AiNotesUpdate, NotesCreate, NotesUpdate};
use fc_api::v1::notes::{CreateNote, NoteDto, UpdateNote};
use fc_api::ErrorCode;

/// Most drafts per call.
const MAX_NOTES: usize = 200;

pub(super) fn register(reg: &mut Registry) {
    reg.command::<AiNotesAdd>(add);
    reg.command::<AiNotesUpdate>(update);
}

fn add(req: AddAiNotes, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<AiNotesAdded>> {
    if req.notes.is_empty() || req.notes.len() > MAX_NOTES {
        return Err(task_error("count", "1–200 notes per call"));
    }
    let mut note_ids = Vec::new();
    let mut skipped = 0;
    for draft in &req.notes {
        if draft.fields.values().all(|v| v.trim().is_empty()) {
            skipped += 1;
            continue;
        }
        let create = CreateNote {
            notetype_id: req.notetype_id,
            deck_id: req.deck_id,
            fields: draft.fields.clone(),
            tags: req.tags.clone(),
            attributes: req.attributes.clone(),
            allow_duplicate: Some(false),
        };
        // A duplicate is refused before `notes.create` writes anything, so skipping it
        // leaves the transaction clean.
        match ctx.call(&inverse_call::<NotesCreate>(&create)?) {
            Ok(v) => {
                let note: NoteDto = crate::support::from_json(&v)?;
                note_ids.push(note.id);
            }
            Err(AppError::Api(e)) if e.code == ErrorCode::ConflictNoteDuplicate => skipped += 1,
            Err(e) => return Err(e),
        }
    }
    let count = note_ids.len();
    Ok(CommandOutcome {
        response: AiNotesAdded { note_ids, skipped },
        // Undone through the nested `notes.create` entries (same batch).
        inverse: vec![],
        summary: summary::<AiNotesAdd>().param("count", count),
        events: vec![],
    })
}

fn update(req: UpdateAiNote, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<NoteDto>> {
    let note = require_live_note(ctx.reads(), req.note_id)?;
    let nt = require_notetype(ctx.reads(), note.notetype_id)?;
    let mut fields: std::collections::BTreeMap<String, String> =
        note.fields.iter().map(|(k, v)| (k.0.clone(), v.clone())).collect();
    for (k, v) in &req.fields {
        if !nt.fields.iter().any(|f| f.key.0 == *k) {
            return Err(task_error("field", format!("the note has no field {k:?}")));
        }
        fields.insert(k.clone(), v.clone());
    }
    let v = ctx.call(&inverse_call::<NotesUpdate>(&UpdateNote {
        id: note.id,
        deck_id: None,
        fields,
        tags: note.tags.iter().map(|t| t.as_str().to_owned()).collect(),
        attributes: vec![],
    })?)?;
    Ok(CommandOutcome {
        response: crate::support::from_json(&v)?,
        inverse: vec![],
        summary: summary::<AiNotesUpdate>(),
        events: vec![],
    })
}
