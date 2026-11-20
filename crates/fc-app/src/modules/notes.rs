//! `notes` module: note CRUD and listing (docs/02 §2). A standard note type generates one card
//! per template (`fc_core::card::standard_card_specs`); Classic yields one `forward` card.

use crate::error::AppResult;
use crate::handler::{inverse_call, CommandCtx, CommandOutcome, QueryCtx};
use crate::module::{main_menu, ModuleDescriptor};
use crate::registry::Registry;
use crate::support::attrs::apply_assignments;
use fc_api::v1::attributes::AttributeAssignment;
use crate::support::content::{add_missing_cards, card_specs, deck_for_new_cards};
use crate::support::decks::DeckIndex;
use crate::support::notes::{
    derive, field_values, insert_note, parse_tags, require_live_note, require_note,
    require_notetype, write_note,
};
use crate::support::{deleted, dto, err_id, summary};
use fc_api::v1::cardtypes::CardIdList;
use fc_api::v1::common::AffectedCount;
use fc_api::v1::events::DomainEvent;
use fc_api::v1::methods::*;
use fc_api::v1::notes::*;
use fc_api::v1::oplog::OplogKind;
use fc_api::{ApiError, ErrorCode};
use fc_core::attribute::{
    effective_attribute_defs, prepare_attribute_values, AttributeTarget, EntityRef,
};
use fc_core::card::Card;
use fc_core::error::CoreError;
use fc_core::note::{normalize_note_fields, Note};
use fc_core::{CardId, NoteId};
use fc_storage::repo::{CardFilter, NoteFilter};

/// Module id.
pub const ID: &str = "notes";

/// Descriptor.
pub fn descriptor() -> ModuleDescriptor {
    ModuleDescriptor {
        id: ID,
        name_key: "modules.notes.name",
        description_key: "modules.notes.description",
        enabled_by_default: true,
        can_disable: false,
        settings: vec![],
        ui: vec![
            main_menu("notes.add", "nav.add_note", "plus", "/notes/new", 20),
            main_menu("notes.list", "nav.notes", "list", "/notes", 30),
        ],
    }
}

/// Registers the module and its methods.
pub fn register(reg: &mut Registry) {
    reg.module(descriptor());
    reg.command::<NotesCreate>(create);
    reg.command::<NotesUpdate>(update);
    reg.command::<NotesDelete>(delete);
    reg.command::<NotesRestore>(restore);
    reg.query::<NotesGet>(get);
    reg.query::<NotesList>(list);
}

fn unique<T: PartialEq + Copy>(ids: &[T]) -> Vec<T> {
    let mut out = Vec::new();
    for id in ids {
        if !out.contains(id) {
            out.push(*id);
        }
    }
    out
}

fn create(req: CreateNote, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<NoteDto>> {
    let nt = require_notetype(ctx.reads(), req.notetype_id)?;
    if nt.deleted_at.is_some() {
        return Err(deleted("notetype", nt.id));
    }
    let index = DeckIndex::load(ctx.reads())?;
    index.require_live(req.deck_id)?;
    let mut note = Note {
        id: NoteId::generate(ctx.ids),
        notetype_id: nt.id,
        home_deck_id: req.deck_id,
        fields: normalize_note_fields(&nt, &field_values(&req.fields))?,
        sort_field: String::new(),
        checksum: String::new(),
        tags: parse_tags(&req.tags)?,
        created_at: ctx.now,
        updated_at: ctx.now,
        deleted_at: None,
    };
    derive(&nt, &mut note);
    // Cards come from the card type engines of the note type's templates (v0.3).
    let specs = card_specs(ctx.registry, &nt, &note)?;
    if specs.is_empty() {
        return Err(CoreError::InvalidNoteType {
            reason: "note generates no cards".into(),
        }
        .into());
    }
    if !req.allow_duplicate.unwrap_or(false) {
        if let Some(existing) = ctx
            .reads()
            .find_notes_by_checksum(nt.id, &note.checksum)?
            .first()
        {
            return Err(ApiError::new(
                ErrorCode::ConflictNoteDuplicate,
                format!("a note like {:?} already exists", note.sort_field),
            )
            .with_detail("id", existing.to_string())
            .into());
        }
    }
    let ancestry = index.ancestry(req.deck_id)?;
    let defs = ctx.reads().list_attribute_defs(false)?;
    let effective = effective_attribute_defs(&ancestry, &defs);
    let values = if attributes_enabled(ctx)? {
        let provided: Vec<_> = req
            .attributes
            .iter()
            .map(|a| (a.attr_id, a.value.clone()))
            .collect();
        prepare_attribute_values(&effective, AttributeTarget::Note, &provided)?
    } else {
        // Module off: classifications are hidden, so nothing is asked of the caller —
        // given values are ignored, defaults still apply, `required` is not enforced.
        default_attribute_values(&effective, AttributeTarget::Note)
    };

    insert_note(ctx, &nt, &note)?;
    for spec in specs {
        let card = Card::new(
            CardId::generate(ctx.ids),
            note.id,
            req.deck_id,
            spec,
            ctx.now,
        );
        ctx.tx.insert_card(&card)?;
    }
    let values: Vec<_> = values.into_iter().map(|(id, v)| (id, Some(v))).collect();
    ctx.tx
        .set_attribute_values(EntityRef::Note(note.id), &values)?;
    let response = dto::note(ctx.reads(), &note)?;
    Ok(CommandOutcome {
        inverse: vec![inverse_call::<NotesDelete>(&NoteIds {
            ids: vec![note.id],
        })?],
        summary: summary::<NotesCreate>().param("text", &note.sort_field),
        events: vec![DomainEvent::NoteCreated {
            note_id: note.id,
            deck_id: req.deck_id,
        }],
        response,
    })
}

/// Whether the `attributes` module is on (in this transaction).
pub(crate) fn attributes_enabled(ctx: &CommandCtx<'_>) -> AppResult<bool> {
    crate::support::module_enabled(ctx.reads(), ctx.registry, crate::modules::attributes::ID)
}

/// Values a new entity gets when classifications are hidden (module off): each applicable
/// definition's valid default, nothing else.
pub(crate) fn default_attribute_values(
    effective: &[fc_core::attribute::EffectiveAttribute<'_>],
    target: AttributeTarget,
) -> Vec<(fc_core::AttributeDefId, fc_core::attribute::AttributeValue)> {
    effective
        .iter()
        .map(|e| e.def)
        .filter(|d| d.target == target)
        .filter_map(|d| {
            let v = d.default_value.as_ref()?;
            fc_core::attribute::validate_attribute_value(d, v)
                .ok()
                .map(|v| (d.id, v))
        })
        .collect()
}

/// Replaces fields and tags; optionally moves the home deck (cards stay put); then sets/clears
/// the listed attributes (others are untouched), resolved against the new home deck.
fn update(req: UpdateNote, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<NoteDto>> {
    let mut note = require_live_note(ctx.reads(), req.id)?;
    let nt = require_notetype(ctx.reads(), note.notetype_id)?;
    let old = note.clone();
    let strict = ctx.batch_kind == OplogKind::Do;
    if let Some(d) = req.deck_id {
        let index = DeckIndex::load(ctx.reads())?;
        if strict {
            index.require_live(d)?;
        } else {
            // Undo/redo may move a note back to a deck that has since been trashed.
            index.require(d)?;
        }
        note.home_deck_id = d;
    }
    note.fields = normalize_note_fields(&nt, &field_values(&req.fields))?;
    note.tags = parse_tags(&req.tags)?;
    note.updated_at = ctx.now;
    write_note(ctx, &nt, &mut note)?;
    // New content may call for new cards (a new cloze number, a filled optional reverse);
    // existing cards are never removed (v0.3).
    let deck = deck_for_new_cards(ctx.reads(), &note)?;
    let new_cards = add_missing_cards(ctx, &nt, &note, deck)?;
    // With the module off, a new edit leaves classifications alone (they are hidden);
    // undo/redo still restore them exactly.
    let assignments: &[AttributeAssignment] = if strict && !attributes_enabled(ctx)? {
        &[]
    } else {
        &req.attributes
    };
    let previous = apply_assignments(ctx.tx, strict, EntityRef::Note(note.id), assignments)?;
    let inverse = UpdateNote {
        id: note.id,
        fields: old
            .fields
            .iter()
            .map(|(k, v)| (k.0.clone(), v.clone()))
            .collect(),
        tags: old.tags.iter().map(|t| t.as_str().to_owned()).collect(),
        attributes: previous,
        deck_id: (old.home_deck_id != note.home_deck_id).then_some(old.home_deck_id),
    };
    let response = dto::note(ctx.reads(), &note)?;
    let mut inverse = vec![inverse_call::<NotesUpdate>(&inverse)?];
    if !new_cards.is_empty() {
        inverse.push(inverse_call::<CardsTrashGenerated>(&CardIdList { ids: new_cards })?);
    }
    Ok(CommandOutcome {
        inverse,
        summary: summary::<NotesUpdate>().param("text", &note.sort_field),
        events: vec![DomainEvent::NoteUpdated { note_id: note.id }],
        response,
    })
}

fn delete(req: NoteIds, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<AffectedCount>> {
    let mut ids = Vec::new();
    for id in unique(&req.ids) {
        if require_note(ctx.reads(), id)?.deleted_at.is_none() {
            ids.push(id);
        }
    }
    let cards: Vec<CardId> = ctx
        .reads()
        .list_cards(&CardFilter {
            deck_ids: None,
            note_ids: Some(ids.clone()),
            include_deleted: false,
        })?
        .iter()
        .map(|c| c.id)
        .collect();
    let affected = AffectedCount {
        decks: 0,
        notes: ctx.tx.set_notes_deleted(&ids, Some(ctx.now))?,
        cards: ctx.tx.set_cards_deleted(&cards, Some(ctx.now))?,
    };
    let inverse = if ids.is_empty() {
        Vec::new()
    } else {
        vec![inverse_call::<NotesRestore>(&NoteIds { ids: ids.clone() })?]
    };
    Ok(CommandOutcome {
        response: affected,
        inverse,
        summary: summary::<NotesDelete>().param("count", ids.len()),
        events: if ids.is_empty() {
            Vec::new()
        } else {
            vec![DomainEvent::NotesDeleted { ids }]
        },
    })
}

/// Restores trashed notes with the cards trashed together with them. A card whose deck is in
/// the trash blocks the restore (`CONFLICT_DELETED`): restore the deck instead.
fn restore(req: NoteIds, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<AffectedCount>> {
    let index = DeckIndex::load(ctx.reads())?;
    let mut ids = Vec::new();
    let mut cards = Vec::new();
    for id in unique(&req.ids) {
        let note = require_note(ctx.reads(), id)?;
        let Some(at) = note.deleted_at else {
            continue;
        };
        for c in ctx.reads().list_cards(&CardFilter {
            deck_ids: None,
            note_ids: Some(vec![id]),
            include_deleted: true,
        })? {
            if c.deleted_at == Some(at) {
                if index.get(c.deck_id).is_none_or(|d| d.is_deleted()) {
                    return Err(deleted("deck", c.deck_id));
                }
                cards.push(c.id);
            }
        }
        ids.push(id);
    }
    let affected = AffectedCount {
        decks: 0,
        notes: ctx.tx.set_notes_deleted(&ids, None)?,
        cards: ctx.tx.set_cards_deleted(&cards, None)?,
    };
    let inverse = if ids.is_empty() {
        Vec::new()
    } else {
        vec![inverse_call::<NotesDelete>(&NoteIds { ids: ids.clone() })?]
    };
    Ok(CommandOutcome {
        response: affected,
        inverse,
        summary: summary::<NotesRestore>().param("count", ids.len()),
        events: if ids.is_empty() {
            Vec::new()
        } else {
            vec![DomainEvent::NotesRestored { ids }]
        },
    })
}

fn get(req: NoteRef, ctx: &QueryCtx<'_>) -> AppResult<NoteDto> {
    dto::note(ctx.tx, &require_note(ctx.tx, req.id)?)
}

fn list(req: ListNotes, ctx: &QueryCtx<'_>) -> AppResult<NotePage> {
    let deck_ids = match req.deck_id {
        None => None,
        Some(d) => {
            let index = DeckIndex::load(ctx.tx)?;
            index
                .require(d)
                .map_err(|_| err_id(ErrorCode::NotFoundDeck, format!("deck {d} not found"), d))?;
            Some(if req.include_subdecks.unwrap_or(true) {
                index.live_subtree(d)
            } else {
                vec![d]
            })
        }
    };
    let page = ctx.tx.list_notes(
        &NoteFilter {
            deck_ids,
            include_deleted: false,
        },
        &req.page,
    )?;
    Ok(NotePage {
        items: page
            .items
            .into_iter()
            .map(|row| NoteListItem {
                id: row.note.id,
                notetype_id: row.note.notetype_id,
                sort_field: row.note.sort_field,
                tags: row.note.tags,
                card_count: row.card_count,
                updated_at: row.note.updated_at,
            })
            .collect(),
        next_cursor: page.next_cursor,
    })
}
