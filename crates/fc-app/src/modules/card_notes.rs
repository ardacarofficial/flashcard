//! `card_notes` module (v0.3): a private note on a single card ("I always mix this up
//! with …"), shown with the card.

use crate::error::AppResult;
use crate::handler::{inverse_call, CommandCtx, CommandOutcome};
use crate::module::ModuleDescriptor;
use crate::modules::cardtypes::card_meta;
use crate::registry::Registry;
use crate::support::notes::{require_note, require_notetype};
use crate::support::{err_id, summary};
use fc_api::v1::cardtypes::{CardMetaDto, SetCardUserNote};
use fc_api::v1::events::DomainEvent;
use fc_api::v1::methods::*;
use fc_api::ErrorCode;

/// Module id.
pub const ID: &str = "card_notes";

/// Longest card note (characters).
pub const MAX_CHARS: usize = 10_000;

/// Descriptor.
pub fn descriptor() -> ModuleDescriptor {
    ModuleDescriptor {
        id: ID,
        name_key: "modules.card_notes.name",
        description_key: "modules.card_notes.description",
        enabled_by_default: true,
        can_disable: true,
        settings: vec![],
        ui: vec![],
    }
}

/// Registers the module and its methods.
pub fn register(reg: &mut Registry) {
    reg.module(descriptor());
    reg.command::<CardsSetUserNote>(set_user_note);
}

fn set_user_note(
    req: SetCardUserNote,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<CardMetaDto>> {
    let mut card = ctx
        .reads()
        .get_card(req.id)?
        .ok_or_else(|| err_id(ErrorCode::NotFoundCard, format!("card {} not found", req.id), req.id))?;
    let text = req
        .text
        .map(|t| t.trim().chars().take(MAX_CHARS).collect::<String>())
        .filter(|t| !t.is_empty());
    let old = card.user_note.clone();
    let inverse = if old == text {
        Vec::new()
    } else {
        card.user_note = text;
        card.updated_at = ctx.now;
        ctx.tx.update_card(&card)?;
        vec![inverse_call::<CardsSetUserNote>(&SetCardUserNote {
            id: card.id,
            text: old,
        })?]
    };
    let note = require_note(ctx.reads(), card.note_id)?;
    let nt = require_notetype(ctx.reads(), note.notetype_id)?;
    Ok(CommandOutcome {
        events: if inverse.is_empty() {
            Vec::new()
        } else {
            vec![DomainEvent::CardsUpdated { ids: vec![card.id] }]
        },
        inverse,
        summary: summary::<CardsSetUserNote>().param("text", &note.sort_field),
        response: card_meta(&nt, card),
    })
}
