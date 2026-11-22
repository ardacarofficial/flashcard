//! `cardtypes` module (v0.3, docs/02 §3–§4): the card type engines, the note type editor
//! (create / update / trash / restore), card rendering and preview. Backbone: rendering is
//! needed everywhere cards are shown, so it cannot be switched off.
//!
//! Schema changes never break cards or their history: fields are matched by stable key,
//! removed fields drop their values (undo restores them via `notetypes.restore_version`),
//! templates that still have cards cannot be removed, new templates generate their cards.

use crate::error::{AppError, AppResult};
use crate::handler::{inverse_call, CommandCtx, CommandOutcome, QueryCtx};
use crate::module::{main_menu, ModuleDescriptor};
use crate::registry::Registry;
use crate::support::content::{
    add_missing_cards, card_seed, card_specs, deck_for_new_cards, fields_snapshot, from_snapshot,
    is_empty_card, render,
    snapshot, sync_card_types, template_for, validate_full, RenderSetup,
};
use crate::support::decks::DeckIndex;
use crate::support::notes::{
    field_values, parse_tags, require_note, require_notetype, write_note,
};
use crate::support::{deleted, dto, err_id, summary};
use fc_api::v1::cardtypes::*;
use fc_api::v1::common::{AffectedCount, Empty};
use fc_api::v1::events::DomainEvent;
use fc_api::v1::methods::*;
use fc_api::v1::notetypes::{NoteTypeDto, NoteTypeRef};
use fc_api::{ApiError, ErrorCode};
use fc_core::card::Card;
use fc_core::content::builtin;
use fc_core::content::validate::assign_missing_keys;
use fc_core::note::{fnv1a64, normalize_note_fields, FieldValues, Note};
use fc_core::notetype::{builtin as classic_builtin, FieldKey, NoteType, NoteTypeKind};
use fc_core::{CardId, NoteId, NoteTypeId};
use fc_providers::card_types::builtin_engines;
use fc_storage::repo::CardFilter;
use std::collections::{BTreeMap, BTreeSet};

/// Module id.
pub const ID: &str = "cardtypes";

/// Descriptor.
pub fn descriptor() -> ModuleDescriptor {
    ModuleDescriptor {
        id: ID,
        name_key: "modules.cardtypes.name",
        description_key: "modules.cardtypes.description",
        enabled_by_default: true,
        can_disable: false,
        settings: vec![],
        ui: vec![main_menu(
            "cardtypes.notetypes",
            "nav.notetypes",
            "layers",
            "/notetypes",
            60,
        )],
    }
}

/// Registers the module, its methods and the built-in card type engines.
pub fn register(reg: &mut Registry) {
    reg.module(descriptor());
    for engine in builtin_engines() {
        reg.card_type(engine);
    }
    reg.command::<NotetypesCreate>(create);
    reg.command::<NotetypesUpdate>(update);
    reg.command::<NotetypesDelete>(delete);
    reg.command::<NotetypesRestore>(restore);
    reg.command::<NotetypesRestoreVersion>(restore_version);
    reg.query::<NotetypesUsage>(usage);
    reg.query::<CardtypesList>(list_card_types);
    reg.command::<CardsTrashGenerated>(trash_generated);
    reg.command::<CardsRestoreGenerated>(restore_generated);
    reg.query::<CardsRender>(render_card);
    reg.query::<CardsPreview>(preview);
    reg.query::<CardsMeta>(cards_meta);
}

// ------------------------------------------------------------------ helpers

fn invalid_nt(reason: impl Into<String>) -> AppError {
    let reason = reason.into();
    AppError::Api(
        ApiError::new(ErrorCode::ValidationNoteType, reason.clone()).with_detail("reason", reason),
    )
}

/// Builds a note type from editor input (keys assigned, CSS kept raw: sanitized on render).
fn from_input(input: NoteTypeInput, base: &NoteType) -> NoteType {
    let mut nt = NoteType {
        id: base.id,
        name: input.name.trim().to_owned(),
        kind: input.kind,
        fields: input.fields,
        templates: input.templates,
        css: input.css,
        sort_field: input.sort_field,
        version: base.version,
        builtin: base.builtin.clone(),
        created_at: base.created_at,
        updated_at: base.updated_at,
        deleted_at: base.deleted_at,
    };
    for f in &mut nt.fields {
        f.name = f.name.trim().to_owned();
    }
    assign_missing_keys(&mut nt);
    // v1.3: a template whose answer field was removed asks for its whole answer side again
    // (the field's role went with it).
    let keys: std::collections::BTreeSet<FieldKey> =
        nt.fields.iter().map(|f| f.key.clone()).collect();
    for t in &mut nt.templates {
        if t.answer_field.as_ref().is_some_and(|k| !keys.contains(k)) {
            t.answer_field = None;
        }
    }
    if nt.sort_field.0.trim().is_empty() || nt.field(&nt.sort_field).is_none() {
        if let Some(first) = nt.fields.first() {
            nt.sort_field = first.key.clone();
        }
    }
    nt
}

/// A minimal front/back definition (rich text fields, one classic template).
fn minimal(name: &str, base: &NoteType) -> NoteType {
    let mut nt = builtin::classic(base.created_at, fc_core::i18n::Locale::En);
    nt.id = base.id;
    nt.name = name.to_owned();
    nt.builtin = None;
    nt.created_at = base.created_at;
    nt.updated_at = base.updated_at;
    nt
}

fn usage_counts(
    ctx_reads: &dyn fc_storage::Reads,
    nt: NoteTypeId,
) -> AppResult<(u32, BTreeMap<String, u32>)> {
    let live = ctx_reads.note_ids_of_notetype(nt, false)?;
    let all = ctx_reads.note_ids_of_notetype(nt, true)?;
    let mut by_template = BTreeMap::new();
    if !all.is_empty() {
        for c in ctx_reads.list_cards(&CardFilter {
            deck_ids: None,
            note_ids: Some(all),
            include_deleted: true,
        })? {
            *by_template.entry(c.template_key).or_insert(0u32) += 1;
        }
    }
    Ok((u32::try_from(live.len()).unwrap_or(u32::MAX), by_template))
}

fn changed(nt: &NoteType) -> DomainEvent {
    DomainEvent::NoteTypeChanged { notetype_id: nt.id }
}

// ------------------------------------------------------------------ note type editor

fn create(req: CreateNoteType, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<NoteTypeDto>> {
    let id = match req.id {
        Some(id) => id,
        None => NoteTypeId::generate(ctx.ids),
    };
    if ctx.reads().get_notetype(id)?.is_some() {
        return Err(invalid_nt(format!("note type {id} already exists")));
    }
    let base = NoteType {
        id,
        name: String::new(),
        kind: NoteTypeKind::Standard,
        fields: Vec::new(),
        templates: Vec::new(),
        css: String::new(),
        sort_field: FieldKey::new(""),
        version: 1,
        builtin: None,
        created_at: ctx.now,
        updated_at: ctx.now,
        deleted_at: None,
    };
    let mut nt = match (req.definition, req.copy_from) {
        (Some(def), _) => from_input(def, &base),
        (None, Some(src)) => {
            let src = require_notetype(ctx.reads(), src)?;
            NoteType {
                name: format!("{} (2)", src.name),
                kind: src.kind,
                fields: src.fields,
                templates: src.templates,
                css: src.css,
                sort_field: src.sort_field,
                ..base.clone()
            }
        }
        (None, None) => minimal(req.name.as_deref().unwrap_or("Note type"), &base),
    };
    if let Some(name) = req.name {
        nt.name = name.trim().to_owned();
    }
    validate_full(ctx.registry, &nt)?;
    ctx.tx.upsert_notetype(&nt)?;
    Ok(CommandOutcome {
        inverse: vec![inverse_call::<NotetypesDelete>(&NoteTypeRef { id: nt.id })?],
        summary: summary::<NotetypesCreate>().param("name", &nt.name),
        events: vec![changed(&nt)],
        response: dto::notetype(&nt),
    })
}

fn update(req: UpdateNoteType, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<NoteTypeDto>> {
    let old = require_notetype(ctx.reads(), req.id)?;
    if old.deleted_at.is_some() {
        return Err(deleted("notetype", old.id));
    }
    let mut nt = from_input(req.definition, &old);
    nt.version = old.version.saturating_add(1);
    nt.updated_at = ctx.now;
    validate_full(ctx.registry, &nt)?;

    let note_ids = ctx.reads().note_ids_of_notetype(old.id, true)?;
    if nt.kind != old.kind && !note_ids.is_empty() {
        return Err(invalid_nt("the kind of a note type with notes cannot change"));
    }
    // A removed template must not orphan cards (their history would be unreachable).
    let (_, by_template) = usage_counts(ctx.reads(), old.id)?;
    for t in &old.templates {
        let still = template_for(&nt, &t.key).is_some_and(|n| n.key == t.key)
            || (nt.templates.len() == 1 && old.templates.len() == 1);
        let cards = by_template.get(&t.key).copied().unwrap_or(0);
        if !still && cards > 0 {
            return Err(AppError::Api(
                ApiError::new(
                    ErrorCode::ConflictTemplateInUse,
                    format!("template {} still has {cards} cards", t.key),
                )
                .with_detail("template", t.key.clone())
                .with_detail("cards", cards),
            ));
        }
    }

    // Field values follow the schema: removed keys dropped, new keys at their default.
    let fields_changed = old.fields != nt.fields || old.sort_field != nt.sort_field;
    let mut snapshots = Vec::new();
    let mut notes: Vec<Note> = Vec::new();
    if fields_changed {
        for id in &note_ids {
            let note = require_note(ctx.reads(), *id)?;
            snapshots.push(fields_snapshot(&note));
            notes.push(note);
        }
    }
    ctx.tx.upsert_notetype(&nt)?;
    for mut note in notes {
        let mut fields = FieldValues::new();
        for f in &nt.fields {
            let v = note
                .fields
                .get(&f.key)
                .cloned()
                .or_else(|| f.default_value.clone())
                .unwrap_or_default();
            fields.insert(f.key.clone(), v);
        }
        note.fields = fields;
        write_note(ctx, &nt, &mut note)?;
    }
    sync_card_types(ctx, &nt, ctx.now)?;

    // New/changed templates may yield new cards for existing live notes.
    let mut new_cards = Vec::new();
    if old.templates != nt.templates {
        for id in ctx.reads().note_ids_of_notetype(nt.id, false)? {
            let note = require_note(ctx.reads(), id)?;
            let deck = deck_for_new_cards(ctx.reads(), &note)?;
            new_cards.extend(add_missing_cards(ctx, &nt, &note, deck)?);
        }
    }

    let mut inverse = vec![inverse_call::<NotetypesRestoreVersion>(&RestoreNoteTypeVersion {
        notetype: snapshot(&old),
        notes: snapshots,
    })?];
    let mut events = vec![changed(&nt)];
    if !new_cards.is_empty() {
        inverse.push(inverse_call::<CardsTrashGenerated>(&CardIdList {
            ids: new_cards.clone(),
        })?);
        events.push(DomainEvent::CardsUpdated { ids: new_cards });
    }
    Ok(CommandOutcome {
        inverse,
        summary: summary::<NotetypesUpdate>().param("name", &nt.name),
        events,
        response: dto::notetype(&nt),
    })
}

fn restore_version(
    req: RestoreNoteTypeVersion,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<NoteTypeDto>> {
    let current = require_notetype(ctx.reads(), req.notetype.id)?;
    let nt = from_snapshot(req.notetype);
    let mut snapshots = Vec::new();
    let mut notes = Vec::new();
    for s in req.notes {
        let note = require_note(ctx.reads(), s.id)?;
        snapshots.push(fields_snapshot(&note));
        notes.push((note, s.fields));
    }
    ctx.tx.upsert_notetype(&nt)?;
    for (mut note, fields) in notes {
        note.fields = field_values(&fields);
        write_note(ctx, &nt, &mut note)?;
    }
    sync_card_types(ctx, &nt, ctx.now)?;
    Ok(CommandOutcome {
        inverse: vec![inverse_call::<NotetypesRestoreVersion>(&RestoreNoteTypeVersion {
            notetype: snapshot(&current),
            notes: snapshots,
        })?],
        summary: summary::<NotetypesUpdate>().param("name", &nt.name),
        events: vec![changed(&nt)],
        response: dto::notetype(&nt),
    })
}

fn delete(req: NoteTypeRef, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<NoteTypeDto>> {
    let mut nt = require_notetype(ctx.reads(), req.id)?;
    if nt.id == classic_builtin::CLASSIC_ID {
        return Err(AppError::Api(ApiError::new(
            ErrorCode::ConflictNoteTypeRequired,
            "the Classic note type cannot be deleted",
        )));
    }
    if nt.deleted_at.is_some() {
        return Ok(CommandOutcome {
            inverse: Vec::new(),
            summary: summary::<NotetypesDelete>().param("name", &nt.name),
            events: Vec::new(),
            response: dto::notetype(&nt),
        });
    }
    let live = ctx.reads().note_ids_of_notetype(nt.id, false)?.len();
    if live > 0 {
        return Err(AppError::Api(
            ApiError::new(
                ErrorCode::ConflictNoteTypeInUse,
                format!("{live} notes use this note type"),
            )
            .with_detail("notes", live),
        ));
    }
    nt.deleted_at = Some(ctx.now);
    ctx.tx.upsert_notetype(&nt)?;
    Ok(CommandOutcome {
        inverse: vec![inverse_call::<NotetypesRestore>(&NoteTypeRef { id: nt.id })?],
        summary: summary::<NotetypesDelete>().param("name", &nt.name),
        events: vec![changed(&nt)],
        response: dto::notetype(&nt),
    })
}

fn restore(req: NoteTypeRef, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<NoteTypeDto>> {
    let mut nt = require_notetype(ctx.reads(), req.id)?;
    if nt.deleted_at.is_none() {
        return Ok(CommandOutcome {
            inverse: Vec::new(),
            summary: summary::<NotetypesRestore>().param("name", &nt.name),
            events: Vec::new(),
            response: dto::notetype(&nt),
        });
    }
    nt.deleted_at = None;
    ctx.tx.upsert_notetype(&nt)?;
    Ok(CommandOutcome {
        inverse: vec![inverse_call::<NotetypesDelete>(&NoteTypeRef { id: nt.id })?],
        summary: summary::<NotetypesRestore>().param("name", &nt.name),
        events: vec![changed(&nt)],
        response: dto::notetype(&nt),
    })
}

fn usage(req: NoteTypeRef, ctx: &QueryCtx<'_>) -> AppResult<NoteTypeUsage> {
    require_notetype(ctx.tx, req.id)?;
    let (notes, cards_by_template) = usage_counts(ctx.tx, req.id)?;
    Ok(NoteTypeUsage {
        notes,
        cards_by_template,
    })
}

fn list_card_types(_: Empty, ctx: &QueryCtx<'_>) -> AppResult<CardTypeList> {
    Ok(CardTypeList {
        card_types: ctx
            .registry
            .card_types()
            .into_iter()
            .map(|e| {
                let info = e.info();
                CardTypeDto {
                    id: info.id,
                    name_key: info.name_key,
                }
            })
            .collect(),
    })
}

fn set_generated(
    ids: &[CardId],
    trash: bool,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<AffectedCount> {
    let mut unique: Vec<CardId> = ids.to_vec();
    unique.sort();
    unique.dedup();
    for id in &unique {
        if ctx.reads().get_card(*id)?.is_none() {
            return Err(err_id(ErrorCode::NotFoundCard, format!("card {id} not found"), id));
        }
    }
    let at = trash.then_some(ctx.now);
    Ok(AffectedCount {
        decks: 0,
        notes: 0,
        cards: ctx.tx.set_cards_deleted(&unique, at)?,
    })
}

fn trash_generated(
    req: CardIdList,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<AffectedCount>> {
    let response = set_generated(&req.ids, true, ctx)?;
    Ok(CommandOutcome {
        inverse: vec![inverse_call::<CardsRestoreGenerated>(&req)?],
        summary: summary::<CardsTrashGenerated>().param("count", req.ids.len()),
        events: vec![DomainEvent::CardsUpdated { ids: req.ids }],
        response,
    })
}

fn restore_generated(
    req: CardIdList,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<AffectedCount>> {
    let response = set_generated(&req.ids, false, ctx)?;
    Ok(CommandOutcome {
        inverse: vec![inverse_call::<CardsTrashGenerated>(&req)?],
        summary: summary::<CardsRestoreGenerated>().param("count", req.ids.len()),
        events: vec![DomainEvent::CardsUpdated { ids: req.ids }],
        response,
    })
}

// ------------------------------------------------------------------ rendering

fn require_card(reads: &dyn fc_storage::Reads, id: CardId) -> AppResult<Card> {
    reads
        .get_card(id)?
        .ok_or_else(|| err_id(ErrorCode::NotFoundCard, format!("card {id} not found"), id))
}

fn render_card(req: CardRef, ctx: &QueryCtx<'_>) -> AppResult<RenderedCardDto> {
    let card = require_card(ctx.tx, req.id)?;
    let note = require_note(ctx.tx, card.note_id)?;
    let nt = require_notetype(ctx.tx, note.notetype_id)?;
    let sides = render(
        ctx.registry,
        ctx.tx,
        &nt,
        &note,
        &card.template_key,
        &RenderSetup {
            deck: Some(card.deck_id),
            seed: card_seed(card.id),
            locale: ctx.locale,
        },
    )?;
    Ok(RenderedCardDto {
        card_id: card.id,
        note_id: note.id,
        empty: is_empty_card(ctx.registry, &nt, &note, &card.template_key)?,
        sides,
        flags: card.flags,
        user_note: card.user_note,
    })
}

fn preview(req: PreviewCards, ctx: &QueryCtx<'_>) -> AppResult<CardPreviewList> {
    let base = match req.notetype_id {
        Some(id) => require_notetype(ctx.tx, id)?,
        None => minimal("", &NoteType {
            id: NoteTypeId(uuid::Uuid::nil()),
            name: String::new(),
            kind: NoteTypeKind::Standard,
            fields: Vec::new(),
            templates: Vec::new(),
            css: String::new(),
            sort_field: FieldKey::new(""),
            version: 1,
            builtin: None,
            created_at: ctx.now,
            updated_at: ctx.now,
            deleted_at: None,
        }),
    };
    let nt = match req.definition {
        Some(def) => from_input(def, &base),
        None if req.notetype_id.is_some() => base,
        None => return Err(invalid_nt("preview needs a note type or a definition")),
    };
    validate_full(ctx.registry, &nt)?;
    if let Some(d) = req.deck_id {
        DeckIndex::load(ctx.tx)?.require(d)?;
    }
    // Unknown keys are ignored here (the editor may be mid-change); required-ness is not
    // enforced so empty drafts preview too.
    let known: BTreeMap<String, String> = req
        .fields
        .into_iter()
        .filter(|(k, _)| nt.field(&FieldKey::new(k.clone())).is_some())
        .collect();
    let mut lenient = nt.clone();
    for f in &mut lenient.fields {
        f.required = false;
    }
    let fields = normalize_note_fields(&lenient, &field_values(&known))?;
    let mut note = Note {
        id: NoteId(uuid::Uuid::nil()),
        notetype_id: nt.id,
        home_deck_id: req.deck_id.unwrap_or(fc_core::DeckId(uuid::Uuid::nil())),
        fields,
        sort_field: String::new(),
        checksum: String::new(),
        tags: parse_tags(&req.tags)?,
        created_at: ctx.now,
        updated_at: ctx.now,
        deleted_at: None,
    };
    crate::support::notes::derive(&nt, &mut note);
    let seed = fnv1a64(note.checksum.as_bytes());
    let mut cards = Vec::new();
    for spec in card_specs(ctx.registry, &nt, &note)? {
        cards.push(render(
            ctx.registry,
            ctx.tx,
            &nt,
            &note,
            &spec.template_key,
            &RenderSetup {
                deck: req.deck_id,
                seed,
                locale: ctx.locale,
            },
        )?);
    }
    Ok(CardPreviewList { cards })
}

fn cards_meta(req: CardsOfNote, ctx: &QueryCtx<'_>) -> AppResult<CardMetaList> {
    let note = require_note(ctx.tx, req.note_id)?;
    let nt = require_notetype(ctx.tx, note.notetype_id)?;
    let keys: BTreeSet<String> = card_specs(ctx.registry, &nt, &note)?
        .into_iter()
        .map(|s| s.template_key)
        .collect();
    let items = dto::note_cards(ctx.tx, &note)?
        .into_iter()
        .map(|c| {
            let empty = !keys.contains(&c.template_key);
            card_meta(&nt, c, empty)
        })
        .collect();
    Ok(CardMetaList { items })
}

/// Card → meta DTO (template name resolved through the note type).
pub(crate) fn card_meta(nt: &NoteType, c: Card, empty: bool) -> CardMetaDto {
    let template_name = match template_for(nt, &c.template_key) {
        Some(t) if t.key == c.template_key => t.name.clone(),
        _ => c.template_key.clone(),
    };
    CardMetaDto {
        id: c.id,
        template_key: c.template_key,
        template_name,
        card_type: c.card_type,
        deck_id: c.deck_id,
        flags: c.flags,
        user_note: c.user_note,
        empty,
    }
}
