//! `bulk.change_notetype` (docs/02 §10 "not türünü değiştir"; deferred by v0.4 until v0.3's
//! card type engines were merged). One command, one undo step:
//!
//! * fields are copied by the field map (target key ← source key);
//! * each live card whose template the template map carries over keeps its row — id, deck,
//!   scheduling, review history, flags — and gets the target template's key and engine
//!   (a cloze/occlusion key `c<n>`/`m<n>` is kept when the target template yields it);
//! * other live cards go to the trash; cards the target type yields but the note lacks are
//!   generated (`support::content::add_missing_cards`).
//!
//! The inverse is `bulk.restore_notes` (fields, tags **and note type**) plus
//! `bulk.restore_cards` (template key and engine included; generated cards are trashed).

use super::bulk::{note_snapshot, snapshot};
use crate::error::AppResult;
use crate::handler::{inverse_call, CommandCtx, CommandOutcome};
use crate::support::content::{add_missing_cards, deck_for_new_cards, engine, strategy_error, template_for};
use crate::support::notes::{require_note, require_notetype, write_note};
use crate::support::{invalid, summary};
use fc_api::v1::bulk::*;
use fc_api::v1::events::DomainEvent;
use fc_api::v1::methods::{BulkChangeNotetype, BulkRestoreCards, BulkRestoreNotes};
use fc_core::note::{normalize_note_fields, FieldValues};
use fc_core::notetype::{FieldKey, NoteType};
use fc_core::{CardId, NoteId};
use fc_storage::repo::CardFilter;
use std::collections::{BTreeMap, HashSet};

type KeyMap = BTreeMap<String, Option<String>>;

/// Default mapping `target key → source key`: equal keys first, then the remaining target
/// keys take the remaining source keys in order.
fn default_map(target: &[String], source: &[String]) -> KeyMap {
    let mut out = KeyMap::new();
    let mut free: Vec<&String> = source.iter().filter(|s| !target.contains(s)).collect();
    free.reverse();
    for t in target {
        let from = if source.contains(t) {
            Some(t.clone())
        } else {
            free.pop().cloned()
        };
        out.insert(t.clone(), from);
    }
    out
}

/// Checks a client map: keys are target keys, values source keys (each at most once when
/// `unique`). Unlisted target keys map to nothing.
fn checked_map(
    given: KeyMap,
    target: &[String],
    source: &[String],
    what: &str,
    unique: bool,
) -> AppResult<KeyMap> {
    let mut used = HashSet::new();
    for (t, s) in &given {
        if !target.contains(t) {
            return Err(invalid(format!("{what} map: {t} is not in the target note type")));
        }
        if let Some(s) = s {
            if !source.contains(s) {
                return Err(invalid(format!("{what} map: {s} is not in the source note type")));
            }
            if unique && !used.insert(s.clone()) {
                return Err(invalid(format!("{what} map: {s} is mapped twice")));
            }
        }
    }
    Ok(target
        .iter()
        .map(|t| (t.clone(), given.get(t).cloned().flatten()))
        .collect())
}

fn field_keys(nt: &NoteType) -> Vec<String> {
    nt.fields.iter().map(|f| f.key.0.clone()).collect()
}

fn template_keys(nt: &NoteType) -> Vec<String> {
    nt.templates.iter().map(|t| t.key.clone()).collect()
}

pub(super) fn change_notetype(
    req: ChangeNoteType,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<ChangeNoteTypeResult>> {
    let target = require_notetype(ctx.reads(), req.notetype_id)?;
    if target.deleted_at.is_some() {
        return Err(invalid("the target note type is deleted"));
    }
    // Live notes of the selection, which must share one source note type.
    let mut notes = Vec::new();
    let mut skipped = 0u32;
    let mut seen = HashSet::new();
    for id in req.note_ids.iter().copied().filter(|i| seen.insert(*i)) {
        let n = require_note(ctx.reads(), id)?;
        if n.deleted_at.is_some() {
            skipped += 1;
        } else {
            notes.push(n);
        }
    }
    let Some(first) = notes.first() else {
        let empty = ChangeNoteTypeResult {
            skipped,
            ..ChangeNoteTypeResult::default()
        };
        return outcome(empty, Vec::new(), Vec::new(), Vec::new(), Vec::new());
    };
    let source_id = first.notetype_id;
    if notes.iter().any(|n| n.notetype_id != source_id) {
        return Err(invalid("the notes have different note types"));
    }
    if source_id == target.id {
        return Err(invalid("the notes already have this note type"));
    }
    let source = require_notetype(ctx.reads(), source_id)?;
    let (tf, sf) = (field_keys(&target), field_keys(&source));
    let fields = match req.field_map {
        Some(m) => checked_map(m, &tf, &sf, "field", false)?,
        None => default_map(&tf, &sf),
    };
    let (tt, st) = (template_keys(&target), template_keys(&source));
    let templates = match req.template_map {
        Some(m) => checked_map(m, &tt, &st, "template", true)?,
        None => default_map(&tt, &st),
    };
    // Source template key → target template.
    let carried: BTreeMap<&str, &str> = templates
        .iter()
        .filter_map(|(t, s)| s.as_deref().map(|s| (s, t.as_str())))
        .collect();

    let mut result = ChangeNoteTypeResult {
        skipped,
        ..ChangeNoteTypeResult::default()
    };
    let mut note_before = Vec::new();
    let mut card_before = Vec::new();
    let mut changed_notes: Vec<NoteId> = Vec::new();
    let mut changed_cards: Vec<CardId> = Vec::new();
    for old in notes {
        let mapped: FieldValues = fields
            .iter()
            .map(|(t, s)| {
                let v = s
                    .as_ref()
                    .and_then(|s| old.fields.get(&FieldKey::new(s.clone())))
                    .cloned()
                    .unwrap_or_default();
                (FieldKey::new(t.clone()), v)
            })
            .collect();
        let mut note = old.clone();
        note.notetype_id = target.id;
        note.fields = normalize_note_fields(&target, &mapped)?;
        note.updated_at = ctx.now;
        write_note(ctx, &target, &mut note)?;
        note_before.push(note_snapshot(&old));
        changed_notes.push(note.id);

        // Card keys each target template yields for the moved note.
        let mut yields: BTreeMap<&str, Vec<String>> = BTreeMap::new();
        for t in &target.templates {
            let specs = engine(ctx.registry, &t.card_type)?
                .generate_cards(&note, &target, t)
                .map_err(strategy_error)?;
            yields.insert(t.key.as_str(), specs.into_iter().map(|s| s.template_key).collect());
        }
        let mut cards = ctx.reads().list_cards(&CardFilter {
            deck_ids: None,
            note_ids: Some(vec![note.id]),
            include_deleted: false,
        })?;
        cards.sort_by_key(|c| (c.created_at, c.id));
        let mut taken: HashSet<String> = HashSet::new();
        for mut card in cards {
            let before = snapshot(&card);
            let source_template = template_for(&source, &card.template_key).map(|t| t.key.as_str());
            let to = source_template
                .and_then(|k| carried.get(k))
                .and_then(|t| target.templates.iter().find(|x| x.key == *t));
            let key = to.and_then(|t| {
                let ys = yields.get(t.key.as_str())?;
                [card.template_key.as_str(), t.key.as_str()]
                    .into_iter()
                    .find(|k| ys.iter().any(|y| y == k) && !taken.contains(*k))
                    .map(str::to_owned)
                    .or_else(|| ys.iter().find(|y| !taken.contains(*y)).cloned())
            });
            match (to, key) {
                (Some(t), Some(key)) => {
                    taken.insert(key.clone());
                    card.template_key = key;
                    card.card_type.clone_from(&t.card_type);
                    result.cards_kept += 1;
                }
                _ => {
                    card.deleted_at = Some(ctx.now);
                    result.cards_trashed += 1;
                }
            }
            card.updated_at = ctx.now;
            ctx.tx.update_card(&card)?;
            card_before.push(before);
            changed_cards.push(card.id);
        }
        let deck = deck_for_new_cards(ctx.reads(), &note)?;
        for id in add_missing_cards(ctx, &target, &note, deck)? {
            let created = ctx
                .reads()
                .get_card(id)?
                .ok_or_else(|| invalid("generated card vanished"))?;
            // Undo trashes what the change generated (like `cards.trash_generated`).
            let mut gone = snapshot(&created);
            gone.deleted_at = Some(ctx.now);
            card_before.push(gone);
            changed_cards.push(id);
            result.cards_created += 1;
        }
        result.notes += 1;
    }
    outcome(result, note_before, card_before, changed_notes, changed_cards)
}

fn outcome(
    result: ChangeNoteTypeResult,
    notes: Vec<NoteSnapshot>,
    cards: Vec<CardSnapshot>,
    changed_notes: Vec<NoteId>,
    changed_cards: Vec<CardId>,
) -> AppResult<CommandOutcome<ChangeNoteTypeResult>> {
    let mut inverse = Vec::new();
    if !cards.is_empty() {
        inverse.push(inverse_call::<BulkRestoreCards>(&RestoreCards { cards })?);
    }
    if !notes.is_empty() {
        inverse.push(inverse_call::<BulkRestoreNotes>(&RestoreNotes { notes })?);
    }
    let mut events: Vec<DomainEvent> = changed_notes
        .into_iter()
        .map(|note_id| DomainEvent::NoteUpdated { note_id })
        .collect();
    if !changed_cards.is_empty() {
        events.push(DomainEvent::CardsChanged { ids: changed_cards });
    }
    Ok(CommandOutcome {
        response: result,
        inverse,
        summary: summary::<BulkChangeNotetype>().param("count", result.notes),
        events,
    })
}
