//! Card info, duplicates, trash listing, database check and repair.

use super::row;
use crate::error::AppResult;
use crate::handler::{inverse_call, CommandCtx, CommandOutcome, QueryCtx};
use crate::support::decks::DeckIndex;
use crate::support::notes::{require_note, require_notetype, write_note};
use crate::support::{dto, err_id, summary};
use fc_api::v1::browse::*;
use fc_api::v1::bulk::{CardSnapshot, RestoreCards};
use fc_api::v1::common::Empty;
use fc_api::v1::events::DomainEvent;
use fc_api::v1::methods::*;
use fc_api::ErrorCode;
use fc_core::query::eval::elapsed_ratio;
use fc_core::query::filter::FSRS_FACTOR;
use fc_core::CardId;
use fc_storage::repo::CardHit;

pub(super) fn card_info(req: CardInfoRef, ctx: &QueryCtx<'_>) -> AppResult<CardInfo> {
    let id = req.card_id;
    let card = ctx
        .tx
        .get_card(id)?
        .ok_or_else(|| err_id(ErrorCode::NotFoundCard, format!("card {id} not found"), id))?;
    let note = require_note(ctx.tx, card.note_id)?;
    let index = DeckIndex::load(ctx.tx)?;
    let retrievability = elapsed_ratio(&card, ctx.now).map(|x| (1.0 + FSRS_FACTOR * x).powf(-0.5));
    let last_review = card.scheduling.last_review;
    let reviews = ctx
        .tx
        .card_reviews(id)?
        .into_iter()
        .map(|r| ReviewDto {
            at: r.created_at,
            rating: r.rating,
            state_before: r.state_before,
            state_after: r.state_after,
            scheduled_days: r.scheduled_days,
            elapsed_days: r.elapsed_days,
            duration_ms: r.duration_ms,
            hints_used: r.hints_used,
            mode: r.mode,
            stability: r.stability_after,
            difficulty: r.difficulty_after,
        })
        .collect();
    let hit = CardHit {
        notetype_id: note.notetype_id,
        sort_field: note.sort_field.clone(),
        tags: note.tags.clone(),
        note_updated_at: note.updated_at,
        card,
    };
    Ok(CardInfo {
        card: row(&index, hit, ctx.now),
        note: dto::note(ctx.tx, &note)?,
        last_review,
        retrievability,
        reviews,
    })
}

pub(super) fn duplicates(_: Empty, ctx: &QueryCtx<'_>) -> AppResult<DuplicateList> {
    let mut groups = Vec::new();
    for ids in ctx.tx.duplicate_note_groups()? {
        let Some(first) = ids.first() else { continue };
        let note = require_note(ctx.tx, *first)?;
        // Notes whose unique fields are all empty share a checksum without being duplicates.
        if note.sort_field.trim().is_empty() {
            continue;
        }
        groups.push(DuplicateGroup {
            sort_field: note.sort_field,
            note_ids: ids,
        });
    }
    Ok(DuplicateList { groups })
}

pub(super) fn trash(req: ListTrash, ctx: &QueryCtx<'_>) -> AppResult<TrashList> {
    let index = DeckIndex::load(ctx.tx)?;
    let mut decks: Vec<TrashedDeck> = ctx
        .tx
        .list_decks(true)?
        .into_iter()
        .filter_map(|d| {
            let at = d.deleted_at?;
            // Only roots of trashed subtrees: restoring them brings the rest back.
            let parent_same = d
                .parent_id
                .and_then(|p| index.get(p))
                .is_some_and(|p| p.deleted_at == Some(at));
            (!parent_same).then(|| TrashedDeck {
                id: d.id,
                path: index.path(d.id),
                deleted_at: at,
            })
        })
        .collect();
    decks.sort_by(|a, b| b.deleted_at.cmp(&a.deleted_at).then(a.path.cmp(&b.path)));
    let notes = ctx
        .tx
        .list_trashed_notes(req.limit.unwrap_or(200).clamp(1, 5000))?
        .into_iter()
        .filter_map(|n| {
            Some(TrashedNote {
                deleted_at: n.deleted_at?,
                id: n.id,
                sort_field: n.sort_field,
            })
        })
        .collect();
    Ok(TrashList { decks, notes })
}

fn report(ctx_reads: &dyn fc_storage::Reads) -> AppResult<DatabaseReport> {
    let r = ctx_reads.integrity_report()?;
    let healthy = r.orphan_cards.is_empty()
        && r.cards_in_trashed_decks.is_empty()
        && r.unscheduled_cards.is_empty()
        && r.missing_search_index.is_empty();
    Ok(DatabaseReport {
        orphan_cards: r.orphan_cards,
        cards_in_trashed_decks: r.cards_in_trashed_decks,
        unscheduled_cards: r.unscheduled_cards,
        missing_search_index: r.missing_search_index,
        cardless_notes: r.cardless_notes,
        dangling_values: r.dangling_values,
        healthy,
    })
}

pub(super) fn check_database(_: Empty, ctx: &QueryCtx<'_>) -> AppResult<DatabaseReport> {
    report(ctx.tx)
}

/// Repairs what [`check_database`] reports: orphan cards and cards in trashed decks go to the
/// trash, scheduled cards without a due time become due now (both undone exactly by one
/// `bulk.restore_cards`), and missing search rows are rebuilt (derived data, nothing to undo).
pub(super) fn repair_database(
    _: Empty,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<RepairResult>> {
    let r = report(ctx.reads())?;
    let mut before: Vec<CardSnapshot> = Vec::new();
    let mut changed: Vec<CardId> = Vec::new();
    let mut result = RepairResult::default();
    let mut fix = |ctx: &mut CommandCtx<'_>, id: CardId, trash: bool| -> AppResult<()> {
        let Some(mut card) = ctx.reads().get_card(id)? else {
            return Ok(());
        };
        if changed.contains(&id) {
            return Ok(());
        }
        before.push(super::bulk::snapshot(&card));
        if trash {
            card.deleted_at = Some(ctx.now);
            result.trashed_cards += 1;
        } else {
            card.scheduling.due = Some(ctx.now);
            result.rescheduled_cards += 1;
        }
        card.updated_at = ctx.now;
        ctx.tx.update_card(&card)?;
        changed.push(id);
        Ok(())
    };
    for id in r.orphan_cards.iter().chain(&r.cards_in_trashed_decks) {
        fix(ctx, *id, true)?;
    }
    for id in &r.unscheduled_cards {
        fix(ctx, *id, false)?;
    }
    for id in &r.missing_search_index {
        let mut note = require_note(ctx.reads(), *id)?;
        let nt = require_notetype(ctx.reads(), note.notetype_id)?;
        write_note(ctx, &nt, &mut note)?;
        result.reindexed_notes += 1;
    }
    let total = result.trashed_cards + result.rescheduled_cards + result.reindexed_notes;
    Ok(CommandOutcome {
        response: result,
        inverse: if before.is_empty() {
            Vec::new()
        } else {
            vec![inverse_call::<BulkRestoreCards>(&RestoreCards { cards: before })?]
        },
        summary: summary::<BrowseRepairDatabase>().param("count", total),
        events: if changed.is_empty() {
            Vec::new()
        } else {
            vec![DomainEvent::CardsChanged { ids: changed }]
        },
    })
}
