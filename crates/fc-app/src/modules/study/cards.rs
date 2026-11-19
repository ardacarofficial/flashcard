//! Card handlers: suspend, bury, postpone (all undoable through `study.restore`), the restore
//! itself, and the card history query.

use super::engine::*;
use super::*;
use crate::error::AppResult;
use crate::handler::{inverse_call, CommandCtx, CommandOutcome, QueryCtx};
use crate::support::decks::DeckIndex;
use crate::support::{invalid, summary};
use fc_api::v1::events::DomainEvent;
use fc_api::v1::study::*;
use fc_core::card::{Card, CardState};
use fc_core::settings::SettingScope;
use fc_core::time::MILLIS_PER_DAY;
use fc_core::{CardId, SessionId, Timestamp};
use fc_storage::repo::{ReviewLogRecord, StudyCardFilter};
use fc_study::day::MAX_TZ_OFFSET_MINS;
use fc_study::session as queue;

/// Longest postponement, days.
pub const MAX_POSTPONE_DAYS: u32 = 365;

fn review_dto(r: &ReviewLogRecord) -> ReviewLogDto {
    ReviewLogDto {
        id: r.id,
        card_id: r.card_id,
        session_id: r.session_id,
        rating: r.rating,
        state_before: r.state_before,
        state_after: r.state_after,
        scheduled_days: r.scheduled_days,
        elapsed_days: r.elapsed_days,
        duration_ms: r.duration_ms,
        answer_duration_ms: r.answer_duration_ms,
        hints_used: r.hints_used,
        mode: r.mode.clone(),
        stability_after: r.stability_after,
        difficulty_after: r.difficulty_after,
        created_at: r.created_at,
    }
}

fn review_record(r: &ReviewLogDto) -> ReviewLogRecord {
    ReviewLogRecord {
        id: r.id,
        card_id: r.card_id,
        session_id: r.session_id,
        rating: r.rating,
        state_before: r.state_before,
        state_after: r.state_after,
        scheduled_days: r.scheduled_days,
        elapsed_days: r.elapsed_days,
        duration_ms: r.duration_ms,
        answer_duration_ms: r.answer_duration_ms,
        hints_used: r.hints_used,
        mode: r.mode.clone(),
        stability_after: r.stability_after,
        difficulty_after: r.difficulty_after,
        created_at: r.created_at,
    }
}

/// Distinct ids, first occurrence order.
fn distinct(ids: &[CardId]) -> Vec<CardId> {
    let mut out: Vec<CardId> = Vec::with_capacity(ids.len());
    for id in ids {
        if !out.contains(id) {
            out.push(*id);
        }
    }
    out
}

/// Applies `change` to each card (skipping cards it leaves unchanged), optionally dropping
/// them from a session's queue, and builds the `study.restore` inverse. A call that changes
/// nothing has no inverse (no undo step).
fn change_cards<M: fc_api::ApiMethod>(
    ctx: &mut CommandCtx<'_>,
    ids: &[CardId],
    session: Option<SessionId>,
    mut change: impl FnMut(&mut Card) -> bool,
) -> AppResult<CommandOutcome<StudyCardsChanged>> {
    let mut before = Vec::new();
    let mut changed = Vec::new();
    for id in distinct(ids) {
        let mut card = require_live_card(ctx.reads(), id)?;
        let snap = snapshot(&card);
        if change(&mut card) {
            card.updated_at = ctx.now;
            ctx.tx.update_card(&card)?;
            before.push(snap);
            changed.push(id);
        }
    }
    let mut session_before = None;
    if let Some(sid) = session {
        let mut s = Session::load(ctx.reads(), sid)?;
        let snap = s.snapshot();
        if queue::remove_cards(&mut s.state, ids) > 0 {
            s.save(ctx.tx)?;
            session_before = Some(snap);
        }
    }
    let n = u32::try_from(changed.len()).unwrap_or(u32::MAX);
    let inverse = if before.is_empty() && session_before.is_none() {
        Vec::new()
    } else {
        vec![inverse_call::<StudyRestore>(&RestoreStudy {
            cards: before,
            session: session_before,
            remove_reviews: Vec::new(),
            add_reviews: Vec::new(),
        })?]
    };
    Ok(CommandOutcome {
        response: StudyCardsChanged { cards: n },
        inverse,
        summary: summary::<M>().param("count", n),
        events: if changed.is_empty() {
            Vec::new()
        } else {
            vec![DomainEvent::StudyCardsChanged { card_ids: changed }]
        },
    })
}

fn require_cards(ids: &[CardId]) -> AppResult<()> {
    if ids.is_empty() {
        return Err(invalid("card_ids is empty"));
    }
    Ok(())
}

fn check_tz(tz: Option<i32>) -> AppResult<()> {
    if tz.is_some_and(|t| t.abs() > MAX_TZ_OFFSET_MINS) {
        return Err(invalid("tz_offset_mins out of range"));
    }
    Ok(())
}

pub(super) fn suspend(
    req: SuspendCards,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<StudyCardsChanged>> {
    require_cards(&req.card_ids)?;
    change_cards::<StudySuspend>(ctx, &req.card_ids, req.session_id, |c| {
        let changed = c.suspended != req.suspended;
        c.suspended = req.suspended;
        changed
    })
}

/// End of today for a client in `tz` (or the session's zone), with the configured rollover.
fn today_end(
    ctx: &CommandCtx<'_>,
    tz: Option<i32>,
    session: Option<SessionId>,
) -> AppResult<Timestamp> {
    let session_cfg = match session {
        Some(s) => Some(Session::load(ctx.reads(), s)?.config),
        None => None,
    };
    let tz = tz
        .or(session_cfg.as_ref().map(|c| c.tz_offset_mins))
        .unwrap_or(0);
    let hour = u8::try_from(setting_u32(ctx.reads(), &SettingScope::App, DAY_START_HOUR)?)
        .unwrap_or(fc_study::day::DEFAULT_ROLLOVER_HOUR);
    Ok(fc_study::day::day_end(ctx.now, tz, hour))
}

pub(super) fn bury(
    req: BuryCards,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<StudyCardsChanged>> {
    require_cards(&req.card_ids)?;
    check_tz(req.tz_offset_mins)?;
    let until = today_end(ctx, req.tz_offset_mins, req.session_id)?;
    change_cards::<StudyBury>(ctx, &req.card_ids, req.session_id, |c| {
        let target = req.buried.then_some(until);
        let unchanged = if req.buried {
            c.buried_until.is_some_and(|b| b >= until)
        } else {
            c.buried_until.is_none()
        };
        c.buried_until = if unchanged { c.buried_until } else { target };
        !unchanged
    })
}

pub(super) fn postpone(
    req: PostponeCards,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<StudyCardsChanged>> {
    if req.days == 0 || req.days > MAX_POSTPONE_DAYS {
        return Err(invalid(format!("days must be 1–{MAX_POSTPONE_DAYS}")));
    }
    check_tz(req.tz_offset_mins)?;
    let deck_ids = match req.deck_id {
        Some(d) => {
            let index = DeckIndex::load(ctx.reads())?;
            index.require_live(d)?;
            Some(if req.include_subdecks.unwrap_or(true) {
                index.live_subtree(d)
            } else {
                vec![d]
            })
        }
        None => None,
    };
    let end = today_end(ctx, req.tz_offset_mins, None)?;
    let due_today: Vec<CardId> = ctx
        .reads()
        .study_cards(&StudyCardFilter {
            deck_ids,
            states: Some(vec![CardState::Learning, CardState::Review, CardState::Relearning]),
            ..StudyCardFilter::default()
        })?
        .into_iter()
        .filter(|c| c.scheduling.due.is_some_and(|d| d < end))
        .map(|c| c.id)
        .collect();
    let shift = i64::from(req.days) * MILLIS_PER_DAY;
    let now = ctx.now;
    change_cards::<StudyPostpone>(ctx, &due_today, None, |c| {
        // Overdue cards move relative to now, not to their old due date.
        let base = c.scheduling.due.map_or(now, |d| d.max(now));
        c.scheduling.due = Some(base.plus_millis(shift));
        true
    })
}

pub(super) fn restore(
    req: RestoreStudy,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<StudyCardsChanged>> {
    let mut inverse = RestoreStudy::default();
    let mut changed = Vec::new();
    for snap in &req.cards {
        let mut card = ctx
            .reads()
            .get_card(snap.id)?
            .ok_or_else(|| crate::support::err_id(fc_api::ErrorCode::NotFoundCard, format!("card {} not found", snap.id), snap.id))?;
        inverse.cards.push(snapshot(&card));
        apply_snapshot(&mut card, snap);
        ctx.tx.update_card(&card)?;
        changed.push(card.id);
    }
    if let Some(snap) = &req.session {
        let mut s = Session::load(ctx.reads(), snap.id)?;
        inverse.session = Some(s.snapshot());
        s.record.state = snap.state.clone();
        s.record.ended_at = snap.ended_at;
        ctx.tx.upsert_session(&s.record)?;
    }
    for id in &req.remove_reviews {
        if let Some(r) = ctx.reads().get_review_log(*id)? {
            ctx.tx.delete_review_log(*id)?;
            inverse.add_reviews.push(review_dto(&r));
        }
    }
    for r in &req.add_reviews {
        ctx.tx.insert_review_log(&review_record(r))?;
        inverse.remove_reviews.push(r.id);
    }
    let n = u32::try_from(changed.len()).unwrap_or(u32::MAX);
    Ok(CommandOutcome {
        response: StudyCardsChanged { cards: n },
        inverse: vec![inverse_call::<StudyRestore>(&inverse)?],
        summary: summary::<StudyRestore>().param("count", n),
        events: if changed.is_empty() {
            Vec::new()
        } else {
            vec![DomainEvent::StudyCardsChanged { card_ids: changed }]
        },
    })
}

pub(super) fn history(req: CardHistoryRequest, ctx: &QueryCtx<'_>) -> AppResult<CardHistory> {
    let card = ctx.tx.get_card(req.card_id)?.ok_or_else(|| {
        crate::support::err_id(
            fc_api::ErrorCode::NotFoundCard,
            format!("card {} not found", req.card_id),
            req.card_id,
        )
    })?;
    let s = &card.scheduling;
    let retrievability = match (s.state, s.stability, s.last_review) {
        (CardState::New, _, _) => None,
        (_, Some(stability), Some(last)) => fc_study::scheduler::retrievability(
            stability,
            (ctx.now.as_millis() - last.as_millis()) as f64 / MILLIS_PER_DAY as f64,
        ),
        _ => None,
    };
    Ok(CardHistory {
        card: snapshot(&card),
        retrievability,
        reviews: ctx
            .tx
            .review_logs_for_card(card.id)?
            .iter()
            .map(review_dto)
            .collect(),
    })
}
