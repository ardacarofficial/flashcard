//! Session handlers: start, current card, answer (undoable), hints, answer checking, end.

use super::engine::*;
use super::*;
use crate::error::AppResult;
use crate::handler::{inverse_call, CommandCtx, CommandOutcome, QueryCtx};
use crate::support::decks::DeckIndex;
use crate::support::{invalid, summary};
use fc_api::v1::common::Empty;
use fc_api::v1::events::DomainEvent;
use fc_api::v1::study::*;
use fc_core::card::{CardState, Rating};
use fc_core::rng::SplitMix64;
use fc_core::settings::SettingScope;
use fc_core::{CardId, ReviewLogId, SessionId, Timestamp};
use fc_storage::repo::{CardFilter, ReviewLogRecord, SessionRecord, StudyCardFilter};
use fc_study::answer::{check, CheckOptions, CheckerKind, Grade};
use fc_study::day::MAX_TZ_OFFSET_MINS;
use fc_study::hint::HintKind;
use fc_study::session::{self as queue, BuildOptions, Order, SessionState};
use std::collections::HashMap;

/// Ends every open session at `now` (one session is open at a time).
fn end_open_sessions(ctx: &mut CommandCtx<'_>) -> AppResult<()> {
    // Bounded: each iteration closes one.
    for _ in 0..1_000 {
        let Some(mut open) = ctx.reads().latest_open_session()? else {
            return Ok(());
        };
        open.ended_at = Some(ctx.now);
        ctx.tx.upsert_session(&open)?;
    }
    Ok(())
}

fn order_of(o: StudyOrder) -> Order {
    match o {
        StudyOrder::Due => Order::Due,
        StudyOrder::Random => Order::Random,
        StudyOrder::HardestFirst => Order::HardestFirst,
        StudyOrder::NewestFirst => Order::NewestFirst,
    }
}

fn parse_order(s: &str) -> StudyOrder {
    match s {
        "random" => StudyOrder::Random,
        "hardest_first" => StudyOrder::HardestFirst,
        "newest_first" => StudyOrder::NewestFirst,
        _ => StudyOrder::Due,
    }
}

pub(super) fn start(req: StartSession, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<SessionDto>> {
    let tz = req.tz_offset_mins.unwrap_or(0);
    if tz.abs() > MAX_TZ_OFFSET_MINS {
        return Err(invalid("tz_offset_mins out of range"));
    }
    let id = SessionId::generate(ctx.ids);
    let mode = req.mode.unwrap_or_default();
    let filter = req.filter.clone().unwrap_or_default();
    // Which cards.
    let deck_ids = match req.deck_id {
        Some(d) => {
            let index = DeckIndex::load(ctx.reads())?;
            index.require_live(d)?;
            let subdecks = match req.include_subdecks {
                Some(b) => b,
                None => setting_bool_any(ctx, d)?,
            };
            Some(if subdecks { index.live_subtree(d) } else { vec![d] })
        }
        None => None,
    };
    let card_ids = match req.kind {
        StudyKind::Mistakes => {
            let from = req
                .from_session
                .ok_or_else(|| invalid("a mistakes session needs from_session"))?;
            Some(Session::load(ctx.reads(), from)?.state.again)
        }
        StudyKind::Normal | StudyKind::Custom => None,
    };
    let candidates = ctx.reads().study_cards(&StudyCardFilter {
        deck_ids,
        card_ids,
        states: filter.states.clone(),
        attributes: filter
            .attributes
            .iter()
            .map(|c| (c.attr_id, c.values.clone()))
            .collect(),
        include_suspended: false,
    })?;
    let scope = deck_scope(req.deck_id);
    let config = SessionConfig {
        kind: req.kind,
        mode,
        deck_id: req.deck_id,
        tz_offset_mins: tz,
        day_start_hour: u8::try_from(setting_u32(ctx.reads(), &SettingScope::App, DAY_START_HOUR)?)
            .unwrap_or(fc_study::day::DEFAULT_ROLLOVER_HOUR),
    };
    let order = match req.order {
        Some(o) => o,
        None => parse_order(&setting_str(ctx.reads(), &SettingScope::App, ORDER)?),
    };
    let new_limit = match (req.new_limit, req.kind) {
        (Some(n), _) => Some(n),
        (None, StudyKind::Normal) => Some(setting_u32(ctx.reads(), &scope, NEW_PER_SESSION)?),
        (None, StudyKind::Custom | StudyKind::Mistakes) => None,
    };
    let bury_siblings = req.kind != StudyKind::Mistakes && setting_bool(ctx.reads(), &scope, BURY_SIBLINGS)?;
    let due_only = match req.kind {
        StudyKind::Normal => filter.due_only.unwrap_or(true),
        StudyKind::Custom => filter.due_only.unwrap_or(false),
        StudyKind::Mistakes => false,
    };
    // Seeded from the session id: shuffles are reproducible from the stored session.
    let mut rng = SplitMix64::new(id.0.as_u64_pair().1);
    let mut queue = queue::build_queue(
        &candidates,
        |c| c.scheduling.clone(),
        BuildOptions {
            now: ctx.now,
            day_end: config.day_end(ctx.now),
            due_only,
            new_limit,
            order: order_of(order),
            bury_siblings,
            rng: &mut rng,
        },
    );
    if let Some(limit) = filter.limit {
        queue.truncate(limit as usize);
    }
    end_open_sessions(ctx)?;
    let mut session = Session {
        record: SessionRecord {
            id,
            kind: kind_str(req.kind).to_owned(),
            query: None,
            config: serde_json::to_value(&config)
                .map_err(|e| crate::AppError::api(fc_api::ErrorCode::Internal, e.to_string()))?,
            state: serde_json::Value::Null,
            started_at: ctx.now,
            ended_at: None,
        },
        config,
        state: SessionState {
            queue,
            last_event_at: Some(ctx.now),
            ..SessionState::default()
        },
    };
    session.save(ctx.tx)?;
    Ok(CommandOutcome {
        response: session_dto(ctx.reads(), &session, ctx.now)?,
        inverse: Vec::new(),
        summary: summary::<StudyStart>(),
        events: Vec::new(),
    })
}

/// `decks.include_subdecks` at the deck (the decks module owns the setting).
fn setting_bool_any(ctx: &CommandCtx<'_>, deck: fc_core::DeckId) -> AppResult<bool> {
    Ok(
        crate::settings::resolve_stored(ctx.reads(), &SettingScope::Deck(deck), "decks.include_subdecks")?
            .and_then(|(v, _)| v.as_bool())
            .unwrap_or(true),
    )
}

pub(super) fn get(req: SessionRef, ctx: &QueryCtx<'_>) -> AppResult<SessionDto> {
    let s = Session::load(ctx.tx, req.session_id)?;
    session_dto(ctx.tx, &s, ctx.now)
}

pub(super) fn active(_: Empty, ctx: &QueryCtx<'_>) -> AppResult<ActiveSession> {
    let session = match ctx.tx.latest_open_session()? {
        Some(r) => Some(session_dto(ctx.tx, &Session::load(ctx.tx, r.id)?, ctx.now)?),
        None => None,
    };
    Ok(ActiveSession { session })
}

/// Siblings to bury after answering `card`: live, unsuspended cards of the same note that are
/// new or due today and not already buried past the day end.
fn siblings_to_bury(
    ctx: &CommandCtx<'_>,
    card: &fc_core::card::Card,
    day_end: Timestamp,
) -> AppResult<Vec<fc_core::card::Card>> {
    let cards = ctx.reads().list_cards(&CardFilter {
        deck_ids: None,
        note_ids: Some(vec![card.note_id]),
        include_deleted: false,
    })?;
    Ok(cards
        .into_iter()
        .filter(|c| c.id != card.id && !c.suspended)
        .filter(|c| c.buried_until.is_none_or(|b| b < day_end))
        .filter(|c| match c.scheduling.state {
            CardState::New => true,
            CardState::Review => c.scheduling.due.is_none_or(|d| d < day_end),
            CardState::Learning | CardState::Relearning => false,
        })
        .collect())
}

pub(super) fn answer(req: AnswerCard, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<SessionDto>> {
    let mut session = Session::load(ctx.reads(), req.session_id)?;
    session.require_open()?;
    session.require_available(req.card_id, ctx.now)?;
    let session_before = session.snapshot();
    drop_dead_cards(ctx, &mut session)?;
    let mut card = require_live_card(ctx.reads(), req.card_id)?;
    let card_before = snapshot(&card);
    let before = session.scheduling_of(&card);
    let params = params_for_deck(ctx.reads(), card.deck_id, &mut HashMap::new())?;
    let outcome = scheduler(ctx.reads())?
        .schedule(&before, req.rating, ctx.now, &params)
        .map_err(scheduler_error)?;
    let day_end = session.config.day_end(ctx.now);
    let duration = queue::clamp_duration(req.duration_ms, session.state.last_event_at, ctx.now);
    let reveal = req.reveal_ms.map(|r| r.min(duration));
    let hints_used = if session.state.hint_card == Some(card.id) {
        session.state.hints
    } else {
        0
    };

    let mut restore_cards = Vec::new();
    let mut changed = Vec::new();
    match session.config.mode {
        StudyMode::Scheduled => {
            restore_cards.push(card_before);
            card.scheduling = outcome.state.clone();
            card.updated_at = ctx.now;
            ctx.tx.update_card(&card)?;
            changed.push(card.id);
            let bury = session.config.kind != StudyKind::Mistakes
                && setting_bool(ctx.reads(), &SettingScope::Deck(card.deck_id), BURY_SIBLINGS)?;
            if bury {
                for mut sib in siblings_to_bury(ctx, &card, day_end)? {
                    restore_cards.push(snapshot(&sib));
                    sib.buried_until = Some(day_end);
                    sib.updated_at = ctx.now;
                    ctx.tx.update_card(&sib)?;
                    queue::remove_cards(&mut session.state, &[sib.id]);
                    changed.push(sib.id);
                }
            }
        }
        StudyMode::Trial => queue::set_trial_state(&mut session.state, card.id, outcome.state.clone()),
    }
    queue::record_answer(
        &mut session.state,
        card.id,
        card.note_id,
        req.rating == Rating::Again,
        duration,
        ctx.now,
        requeue_at(&outcome.state, day_end),
    );
    let review = ReviewLogRecord {
        id: ReviewLogId::generate(ctx.ids),
        card_id: card.id,
        session_id: Some(session.record.id),
        rating: req.rating,
        state_before: before.state,
        state_after: outcome.state.state,
        scheduled_days: outcome.scheduled_days,
        elapsed_days: outcome.elapsed_days,
        duration_ms: duration,
        answer_duration_ms: reveal,
        hints_used,
        mode: review_mode(session.config.mode).to_owned(),
        stability_after: outcome.state.stability,
        difficulty_after: outcome.state.difficulty,
        created_at: ctx.now,
    };
    ctx.tx.insert_review_log(&review)?;
    session.save(ctx.tx)?;
    let rating = serde_json::to_value(req.rating)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default();
    Ok(CommandOutcome {
        response: session_dto(ctx.reads(), &session, ctx.now)?,
        inverse: vec![inverse_call::<StudyRestore>(&RestoreStudy {
            cards: restore_cards,
            session: Some(session_before),
            remove_reviews: vec![review.id],
            add_reviews: Vec::new(),
        })?],
        summary: summary::<StudyAnswer>().param("rating", rating),
        events: if changed.is_empty() {
            Vec::new()
        } else {
            vec![DomainEvent::StudyCardsChanged { card_ids: changed }]
        },
    })
}

/// Removes queued cards that were deleted since the queue was built.
fn drop_dead_cards(ctx: &CommandCtx<'_>, session: &mut Session) -> AppResult<()> {
    let mut dead = Vec::new();
    for e in &session.state.queue {
        let live = ctx
            .reads()
            .get_card(e.card_id)?
            .is_some_and(|c| c.deleted_at.is_none());
        if !live {
            dead.push(e.card_id);
        }
    }
    queue::remove_cards(&mut session.state, &dead);
    Ok(())
}

/// Text a hint/check is computed from: the typed-answer field, else the answer side.
fn expected_answer(ctx_reads: &dyn fc_storage::Reads, card: CardId) -> AppResult<String> {
    let card = require_live_card(ctx_reads, card)?;
    Ok(render_card(ctx_reads, &card)?.expected)
}

pub(super) fn hint(req: HintRequest, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<HintDto>> {
    let mut session = Session::load(ctx.reads(), req.session_id)?;
    session.require_open()?;
    session.require_available(req.card_id, ctx.now)?;
    let used = if session.state.hint_card == Some(req.card_id) {
        session.state.hints
    } else {
        0
    };
    let kind = HintKind::parse(&setting_str(ctx.reads(), &SettingScope::App, HINT_STRATEGY)?)
        .unwrap_or(HintKind::FirstLetters);
    let text = fc_study::hint::hint(kind, &expected_answer(ctx.reads(), req.card_id)?, used + 1);
    let level = if text.is_some() {
        session.state.hint_card = Some(req.card_id);
        session.state.hints = used + 1;
        session.save(ctx.tx)?;
        used + 1
    } else {
        used
    };
    Ok(CommandOutcome {
        response: HintDto { level, text },
        inverse: Vec::new(),
        summary: summary::<StudyHint>(),
        events: Vec::new(),
    })
}

pub(super) fn check_answer(req: CheckAnswer, ctx: &QueryCtx<'_>) -> AppResult<AnswerCheckDto> {
    let session = Session::load(ctx.tx, req.session_id)?;
    session.require_available(req.card_id, ctx.now)?;
    let kind = CheckerKind::parse(&setting_str(ctx.tx, &SettingScope::App, ANSWER_CHECKER)?)
        .unwrap_or(CheckerKind::Multi);
    let opts = CheckOptions {
        fold_turkish: setting_bool(ctx.tx, &SettingScope::App, FOLD_TURKISH)?,
        ..CheckOptions::default()
    };
    let c = check(kind, &expected_answer(ctx.tx, req.card_id)?, &req.given, &opts);
    Ok(AnswerCheckDto {
        grade: match c.grade {
            Grade::Correct => AnswerGrade::Correct,
            Grade::Almost => AnswerGrade::Almost,
            Grade::Wrong => AnswerGrade::Wrong,
        },
        score: c.score,
        diff: c
            .diff
            .into_iter()
            .map(|s| DiffSpanDto {
                kind: match s.kind {
                    fc_providers::strategy::DiffSpanKind::Same => DiffKind::Same,
                    fc_providers::strategy::DiffSpanKind::Missing => DiffKind::Missing,
                    fc_providers::strategy::DiffSpanKind::Extra => DiffKind::Extra,
                },
                text: s.text,
            })
            .collect(),
        expected: c.matched,
        suggested_rating: c.grade.suggested_rating(),
        auto_grade: setting_bool(ctx.tx, &SettingScope::App, AUTO_GRADE)?,
    })
}

pub(super) fn end(req: SessionRef, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<SessionSummary>> {
    let mut session = Session::load(ctx.reads(), req.session_id)?;
    if session.record.ended_at.is_none() {
        session.record.ended_at = Some(ctx.now);
        session.save(ctx.tx)?;
    }
    Ok(CommandOutcome {
        response: summary_dto(&session),
        inverse: Vec::new(),
        summary: summary::<StudyEnd>(),
        events: Vec::new(),
    })
}

pub(super) fn summary_query(req: SessionRef, ctx: &QueryCtx<'_>) -> AppResult<SessionSummary> {
    Ok(summary_dto(&Session::load(ctx.tx, req.session_id)?))
}
