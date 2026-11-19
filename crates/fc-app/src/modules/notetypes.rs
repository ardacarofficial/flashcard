//! `notetypes` module: note type schemas (read-only in v0.1; editor in v0.3).

use crate::error::AppResult;
use crate::handler::QueryCtx;
use crate::module::ModuleDescriptor;
use crate::registry::Registry;
use crate::support::dto;
use crate::support::notes::require_notetype;
use fc_api::v1::common::Empty;
use fc_api::v1::methods::*;
use fc_api::v1::notetypes::{NoteTypeDto, NoteTypeList, NoteTypeRef};

/// Module id.
pub const ID: &str = "notetypes";

/// Descriptor.
pub fn descriptor() -> ModuleDescriptor {
    ModuleDescriptor {
        id: ID,
        name_key: "modules.notetypes.name",
        description_key: "modules.notetypes.description",
        enabled_by_default: true,
        can_disable: false,
        settings: vec![],
        ui: vec![],
    }
}

/// Registers the module and its methods.
pub fn register(reg: &mut Registry) {
    reg.module(descriptor());
    reg.query::<NotetypesList>(list);
    reg.query::<NotetypesGet>(get);
}

fn list(_: Empty, ctx: &QueryCtx<'_>) -> AppResult<NoteTypeList> {
    let mut all = ctx.tx.list_notetypes(false)?;
    all.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then(a.id.cmp(&b.id))
    });
    Ok(NoteTypeList {
        notetypes: all.iter().map(dto::notetype).collect(),
    })
}

fn get(req: NoteTypeRef, ctx: &QueryCtx<'_>) -> AppResult<NoteTypeDto> {
    Ok(dto::notetype(&require_notetype(ctx.tx, req.id)?))
}
