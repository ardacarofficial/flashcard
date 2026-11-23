//! `tags` module: hierarchical tags (docs/02 §8). Tags live on notes (`note_tags`); rename and
//! merge rewrite the tag lists of the affected notes, so their inverse restores each note's
//! previous list (`notes.update` with the old content).

use crate::error::AppResult;
use crate::handler::{inverse_call, CommandCtx, CommandOutcome, QueryCtx};
use crate::module::ModuleDescriptor;
use crate::registry::Registry;
use crate::support::notes::{require_notetype, restore_content_call, write_note};
use crate::support::{err_id, summary};
use fc_api::v1::common::Empty;
use fc_api::v1::events::DomainEvent;
use fc_api::v1::methods::*;
use fc_api::v1::tags::*;
use fc_api::ErrorCode;
use fc_core::tag::{dedup_tags, Tag, TagName};
use fc_core::TagId;

/// Module id.
pub const ID: &str = "tags";

/// Descriptor.
pub fn descriptor() -> ModuleDescriptor {
    ModuleDescriptor {
        id: ID,
        name_key: "modules.tags.name",
        description_key: "modules.tags.description",
        enabled_by_default: true,
        can_disable: true,
        settings: vec![],
        ui: vec![],
    }
}

/// Registers the module and its methods.
pub fn register(reg: &mut Registry) {
    reg.module(descriptor());
    reg.query::<TagsList>(list);
    reg.command::<TagsRename>(rename);
    reg.command::<TagsMerge>(merge);
    reg.command::<TagsSetColor>(set_color);
}

/// Tags in use (by at least one live note), sorted by name.
fn list(_: Empty, ctx: &QueryCtx<'_>) -> AppResult<TagList> {
    let mut tags: Vec<TagDto> = ctx
        .tx
        .list_tags()?
        .into_iter()
        .filter(|(_, n)| *n > 0)
        .map(|(t, note_count)| TagDto {
            id: t.id,
            name: t.name,
            color: t.color,
            note_count,
        })
        .collect();
    tags.sort_by_key(|t| t.name.key());
    Ok(TagList { tags })
}

/// Rewrites the tags of every live note carrying one of `from` (or a child of it), re-rooting
/// them onto `to`. Returns the inverse calls (one per changed note).
fn retag(
    ctx: &mut CommandCtx<'_>,
    from: &[TagName],
    to: &TagName,
) -> AppResult<Vec<fc_api::InvokeRequest>> {
    let mut note_ids = Vec::new();
    for f in from {
        for id in ctx.reads().note_ids_with_tag(f)? {
            if !note_ids.contains(&id) {
                note_ids.push(id);
            }
        }
    }
    note_ids.sort();
    let mut inverse = Vec::new();
    for id in note_ids {
        let Some(mut note) = ctx.reads().get_note(id)? else {
            continue;
        };
        let old = note.clone();
        let rewritten = note.tags.iter().map(|t| {
            from.iter()
                .find_map(|f| t.rebase(f, to))
                .unwrap_or_else(|| t.clone())
        });
        note.tags = dedup_tags(rewritten);
        if note.tags == old.tags {
            continue;
        }
        let nt = require_notetype(ctx.reads(), note.notetype_id)?;
        note.updated_at = ctx.now;
        write_note(ctx, &nt, &mut note)?;
        inverse.push(restore_content_call(&old)?);
    }
    Ok(inverse)
}

/// Carries the color of `from` over to `to` if `to` has no row yet. Returns the inverse
/// (v1.4.1 H3: without it, undoing the rename left a coloured tag no note uses).
fn carry_color(
    ctx: &mut CommandCtx<'_>,
    from: &TagName,
    to: &TagName,
) -> AppResult<Option<fc_api::InvokeRequest>> {
    let Some(src) = ctx.reads().get_tag_by_name(from)? else {
        return Ok(None);
    };
    if src.color.is_some() && ctx.reads().get_tag_by_name(to)?.is_none() {
        ctx.tx.upsert_tag(&Tag {
            id: TagId::generate(ctx.ids),
            name: to.clone(),
            color: src.color,
            created_at: ctx.now,
            updated_at: ctx.now,
        })?;
        return Ok(Some(inverse_call::<TagsSetColor>(&SetTagColor {
            name: to.as_str().to_owned(),
            color: None,
        })?));
    }
    Ok(None)
}

fn rename(req: RenameTag, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<TagChangeResult>> {
    let from = TagName::parse(&req.from)?;
    let to = TagName::parse(&req.to)?;
    if ctx.reads().get_tag_by_name(&from)?.is_none()
        && ctx.reads().note_ids_with_tag(&from)?.is_empty()
    {
        return Err(err_id(
            ErrorCode::NotFoundTag,
            format!("tag {from} not found"),
            from.as_str(),
        ));
    }
    let uncolor = carry_color(ctx, &from, &to)?;
    let mut inverse = retag(ctx, std::slice::from_ref(&from), &to)?;
    let notes_affected = u32::try_from(inverse.len()).unwrap_or(u32::MAX);
    inverse.extend(uncolor);
    Ok(CommandOutcome {
        response: TagChangeResult { notes_affected },
        events: if inverse.is_empty() {
            Vec::new()
        } else {
            vec![DomainEvent::TagsChanged]
        },
        inverse,
        summary: summary::<TagsRename>()
            .param("from", from.as_str())
            .param("to", to.as_str()),
    })
}

fn merge(req: MergeTags, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<TagChangeResult>> {
    let into = TagName::parse(&req.into)?;
    let sources: Vec<TagName> = req
        .sources
        .iter()
        .map(|s| TagName::parse(s))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|s| s.key() != into.key())
        .collect();
    let inverse = retag(ctx, &sources, &into)?;
    let notes_affected = u32::try_from(inverse.len()).unwrap_or(u32::MAX);
    Ok(CommandOutcome {
        response: TagChangeResult { notes_affected },
        events: if inverse.is_empty() {
            Vec::new()
        } else {
            vec![DomainEvent::TagsChanged]
        },
        inverse,
        summary: summary::<TagsMerge>()
            .param("count", sources.len())
            .param("into", into.as_str()),
    })
}

fn set_color(req: SetTagColor, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<TagDto>> {
    let name = TagName::parse(&req.name)?;
    let mut tag = ctx.reads().get_tag_by_name(&name)?.ok_or_else(|| {
        err_id(
            ErrorCode::NotFoundTag,
            format!("tag {name} not found"),
            name.as_str(),
        )
    })?;
    let old = std::mem::replace(&mut tag.color, req.color);
    tag.updated_at = ctx.now;
    ctx.tx.upsert_tag(&tag)?;
    let note_count = ctx
        .reads()
        .list_tags()?
        .into_iter()
        .find(|(t, _)| t.id == tag.id)
        .map_or(0, |(_, n)| n);
    Ok(CommandOutcome {
        inverse: vec![inverse_call::<TagsSetColor>(&SetTagColor {
            name: tag.name.as_str().to_owned(),
            color: old,
        })?],
        summary: summary::<TagsSetColor>().param("name", tag.name.as_str()),
        events: vec![DomainEvent::TagsChanged],
        response: TagDto {
            id: tag.id,
            name: tag.name,
            color: tag.color,
            note_count,
        },
    })
}
