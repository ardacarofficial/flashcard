//! `links` module (v0.3, docs/02 §14): `[[note]]` links between notes and backlinks. Links
//! are stored by normalized target text and resolved when read, so linking to a note that
//! is created later just works. Rendering resolves links regardless; switching the module
//! off hides backlinks and `[[` completion.

use crate::error::AppResult;
use crate::handler::QueryCtx;
use crate::module::ModuleDescriptor;
use crate::registry::Registry;
use crate::support::notes::require_note;
use fc_api::v1::cardtypes::{BacklinksOf, LinkedNote, LinkedNoteList, SuggestLinks};
use fc_api::v1::methods::*;
use fc_core::content::refs::link_key;

/// Module id.
pub const ID: &str = "links";

/// Descriptor.
pub fn descriptor() -> ModuleDescriptor {
    ModuleDescriptor {
        id: ID,
        name_key: "modules.links.name",
        description_key: "modules.links.description",
        enabled_by_default: true,
        can_disable: true,
        settings: vec![],
        ui: vec![],
    }
}

/// Registers the module and its methods.
pub fn register(reg: &mut Registry) {
    reg.module(descriptor());
    reg.query::<LinksBacklinks>(backlinks);
    reg.query::<LinksSuggest>(suggest);
}

fn backlinks(req: BacklinksOf, ctx: &QueryCtx<'_>) -> AppResult<LinkedNoteList> {
    let target = require_note(ctx.tx, req.note_id)?;
    let key = link_key(&target.sort_field);
    let mut items = Vec::new();
    if !key.is_empty() {
        for id in ctx.tx.notes_linking_to(&key)? {
            if id == target.id {
                continue;
            }
            let n = require_note(ctx.tx, id)?;
            items.push(LinkedNote {
                id,
                title: n.sort_field,
            });
        }
    }
    Ok(LinkedNoteList { items })
}

fn suggest(req: SuggestLinks, ctx: &QueryCtx<'_>) -> AppResult<LinkedNoteList> {
    let limit = req.limit.unwrap_or(10).clamp(1, 50);
    let items = ctx
        .tx
        .notes_with_key_prefix(&link_key(&req.prefix), limit)?
        .into_iter()
        .map(|(id, title)| LinkedNote { id, title })
        .collect();
    Ok(LinkedNoteList { items })
}
