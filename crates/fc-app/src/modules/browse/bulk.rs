//! `bulk.*`: one command per operation over a selection. Each records **one** oplog entry
//! whose inverse is an exact snapshot write-back (`bulk.restore_cards` / `restore_notes` /
//! `restore_values`), so a 1000-card change is one undo step and comes back verbatim.

use crate::error::AppResult;
use crate::handler::{inverse_call, CommandCtx, CommandOutcome};
use crate::support::attrs::{effective_for_deck, require_def, require_live_def};
use crate::support::decks::DeckIndex;
use crate::support::notes::{parse_tags, require_note, require_notetype, write_note};
use crate::support::{err_id, invalid, summary};
use fc_api::v1::bulk::*;
use fc_api::v1::events::DomainEvent;
use fc_api::v1::methods::*;
use fc_api::{ApiError, ApiMethod, ErrorCode};
use fc_core::attribute::{
    validate_attribute_value, AttributeTarget, AttributeValue, EntityRef,
};
use fc_core::card::{Card, SchedulingState};
use fc_core::error::CoreError;
use fc_core::note::Note;
use fc_core::notetype::FieldKey;
use fc_core::query::text::replace_text;
use fc_core::tag::dedup_tags;
use fc_core::time::MILLIS_PER_DAY;
use fc_core::{CardId, DeckId, NoteId, Timestamp};
use std::collections::{HashMap, HashSet};

fn unique<T: Copy + Eq + std::hash::Hash>(ids: &[T]) -> Vec<T> {
    let mut seen = HashSet::new();
    ids.iter().copied().filter(|i| seen.insert(*i)).collect()
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// Every mutable column of `c`.
pub(super) fn snapshot(c: &Card) -> CardSnapshot {
    let s = &c.scheduling;
    CardSnapshot {
        id: c.id,
        deck_id: c.deck_id,
        state: s.state,
        due: s.due,
        stability: s.stability,
        difficulty: s.difficulty,
        reps: s.reps,
        lapses: s.lapses,
        last_review: s.last_review,
        flags: c.flags,
        suspended: c.suspended,
        buried_until: c.buried_until,
        deleted_at: c.deleted_at,
    }
}

fn apply_snapshot(c: &mut Card, s: &CardSnapshot) {
    c.deck_id = s.deck_id;
    c.scheduling = SchedulingState {
        state: s.state,
        due: s.due,
        stability: s.stability,
        difficulty: s.difficulty,
        reps: s.reps,
        lapses: s.lapses,
        last_review: s.last_review,
    };
    c.flags = s.flags;
    c.suspended = s.suspended;
    c.buried_until = s.buried_until;
    c.deleted_at = s.deleted_at;
}

fn require_card(ctx: &CommandCtx<'_>, id: CardId) -> AppResult<Card> {
    ctx.reads()
        .get_card(id)?
        .ok_or_else(|| err_id(ErrorCode::NotFoundCard, format!("card {id} not found"), id))
}

/// Applies `f` to every live card of the selection; changed cards get their previous state
/// recorded for the single inverse.
fn card_op<M: ApiMethod>(
    ctx: &mut CommandCtx<'_>,
    ids: &[CardId],
    f: impl Fn(&mut Card),
) -> AppResult<CommandOutcome<BulkResult>> {
    let mut before = Vec::new();
    let mut changed = Vec::new();
    let mut skipped = 0;
    for id in unique(ids) {
        let mut card = require_card(ctx, id)?;
        if card.deleted_at.is_some() {
            skipped += 1;
            continue;
        }
        let old = card.clone();
        f(&mut card);
        if card == old {
            skipped += 1;
            continue;
        }
        card.updated_at = ctx.now;
        ctx.tx.update_card(&card)?;
        before.push(snapshot(&old));
        changed.push(id);
    }
    cards_outcome::<M>(before, changed, skipped)
}

fn cards_outcome<M: ApiMethod>(
    before: Vec<CardSnapshot>,
    changed: Vec<CardId>,
    skipped: u32,
) -> AppResult<CommandOutcome<BulkResult>> {
    let n = changed.len();
    Ok(CommandOutcome {
        response: BulkResult {
            changed: count(n),
            skipped,
        },
        inverse: if before.is_empty() {
            Vec::new()
        } else {
            vec![inverse_call::<BulkRestoreCards>(&RestoreCards { cards: before })?]
        },
        summary: summary::<M>().param("count", n),
        events: if changed.is_empty() {
            Vec::new()
        } else {
            vec![DomainEvent::CardsChanged { ids: changed }]
        },
    })
}

pub(super) fn move_cards(
    req: MoveCards,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<BulkResult>> {
    DeckIndex::load(ctx.reads())?.require_live(req.deck_id)?;
    let deck: DeckId = req.deck_id;
    card_op::<BulkMoveCards>(ctx, &req.card_ids, |c| c.deck_id = deck)
}

pub(super) fn set_suspended(
    req: SetSuspended,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<BulkResult>> {
    card_op::<BulkSetSuspended>(ctx, &req.card_ids, |c| c.suspended = req.suspended)
}

/// Start of the next UTC day.
fn next_midnight(now: Timestamp) -> Timestamp {
    Timestamp((now.0.div_euclid(MILLIS_PER_DAY) + 1) * MILLIS_PER_DAY)
}

pub(super) fn set_buried(
    req: SetBuried,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<BulkResult>> {
    let now = ctx.now;
    let until = next_midnight(now);
    card_op::<BulkSetBuried>(ctx, &req.card_ids, |c| {
        if req.buried {
            c.buried_until = Some(until);
        } else if c.buried_until.is_some_and(|b| b > now) {
            c.buried_until = None;
        }
    })
}

pub(super) fn set_flag(req: SetFlag, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<BulkResult>> {
    let max = fc_core::query::resolve::MAX_FLAG;
    let mask = match req.flag {
        0 if !req.on => u32::MAX,
        n if (1..=max).contains(&n) => 1u32 << (n - 1),
        n => return Err(invalid(format!("flag must be 1..={max} (0 only to clear), got {n}"))),
    };
    card_op::<BulkSetFlag>(ctx, &req.card_ids, |c| {
        if req.on {
            c.flags |= mask;
        } else {
            c.flags &= !mask;
        }
    })
}

pub(super) fn reset_progress(
    req: CardSelection,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<BulkResult>> {
    card_op::<BulkResetProgress>(ctx, &req.card_ids, |c| {
        c.scheduling = SchedulingState::NEW;
    })
}

pub(super) fn restore_cards(
    req: RestoreCards,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<BulkResult>> {
    let index = DeckIndex::load(ctx.reads())?;
    let mut before = Vec::new();
    let mut changed = Vec::new();
    for s in &req.cards {
        index.require(s.deck_id)?;
        let mut card = require_card(ctx, s.id)?;
        before.push(snapshot(&card));
        apply_snapshot(&mut card, s);
        card.updated_at = ctx.now;
        ctx.tx.update_card(&card)?;
        changed.push(s.id);
    }
    cards_outcome::<BulkRestoreCards>(before, changed, 0)
}

// ------------------------------------------------------------------ notes

fn note_snapshot(n: &Note) -> NoteSnapshot {
    NoteSnapshot {
        id: n.id,
        fields: n.fields.iter().map(|(k, v)| (k.0.clone(), v.clone())).collect(),
        tags: n.tags.iter().map(|t| t.as_str().to_owned()).collect(),
    }
}

/// Applies `f` to every live note of the selection; notes whose fields and tags stay equal
/// are skipped.
fn note_op<M: ApiMethod>(
    ctx: &mut CommandCtx<'_>,
    ids: &[NoteId],
    f: impl Fn(&mut Note) -> AppResult<()>,
) -> AppResult<CommandOutcome<BulkResult>> {
    let mut before = Vec::new();
    let mut changed = Vec::new();
    let mut skipped = 0;
    for id in unique(ids) {
        let mut note = require_note(ctx.reads(), id)?;
        if note.deleted_at.is_some() {
            skipped += 1;
            continue;
        }
        let old = note.clone();
        f(&mut note)?;
        if note.fields == old.fields && note.tags == old.tags {
            skipped += 1;
            continue;
        }
        let nt = require_notetype(ctx.reads(), note.notetype_id)?;
        note.updated_at = ctx.now;
        write_note(ctx, &nt, &mut note)?;
        before.push(note_snapshot(&old));
        changed.push(id);
    }
    notes_outcome::<M>(before, changed, skipped)
}

fn notes_outcome<M: ApiMethod>(
    before: Vec<NoteSnapshot>,
    changed: Vec<NoteId>,
    skipped: u32,
) -> AppResult<CommandOutcome<BulkResult>> {
    let n = changed.len();
    Ok(CommandOutcome {
        response: BulkResult {
            changed: count(n),
            skipped,
        },
        inverse: if before.is_empty() {
            Vec::new()
        } else {
            vec![inverse_call::<BulkRestoreNotes>(&RestoreNotes { notes: before })?]
        },
        summary: summary::<M>().param("count", n),
        events: changed
            .into_iter()
            .map(|note_id| DomainEvent::NoteUpdated { note_id })
            .collect(),
    })
}

pub(super) fn add_tags(req: NoteTags, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<BulkResult>> {
    let add = parse_tags(&req.tags)?;
    note_op::<BulkAddTags>(ctx, &req.note_ids, |n| {
        let mut tags = n.tags.clone();
        tags.extend(add.iter().cloned());
        n.tags = dedup_tags(tags);
        Ok(())
    })
}

pub(super) fn remove_tags(
    req: NoteTags,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<BulkResult>> {
    let remove = parse_tags(&req.tags)?;
    note_op::<BulkRemoveTags>(ctx, &req.note_ids, |n| {
        n.tags.retain(|t| !remove.iter().any(|r| t.is_within(r)));
        Ok(())
    })
}

pub(super) fn find_replace(
    req: FindReplace,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<BulkResult>> {
    let regex = req.regex.unwrap_or(false);
    let case = req.match_case.unwrap_or(false);
    // Validate once up front so a bad pattern fails even on an empty selection.
    replace_text("", &req.find, &req.replace, regex, case).map_err(|reason| {
        ApiError::new(ErrorCode::ValidationFindReplace, reason.clone()).with_detail("reason", reason)
    })?;
    let field = req.field.clone().map(FieldKey::new);
    note_op::<BulkFindReplace>(ctx, &req.note_ids, |n| {
        for (k, v) in n.fields.iter_mut() {
            if field.as_ref().is_none_or(|f| f == k) {
                *v = replace_text(v, &req.find, &req.replace, regex, case)
                    .map_err(invalid)?;
            }
        }
        Ok(())
    })
}

pub(super) fn restore_notes(
    req: RestoreNotes,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<BulkResult>> {
    let mut before = Vec::new();
    let mut changed = Vec::new();
    for s in &req.notes {
        let mut note = require_note(ctx.reads(), s.id)?;
        let nt = require_notetype(ctx.reads(), note.notetype_id)?;
        before.push(note_snapshot(&note));
        note.fields = s
            .fields
            .iter()
            .map(|(k, v)| (FieldKey::new(k.clone()), v.clone()))
            .collect();
        note.tags = parse_tags(&s.tags)?;
        note.updated_at = ctx.now;
        write_note(ctx, &nt, &mut note)?;
        changed.push(s.id);
    }
    notes_outcome::<BulkRestoreNotes>(before, changed, 0)
}

// ------------------------------------------------------------------ classifications

/// Sets/clears one classification on the selection's notes (note-level) or cards
/// (card-level). Entities whose deck does not inherit the definition, and trashed ones, are
/// skipped; clearing a required classification fails.
pub(super) fn set_attribute(
    req: SetAttributeBulk,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<BulkResult>> {
    let def = require_live_def(ctx.reads(), req.attr_id)?;
    let value = match &req.value {
        Some(v) => Some(validate_attribute_value(&def, v)?),
        None if def.required => return Err(CoreError::AttributeRequired { key: def.key }.into()),
        None => None,
    };
    let index = DeckIndex::load(ctx.reads())?;
    // Which entities, and the deck their definitions resolve from.
    let mut entities: Vec<(EntityRef, DeckId)> = Vec::new();
    let mut skipped = 0u32;
    let mut seen = HashSet::new();
    for id in unique(&req.card_ids) {
        let card = require_card(ctx, id)?;
        if card.deleted_at.is_some() {
            skipped += 1;
            continue;
        }
        match def.target {
            AttributeTarget::Card => entities.push((EntityRef::Card(card.id), card.deck_id)),
            AttributeTarget::Note => {
                if seen.insert(card.note_id) {
                    let note = require_note(ctx.reads(), card.note_id)?;
                    if note.deleted_at.is_some() {
                        skipped += 1;
                    } else {
                        entities.push((EntityRef::Note(note.id), note.home_deck_id));
                    }
                }
            }
        }
    }
    let mut applies: HashMap<DeckId, bool> = HashMap::new();
    let mut previous = Vec::new();
    let mut changed = Vec::new();
    for (entity, deck) in entities {
        let ok = match applies.get(&deck) {
            Some(ok) => *ok,
            None => {
                let ok = effective_for_deck(ctx.reads(), &index, Some(deck))?
                    .iter()
                    .any(|(d, _)| d.id == def.id);
                applies.insert(deck, ok);
                ok
            }
        };
        if !ok {
            skipped += 1;
            continue;
        }
        let current: Option<AttributeValue> = ctx
            .reads()
            .get_attribute_values(entity)?
            .into_iter()
            .find(|(a, _)| *a == def.id)
            .map(|(_, v)| v);
        if current == value {
            skipped += 1;
            continue;
        }
        ctx.tx.set_attribute_values(entity, &[(def.id, value.clone())])?;
        previous.push(EntityValue {
            entity,
            value: current,
        });
        changed.push(entity);
    }
    values_outcome::<BulkSetAttribute>(def.id, previous, changed, skipped, &def.name)
}

fn values_outcome<M: ApiMethod>(
    attr_id: fc_core::AttributeDefId,
    previous: Vec<EntityValue>,
    changed: Vec<EntityRef>,
    skipped: u32,
    name: &str,
) -> AppResult<CommandOutcome<BulkResult>> {
    let n = changed.len();
    Ok(CommandOutcome {
        response: BulkResult {
            changed: count(n),
            skipped,
        },
        inverse: if previous.is_empty() {
            Vec::new()
        } else {
            vec![inverse_call::<BulkRestoreValues>(&RestoreValues {
                attr_id,
                values: previous,
            })?]
        },
        summary: summary::<M>().param("count", n).param("name", name),
        events: if changed.is_empty() {
            Vec::new()
        } else {
            vec![DomainEvent::AttributeValuesChanged { entities: changed }]
        },
    })
}

pub(super) fn restore_values(
    req: RestoreValues,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<BulkResult>> {
    let def = require_def(ctx.reads(), req.attr_id)?;
    let mut previous = Vec::new();
    let mut changed = Vec::new();
    for ev in &req.values {
        let value = match &ev.value {
            Some(v) => Some(validate_attribute_value(&def, v)?),
            None => None,
        };
        let current = ctx
            .reads()
            .get_attribute_values(ev.entity)?
            .into_iter()
            .find(|(a, _)| *a == def.id)
            .map(|(_, v)| v);
        ctx.tx.set_attribute_values(ev.entity, &[(def.id, value)])?;
        previous.push(EntityValue {
            entity: ev.entity,
            value: current,
        });
        changed.push(ev.entity);
    }
    values_outcome::<BulkRestoreValues>(def.id, previous, changed, 0, &def.name)
}
