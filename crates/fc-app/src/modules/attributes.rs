//! `attributes` module: user-defined classifications (docs/02 §8.1). Building blocks live in
//! `fc_core::attribute` (validation, option editing, inheritance, templates, storage form) and
//! `support::attrs` (applicability, validated writes, option replacement).
//!
//! Option ids are stable: rename/recolor/reorder never touch values. Removing an option
//! (`option.delete` / `option.merge`) rewrites the values that used it; the inverse is the
//! internal `attributes.option.restore` (old id, old index, every changed value verbatim) plus
//! an `attributes.update` when the default changed.

use crate::error::AppResult;
use crate::handler::{inverse_call, CommandCtx, CommandOutcome, QueryCtx};
use crate::module::{bool_setting, settings_page, ModuleDescriptor};
use crate::registry::Registry;
use crate::settings::resolve_stored;
use crate::support::attrs::{
    apply_assignments, effective_for_deck, entity_deck, replace_option, require_def,
    require_live_def, uses_option,
};
use crate::support::decks::DeckIndex;
use crate::support::{dto, invalid, store_ops, summary};
use fc_api::v1::attributes::*;
use fc_api::v1::events::DomainEvent;
use fc_api::v1::methods::*;
use fc_api::v1::oplog::OplogKind;
use fc_api::{ApiError, ErrorCode, InvokeRequest};
use fc_core::attribute::{
    builtin_templates, find_template, instantiate_template, slugify_key, validate_attribute_def,
    validate_attribute_key, validate_attribute_value, AttributeDef, AttributeOption,
    AttributeTarget, EntityRef,
};
use fc_core::error::CoreError;
use fc_core::paging::PageRequest;
use fc_core::settings::{SettingScope, SettingScopeKind};
use fc_core::{AttributeDefId, AttributeOptionId, DeckId};
use fc_storage::repo::NoteFilter;
use fc_storage::Reads;

/// Module id.
pub const ID: &str = "attributes";

/// Setting: preselect the last used value in the note form.
pub const REMEMBER_LAST: &str = "attributes.remember_last";

/// Descriptor.
pub fn descriptor() -> ModuleDescriptor {
    ModuleDescriptor {
        id: ID,
        name_key: "modules.attributes.name",
        description_key: "modules.attributes.description",
        enabled_by_default: true,
        can_disable: true,
        settings: vec![bool_setting(
            REMEMBER_LAST,
            false,
            &[SettingScopeKind::App, SettingScopeKind::Deck],
        )],
        ui: vec![settings_page(
            "attributes.manage",
            "settings.pages.attributes",
            "/settings/attributes",
            20,
        )],
    }
}

/// Registers the module and its methods.
pub fn register(reg: &mut Registry) {
    reg.module(descriptor());
    reg.query::<AttributesList>(list);
    reg.query::<AttributesEffective>(effective);
    reg.query::<AttributesTemplates>(templates);
    reg.query::<AttributesGet>(get);
    reg.command::<AttributesDefine>(define);
    reg.command::<AttributesDefineFromTemplate>(define_from_template);
    reg.command::<AttributesUpdate>(update);
    reg.command::<AttributesDelete>(delete);
    reg.command::<AttributesRestore>(restore);
    reg.command::<AttributesOptionAdd>(option_add);
    reg.command::<AttributesOptionRename>(option_rename);
    reg.command::<AttributesOptionSetColor>(option_set_color);
    reg.command::<AttributesOptionReorder>(option_reorder);
    reg.command::<AttributesOptionMerge>(option_merge);
    reg.command::<AttributesOptionDelete>(option_delete);
    reg.command::<AttributesOptionRestore>(option_restore);
    reg.query::<AttributesValuesGet>(values_get);
    reg.command::<AttributesValuesSet>(values_set);
}

// ------------------------------------------------------------------ helpers

fn normalize_name(name: &str) -> String {
    name.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// `CONFLICT_ATTRIBUTE_KEY_EXISTS` if a live definition in `scope` already uses `key`.
fn ensure_key_free(
    reads: &dyn Reads,
    scope: Option<DeckId>,
    key: &str,
    except: Option<AttributeDefId>,
) -> AppResult<()> {
    let taken = reads
        .list_attribute_defs(false)?
        .iter()
        .any(|d| d.scope_deck_id == scope && d.key == key && Some(d.id) != except);
    if taken {
        return Err(ApiError::new(
            ErrorCode::ConflictAttributeKeyExists,
            format!("attribute key {key:?} already exists here"),
        )
        .with_detail("key", key)
        .into());
    }
    Ok(())
}

fn next_position(reads: &dyn Reads, scope: Option<DeckId>) -> AppResult<i32> {
    Ok(reads
        .list_attribute_defs(false)?
        .iter()
        .filter(|d| d.scope_deck_id == scope)
        .map(|d| d.position.saturating_add(1))
        .max()
        .unwrap_or(0))
}

fn require_scope(reads: &dyn Reads, scope: Option<DeckId>) -> AppResult<()> {
    if let Some(d) = scope {
        DeckIndex::load(reads)?.require_live(d)?;
    }
    Ok(())
}

fn unknown_option(def: &AttributeDef, id: AttributeOptionId) -> CoreError {
    CoreError::UnknownAttributeOption {
        key: def.key.clone(),
        option: id.to_string(),
    }
}

/// Validates and persists a changed definition.
fn save(ctx: &mut CommandCtx<'_>, def: &mut AttributeDef) -> AppResult<AttributeDefDto> {
    def.updated_at = ctx.now;
    validate_attribute_def(def)?;
    ctx.tx.update_attribute_def(def)?;
    Ok(dto::attr_def(def))
}

/// Outcome of a definition change: response + `AttributeUpdated` event.
fn updated<M: fc_api::ApiMethod>(
    def: AttributeDefDto,
    inverse: Vec<InvokeRequest>,
) -> CommandOutcome<AttributeDefDto> {
    CommandOutcome {
        summary: summary::<M>().param("name", &def.name),
        events: vec![DomainEvent::AttributeUpdated { def: def.clone() }],
        response: def,
        inverse,
    }
}

fn insert_new(
    ctx: &mut CommandCtx<'_>,
    def: AttributeDef,
    summary_msg: fc_api::v1::common::I18nMessage,
) -> AppResult<CommandOutcome<AttributeDefDto>> {
    validate_attribute_def(&def)?;
    ensure_key_free(ctx.reads(), def.scope_deck_id, &def.key, None)?;
    ctx.tx.insert_attribute_def(&def)?;
    let dto = dto::attr_def(&def);
    Ok(CommandOutcome {
        inverse: vec![inverse_call::<AttributesDelete>(&AttributeRef {
            id: def.id,
        })?],
        summary: summary_msg.param("name", &def.name),
        events: vec![DomainEvent::AttributeDefined { def: dto.clone() }],
        response: dto,
    })
}

// ------------------------------------------------------------------ queries

fn list(req: ListAttributes, ctx: &QueryCtx<'_>) -> AppResult<AttributeDefList> {
    let mut defs: Vec<AttributeDef> = ctx
        .tx
        .list_attribute_defs(req.include_deleted.unwrap_or(false))?
        .into_iter()
        .filter(|d| req.scope_deck_id.is_none() || d.scope_deck_id == req.scope_deck_id)
        .collect();
    defs.sort_by(|a, b| {
        a.scope_deck_id
            .cmp(&b.scope_deck_id)
            .then(a.position.cmp(&b.position))
            .then_with(|| a.key.cmp(&b.key))
    });
    Ok(AttributeDefList {
        defs: defs.iter().map(dto::attr_def).collect(),
    })
}

/// Applicable definitions in form order. `suggested_value` is the default, or — with
/// `attributes.remember_last` on for the deck — the value on the deck's most recently edited
/// note.
fn effective(req: EffectiveAttributes, ctx: &QueryCtx<'_>) -> AppResult<EffectiveAttributeList> {
    let index = DeckIndex::load(ctx.tx)?;
    index.require(req.deck_id)?;
    let defs: Vec<(AttributeDef, bool)> = effective_for_deck(ctx.tx, &index, Some(req.deck_id))?
        .into_iter()
        .filter(|(d, _)| d.target == req.target)
        .collect();
    let remember = resolve_stored(ctx.tx, &SettingScope::Deck(req.deck_id), REMEMBER_LAST)?
        .and_then(|(v, _)| v.as_bool())
        .unwrap_or(false);
    let last_values = if remember && req.target == AttributeTarget::Note {
        let page = ctx.tx.list_notes(
            &NoteFilter {
                deck_ids: Some(index.live_subtree(req.deck_id)),
                include_deleted: false,
            },
            &PageRequest {
                cursor: None,
                limit: Some(1),
            },
        )?;
        match page.items.first() {
            Some(row) => ctx.tx.get_attribute_values(EntityRef::Note(row.note.id))?,
            None => Vec::new(),
        }
    } else {
        Vec::new()
    };
    let attributes = defs
        .into_iter()
        .map(|(def, inherited)| {
            let last = last_values
                .iter()
                .find(|(id, _)| *id == def.id)
                .map(|(_, v)| v.clone());
            EffectiveAttributeDto {
                suggested_value: last.or_else(|| def.default_value.clone()),
                def: dto::attr_def(&def),
                inherited,
            }
        })
        .collect();
    Ok(EffectiveAttributeList { attributes })
}

fn templates(req: ListAttributeTemplates, ctx: &QueryCtx<'_>) -> AppResult<AttributeTemplateList> {
    let locale = req.locale.unwrap_or(ctx.locale);
    Ok(AttributeTemplateList {
        templates: builtin_templates()
            .iter()
            .map(|t| AttributeTemplateDto {
                id: t.id.to_owned(),
                key: t.key.to_owned(),
                name: t.name.get(locale).to_owned(),
                kind: t.kind,
                options: t.options.iter().map(|o| o.get(locale).to_owned()).collect(),
            })
            .collect(),
    })
}

fn get(req: AttributeRef, ctx: &QueryCtx<'_>) -> AppResult<AttributeDefDto> {
    Ok(dto::attr_def(&require_def(ctx.tx, req.id)?))
}

fn values_get(req: GetAttributeValues, ctx: &QueryCtx<'_>) -> AppResult<AttributeValues> {
    entity_deck(ctx.tx, req.entity)?;
    Ok(AttributeValues {
        entity: req.entity,
        values: dto::assignments(ctx.tx.get_attribute_values(req.entity)?),
    })
}

// ------------------------------------------------------------------ definitions

fn define(
    req: DefineAttribute,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<AttributeDefDto>> {
    require_scope(ctx.reads(), req.scope_deck_id)?;
    let name = normalize_name(&req.name);
    let key = match &req.key {
        Some(k) => k.trim().to_owned(),
        None => slugify_key(&name),
    };
    validate_attribute_key(&key)?;
    let mut def = AttributeDef {
        id: AttributeDefId::generate(ctx.ids),
        scope_deck_id: req.scope_deck_id,
        key,
        name,
        kind: req.kind,
        options: Vec::new(),
        target: req.target.unwrap_or(AttributeTarget::Note),
        required: req.required.unwrap_or(false),
        default_value: None,
        position: next_position(ctx.reads(), req.scope_deck_id)?,
        template_id: None,
        created_at: ctx.now,
        updated_at: ctx.now,
        deleted_at: None,
    };
    for o in &req.options {
        let option = AttributeOption {
            id: AttributeOptionId::generate(ctx.ids),
            label: o.label.clone(),
            color: o.color.clone(),
            order: 0,
        };
        def.add_option(option, None)?;
    }
    insert_new(ctx, def, summary::<AttributesDefine>())
}

fn define_from_template(
    req: DefineAttributeFromTemplate,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<AttributeDefDto>> {
    let template = find_template(&req.template_id).ok_or_else(|| {
        ApiError::new(
            ErrorCode::NotFoundAttributeTemplate,
            format!("template {:?} not found", req.template_id),
        )
        .with_detail("id", req.template_id.clone())
    })?;
    require_scope(ctx.reads(), req.scope_deck_id)?;
    let position = next_position(ctx.reads(), req.scope_deck_id)?;
    let locale = req.locale.unwrap_or(ctx.locale);
    let def = instantiate_template(
        template,
        req.scope_deck_id,
        locale,
        position,
        ctx.ids,
        ctx.now,
    );
    insert_new(ctx, def, summary::<AttributesDefineFromTemplate>())
}

fn update(
    req: UpdateAttribute,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<AttributeDefDto>> {
    let mut def = require_live_def(ctx.reads(), req.id)?;
    let inverse = UpdateAttribute {
        id: def.id,
        name: def.name.clone(),
        required: def.required,
        default_value: def.default_value.clone(),
        position: def.position,
    };
    def.name = normalize_name(&req.name);
    def.required = req.required;
    def.default_value = match &req.default_value {
        Some(v) => Some(validate_attribute_value(&def, v)?),
        None => None,
    };
    def.position = req.position;
    let dto = save(ctx, &mut def)?;
    Ok(updated::<AttributesUpdate>(
        dto,
        vec![inverse_call::<AttributesUpdate>(&inverse)?],
    ))
}

/// Trashes a definition; its values are kept (hidden) and come back with `attributes.restore`.
fn delete(
    req: AttributeRef,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<AttributeDefDto>> {
    let def = require_def(ctx.reads(), req.id)?;
    let dto = dto::attr_def(&def);
    if def.deleted_at.is_some() {
        return Ok(CommandOutcome {
            response: dto,
            inverse: Vec::new(),
            summary: summary::<AttributesDelete>().param("name", &def.name),
            events: Vec::new(),
        });
    }
    ctx.tx.set_attribute_def_deleted(def.id, Some(ctx.now))?;
    Ok(CommandOutcome {
        response: dto,
        inverse: vec![inverse_call::<AttributesRestore>(&req)?],
        summary: summary::<AttributesDelete>().param("name", &def.name),
        events: vec![DomainEvent::AttributeDeleted { id: def.id }],
    })
}

fn restore(
    req: AttributeRef,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<AttributeDefDto>> {
    let mut def = require_def(ctx.reads(), req.id)?;
    if def.deleted_at.is_none() {
        return Ok(CommandOutcome {
            summary: summary::<AttributesRestore>().param("name", &def.name),
            response: dto::attr_def(&def),
            inverse: Vec::new(),
            events: Vec::new(),
        });
    }
    ensure_key_free(ctx.reads(), def.scope_deck_id, &def.key, Some(def.id))?;
    ctx.tx.set_attribute_def_deleted(def.id, None)?;
    def.deleted_at = None;
    let dto = dto::attr_def(&def);
    Ok(CommandOutcome {
        inverse: vec![inverse_call::<AttributesDelete>(&req)?],
        summary: summary::<AttributesRestore>().param("name", &def.name),
        events: vec![DomainEvent::AttributeDefined { def: dto.clone() }],
        response: dto,
    })
}

// ------------------------------------------------------------------ options

fn option_add(
    req: AddAttributeOption,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<AttributeDefDto>> {
    let mut def = require_live_def(ctx.reads(), req.attr_id)?;
    let id = match req.id {
        Some(id) => id,
        None => AttributeOptionId::generate(ctx.ids),
    };
    def.add_option(
        AttributeOption {
            id,
            label: req.label.clone(),
            color: req.color.clone(),
            order: 0,
        },
        req.after,
    )?;
    let dto = save(ctx, &mut def)?;
    let inverse = inverse_call::<AttributesOptionDelete>(&DeleteAttributeOption {
        attr_id: def.id,
        option_id: id,
        replace_with: None,
    })?;
    Ok(updated::<AttributesOptionAdd>(dto, vec![inverse]))
}

fn option_rename(
    req: RenameAttributeOption,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<AttributeDefDto>> {
    let mut def = require_live_def(ctx.reads(), req.attr_id)?;
    let old = def.rename_option(req.option_id, &req.label)?;
    let dto = save(ctx, &mut def)?;
    let inverse = inverse_call::<AttributesOptionRename>(&RenameAttributeOption {
        attr_id: def.id,
        option_id: req.option_id,
        label: old,
    })?;
    Ok(updated::<AttributesOptionRename>(dto, vec![inverse]))
}

fn option_set_color(
    req: SetAttributeOptionColor,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<AttributeDefDto>> {
    let mut def = require_live_def(ctx.reads(), req.attr_id)?;
    let old = def.set_option_color(req.option_id, req.color.clone())?;
    let dto = save(ctx, &mut def)?;
    let inverse = inverse_call::<AttributesOptionSetColor>(&SetAttributeOptionColor {
        attr_id: def.id,
        option_id: req.option_id,
        color: old,
    })?;
    Ok(updated::<AttributesOptionSetColor>(dto, vec![inverse]))
}

fn option_reorder(
    req: ReorderAttributeOptions,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<AttributeDefDto>> {
    let mut def = require_live_def(ctx.reads(), req.attr_id)?;
    let previous = def.reorder_options(&req.order)?;
    let dto = save(ctx, &mut def)?;
    let inverse = inverse_call::<AttributesOptionReorder>(&ReorderAttributeOptions {
        attr_id: def.id,
        order: previous,
    })?;
    Ok(updated::<AttributesOptionReorder>(dto, vec![inverse]))
}

/// Removes `option`, rewriting values (and the default) that use it to `replacement` (or
/// dropping it). Returns the number of values changed, the inverse calls and the new def.
///
/// The inverse is the internal `attributes.option.restore` (option back under its id at its
/// index, every changed value back verbatim), then — if the default changed — an
/// `attributes.update` restoring it.
fn remove_option(
    ctx: &mut CommandCtx<'_>,
    attr_id: AttributeDefId,
    option: AttributeOptionId,
    replacement: Option<AttributeOptionId>,
) -> AppResult<(u32, Vec<InvokeRequest>, AttributeDef)> {
    let def = require_live_def(ctx.reads(), attr_id)?;
    let removed = def
        .option(option)
        .cloned()
        .ok_or_else(|| unknown_option(&def, option))?;
    if let Some(r) = replacement {
        if r == option {
            return Err(invalid("an option cannot be replaced by itself"));
        }
        def.option(r).ok_or_else(|| unknown_option(&def, r))?;
    }
    let index = def
        .sorted_options()
        .iter()
        .position(|o| o.id == option)
        .unwrap_or_default();
    let strict = ctx.batch_kind == OplogKind::Do;
    if strict && def.required && replacement.is_none() {
        let in_use = ctx.reads().entities_with_option(def.id, option)?.len();
        if in_use > 0 {
            return Err(ApiError::new(
                ErrorCode::ConflictAttributeOptionInUse,
                format!(
                    "option {:?} of required {:?} is in use",
                    removed.label, def.key
                ),
            )
            .with_detail("id", option.to_string())
            .with_detail("count", in_use)
            .into());
        }
    }
    let changed = store_ops::remove_option(ctx.tx, def.id, option, replacement, ctx.now)?;
    let old = def.clone();
    let mut def = require_def(ctx.reads(), def.id)?;
    let default_changed = old
        .default_value
        .as_ref()
        .is_some_and(|d| uses_option(d, option));
    if default_changed {
        def.default_value = old
            .default_value
            .as_ref()
            .and_then(|d| replace_option(d, option, replacement));
    }
    save(ctx, &mut def)?;

    let affected = u32::try_from(changed.len()).unwrap_or(u32::MAX);
    let mut inverse = vec![inverse_call::<AttributesOptionRestore>(
        &RestoreAttributeOption {
            attr_id: def.id,
            option: removed,
            index: u32::try_from(index).unwrap_or(u32::MAX),
            replaced_by: replacement,
            values: changed
                .into_iter()
                .map(|(entity, value)| EntityAttributeValue { entity, value })
                .collect(),
        },
    )?];
    if default_changed {
        inverse.push(inverse_call::<AttributesUpdate>(&UpdateAttribute {
            id: old.id,
            name: old.name,
            required: old.required,
            default_value: old.default_value,
            position: old.position,
        })?);
    }
    Ok((affected, inverse, def))
}

/// Internal: re-inserts a removed option at its index and writes the listed values back
/// verbatim (still validated against the definition). The inverse removes it again with the
/// original replacement, which recomputes the same values from this state.
fn option_restore(
    req: RestoreAttributeOption,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<AttributeDefDto>> {
    let def = require_live_def(ctx.reads(), req.attr_id)?;
    if let Some(r) = req.replaced_by {
        def.option(r).ok_or_else(|| unknown_option(&def, r))?;
    }
    let index = usize::try_from(req.index).unwrap_or(usize::MAX);
    store_ops::insert_option(ctx.tx, def.id, &req.option, index, ctx.now)?;
    let mut def = require_def(ctx.reads(), def.id)?;
    let mut writes = Vec::with_capacity(req.values.len());
    for v in &req.values {
        entity_deck(ctx.reads(), v.entity)?;
        let value = validate_attribute_value(&def, &v.value)?;
        writes.push((v.entity, value));
    }
    for (entity, value) in writes {
        ctx.tx
            .set_attribute_values(entity, &[(def.id, Some(value))])?;
    }
    let dto = save(ctx, &mut def)?;
    let inverse = inverse_call::<AttributesOptionDelete>(&DeleteAttributeOption {
        attr_id: def.id,
        option_id: req.option.id,
        replace_with: req.replaced_by,
    })?;
    Ok(updated::<AttributesOptionRestore>(dto, vec![inverse]))
}

fn option_outcome<M: fc_api::ApiMethod>(
    (values_affected, inverse, def): (u32, Vec<InvokeRequest>, AttributeDef),
) -> CommandOutcome<OptionChangeResult> {
    CommandOutcome {
        response: OptionChangeResult { values_affected },
        inverse,
        summary: summary::<M>()
            .param("name", &def.name)
            .param("count", values_affected),
        events: vec![DomainEvent::AttributeUpdated {
            def: dto::attr_def(&def),
        }],
    }
}

fn option_merge(
    req: MergeAttributeOptions,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<OptionChangeResult>> {
    let result = remove_option(ctx, req.attr_id, req.from, Some(req.into))?;
    Ok(option_outcome::<AttributesOptionMerge>(result))
}

fn option_delete(
    req: DeleteAttributeOption,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<OptionChangeResult>> {
    let result = remove_option(ctx, req.attr_id, req.option_id, req.replace_with)?;
    Ok(option_outcome::<AttributesOptionDelete>(result))
}

// ------------------------------------------------------------------ values

fn values_set(
    req: SetAttributeValues,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<AttributeValues>> {
    let strict = ctx.batch_kind == OplogKind::Do;
    let previous = apply_assignments(ctx.tx, strict, req.entity, &req.values)?;
    let response = AttributeValues {
        entity: req.entity,
        values: dto::assignments(ctx.reads().get_attribute_values(req.entity)?),
    };
    Ok(CommandOutcome {
        response,
        inverse: vec![inverse_call::<AttributesValuesSet>(&SetAttributeValues {
            entity: req.entity,
            values: previous,
        })?],
        summary: summary::<AttributesValuesSet>().param("count", req.values.len()),
        events: vec![DomainEvent::AttributeValuesChanged {
            entities: vec![req.entity],
        }],
    })
}
