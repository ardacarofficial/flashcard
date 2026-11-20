//! `inline_cards` module: inline card creation (docs/02 §14, RemNote style). Every
//! `question :: answer` line of a text (`fc_core::inline_cards`) becomes a note: question in
//! the note type's first field, answer in its second. `modes.inline.preview` shows what a text
//! would make (and which lines are duplicates); `modes.inline.create` runs `notes.create` nested
//! per line — validation, cards, classification defaults and duplicate detection are exactly
//! `notes.create`'s — as **one undo step**. It never touches a running study session, so the
//! study screen (and the reading screen) can offer it without leaving the card.

use crate::error::AppResult;
use crate::handler::{inverse_call, CommandCtx, CommandOutcome, QueryCtx};
use crate::module::ModuleDescriptor;
use crate::registry::Registry;
use crate::support::notes::{field_values, require_notetype};
use crate::support::{deleted, summary};
use crate::AppError;
use fc_api::v1::methods::{ModesInlineCreate, ModesInlinePreview, NotesCreate};
use fc_api::v1::modes::*;
use fc_api::v1::notes::{CreateNote, NoteDto};
use fc_api::{ApiError, ErrorCode};
use fc_core::inline_cards::{parse, InlineCard};
use fc_core::note::{normalize_note_fields, note_checksum};
use fc_core::notetype::{builtin::CLASSIC_ID, NoteType};
use fc_core::NoteTypeId;
use fc_storage::Reads;
use std::collections::BTreeMap;

/// Module id.
pub const ID: &str = "inline_cards";

/// Descriptor.
pub fn descriptor() -> ModuleDescriptor {
    ModuleDescriptor {
        id: ID,
        name_key: "modules.inline_cards.name",
        description_key: "modules.inline_cards.description",
        enabled_by_default: true,
        can_disable: true,
        settings: vec![],
        ui: vec![],
    }
}

/// Registers the module.
pub fn register(reg: &mut Registry) {
    reg.module(descriptor());
    reg.query::<ModesInlinePreview>(preview);
    reg.command::<ModesInlineCreate>(create);
}

fn inline_error(reason: &str, message: &str) -> AppError {
    AppError::Api(
        ApiError::new(ErrorCode::ValidationInlineCards, message).with_detail("reason", reason),
    )
}

/// The note type and its (question, answer) field keys.
fn target(reads: &dyn Reads, id: Option<NoteTypeId>) -> AppResult<(NoteType, String, String)> {
    let nt = require_notetype(reads, id.unwrap_or(CLASSIC_ID))?;
    if nt.deleted_at.is_some() {
        return Err(deleted("notetype", nt.id));
    }
    match (nt.fields.first(), nt.fields.get(1)) {
        (Some(q), Some(a)) => {
            let (q, a) = (q.key.0.clone(), a.key.0.clone());
            Ok((nt, q, a))
        }
        _ => Err(inline_error(
            "fields",
            "the note type needs a question and an answer field",
        )),
    }
}

/// Field text of one side: plain text, so `<`, `>` and `&` are escaped.
fn field_text(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn fields(c: &InlineCard, q: &str, a: &str) -> BTreeMap<String, String> {
    BTreeMap::from([
        (q.to_owned(), field_text(&c.front)),
        (a.to_owned(), field_text(&c.back)),
    ])
}

fn preview(req: InlinePreviewRequest, ctx: &QueryCtx<'_>) -> AppResult<InlinePreview> {
    let (nt, q, a) = target(ctx.tx, req.notetype_id)?;
    let parsed = parse(&req.text);
    let mut cards = Vec::with_capacity(parsed.cards.len());
    for c in parsed.cards {
        let values = normalize_note_fields(&nt, &field_values(&fields(&c, &q, &a)))?;
        let checksum = note_checksum(&nt, &values);
        let duplicate = !ctx.tx.find_notes_by_checksum(nt.id, &checksum)?.is_empty();
        cards.push(InlineCardDto {
            line: c.line,
            front: c.front,
            back: c.back,
            duplicate,
        });
    }
    Ok(InlinePreview {
        cards,
        truncated: parsed.truncated,
    })
}

fn create(
    req: CreateInlineCards,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<InlineCardsCreated>> {
    let (nt, q, a) = target(ctx.reads(), req.notetype_id)?;
    let parsed = parse(&req.text);
    if parsed.cards.is_empty() {
        return Err(inline_error("no_cards", "no `question :: answer` line"));
    }
    let mut note_ids = Vec::new();
    let mut duplicates = 0u32;
    for c in &parsed.cards {
        let create = CreateNote {
            notetype_id: nt.id,
            deck_id: req.deck_id,
            fields: fields(c, &q, &a),
            tags: req.tags.clone(),
            attributes: vec![],
            allow_duplicate: Some(false),
        };
        // A duplicate is refused before `notes.create` writes anything (as `ai.notes.add`).
        match ctx.call(&inverse_call::<NotesCreate>(&create)?) {
            Ok(v) => {
                let note: NoteDto = crate::support::from_json(&v)?;
                note_ids.push(note.id);
            }
            Err(AppError::Api(e)) if e.code == ErrorCode::ConflictNoteDuplicate => duplicates += 1,
            Err(e) => return Err(e),
        }
    }
    let count = note_ids.len();
    Ok(CommandOutcome {
        response: InlineCardsCreated {
            note_ids,
            duplicates,
            truncated: parsed.truncated,
        },
        // Undone through the nested `notes.create` entries (same batch).
        inverse: vec![],
        summary: summary::<ModesInlineCreate>().param("count", count),
        events: vec![],
    })
}
