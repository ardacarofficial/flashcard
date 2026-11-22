//! Session handlers: start, current card, answer (undoable), hints, answer checking, end.

use super::engine::*;
use super::plan::{Plan, PlanRequest, Planner, UX_CLIENT_TZ};
use super::*;
use crate::error::AppResult;
use crate::handler::{inverse_call, CommandCtx, CommandOutcome, QueryCtx};
use crate::support::{invalid, summary};
use fc_api::v1::common::Empty;
use fc_api::v1::events::DomainEvent;
use fc_api::v1::study::*;
use fc_core::card::{CardState, Rating};
use fc_core::settings::SettingScope;
use fc_core::{CardId, ReviewLogId, SessionId, Timestamp};
use fc_storage::repo::{CardFilter, ReviewLogRecord, SessionRecord};
use fc_api::v1::cardtypes::CardInteraction;
use fc_providers::strategy::{CardResponse, Evaluation};
use fc_study::answer::{check, CheckOptions, CheckerKind, Grade};
use fc_study::text::html_to_text;
use fc_study::day::MAX_TZ_OFFSET_MINS;
use fc_study::hint::HintKind;
use fc_study::session::{self as queue, SessionState};

/// Ends the open sessions of `kind` at `now` (v1.3: one session is open per kind, so a test
/// paper or a learn round survives a flashcard session started in between).
fn end_open_sessions(ctx: &mut CommandCtx<'_>, kind: StudyKind) -> AppResult<()> {
    let kind = kind_str(kind);
    for mut open in ctx.reads().open_sessions()? {
        if open.kind == kind {
            open.ended_at = Some(ctx.now);
            ctx.tx.upsert_session(&open)?;
        }
    }
    Ok(())
}

/// `CONFLICT_NOTHING_TO_STUDY` with why and when that changes.
fn nothing_to_study(reason: StudyEmptyReason, next: Option<Timestamp>) -> crate::AppError {
    let reason_str = serde_json::to_value(reason)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default();
    crate::AppError::Api(
        fc_api::ApiError::new(
            fc_api::ErrorCode::ConflictNothingToStudy,
            format!("nothing to study ({reason_str})"),
        )
        .with_detail("reason", reason_str)
        .with_detail(
            "next_available_at",
            next.map_or(serde_json::Value::Null, |t| serde_json::Value::from(t.0)),
        ),
    )
}

pub(super) fn start(req: StartSession, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<SessionDto>> {
    let tz = req.tz_offset_mins.unwrap_or(0);
    if tz.abs() > MAX_TZ_OFFSET_MINS {
        return Err(invalid("tz_offset_mins out of range"));
    }
    let id = SessionId::generate(ctx.ids);
    // v0.10 modes: module switches, time limit, rating scale (validated before anything is
    // written).
    let modes_config = crate::modules::modes::prepare_start(ctx, &req)?;
    let filter = req.filter.clone().unwrap_or_default();
    let plan_req = PlanRequest {
        kind: req.kind,
        scope: req.scope,
        deck_id: req.deck_id,
        include_subdecks: req.include_subdecks,
        filter: &filter,
        from_session: req.from_session,
        new_limit: req.new_limit,
        order: req.order,
        seed: id.0.as_u64_pair().1,
    };
    let mut planner = Planner::new(View::cmd(ctx), ctx.now, tz)?;
    let Plan {
        queue,
        mut rng,
        query,
        practice,
    } = planner.plan(&plan_req)?;
    // v1.3: an empty start creates nothing and leaves the open sessions alone. Card-by-card
    // sessions also need a card to show now (learn/test/match take every card at once).
    let shows_now = match req.kind {
        StudyKind::Normal | StudyKind::Custom | StudyKind::Mistakes => planner.available(&queue),
        StudyKind::Learn | StudyKind::Test | StudyKind::Match => !queue.is_empty(),
    };
    if !shows_now {
        let (reason, next) = planner.empty_reason(&plan_req, &queue)?;
        return Err(nothing_to_study(reason, next));
    }
    let day_start_hour = planner.day_start_hour();
    drop(planner);
    let config = SessionConfig {
        kind: req.kind,
        // Practice scopes never change the schedule, whatever was asked.
        mode: if practice {
            StudyMode::Trial
        } else {
            req.mode.unwrap_or_default()
        },
        deck_id: req.deck_id,
        tz_offset_mins: tz,
        day_start_hour,
        modes: modes_config,
    };
    let mut state = SessionState {
        queue,
        last_event_at: Some(ctx.now),
        ..SessionState::default()
    };
    // v0.10: learn progress / test paper (no-op for other kinds).
    crate::modules::modes::init_state(View::cmd(ctx), &req, &config, &mut state, &mut rng)?;
    end_open_sessions(ctx, req.kind)?;
    // The client's offset, for queries that get none (today counts, `reviewed:1`).
    if req.tz_offset_mins.is_some() {
        ctx.tx
            .set_ux_state(UX_CLIENT_TZ, &serde_json::Value::from(tz), ctx.now)?;
    }
    let mut session = Session::new(
        ctx.registry,
        SessionRecord {
            id,
            kind: kind_str(req.kind).to_owned(),
            query,
            config: serde_json::to_value(&config)
                .map_err(|e| crate::AppError::api(fc_api::ErrorCode::Internal, e.to_string()))?,
            state: serde_json::Value::Null,
            started_at: ctx.now,
            ended_at: None,
        },
        config,
        state,
    );
    session.save(ctx.tx)?;
    Ok(CommandOutcome {
        response: session_dto(View::cmd(ctx), &session, ctx.now)?,
        inverse: Vec::new(),
        summary: summary::<StudyStart>(),
        events: Vec::new(),
    })
}

pub(super) fn get(req: SessionRef, ctx: &QueryCtx<'_>) -> AppResult<SessionDto> {
    let s = Session::load(ctx.tx, ctx.registry, req.session_id)?;
    session_dto(View::query(ctx), &s, ctx.now)
}

pub(super) fn active(_: Empty, ctx: &QueryCtx<'_>) -> AppResult<ActiveSession> {
    let session = match ctx.tx.latest_open_session()? {
        Some(r) => Some(session_dto(
            View::query(ctx),
            &Session::load(ctx.tx, ctx.registry, r.id)?,
            ctx.now,
        )?),
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

/// Whether answering a card of a session of `kind` buries its siblings (hand-picked sets of
/// mistakes, learn and test cards keep them).
fn buries_siblings(kind: StudyKind) -> bool {
    matches!(kind, StudyKind::Normal | StudyKind::Custom)
}

pub(super) fn answer(req: AnswerCard, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<SessionDto>> {
    // v0.10: learn cards are judged (`modes.learn.answer`); a timed card past its time is
    // answered by `modes.timed.expire`.
    crate::modules::modes::check_button_answer(ctx, &req)?;
    answer_core(ctx, &req, AnswerHooks::default())
}

/// What a v0.10 mode changes about an answer (extension point of the session engine).
#[derive(Default)]
pub(crate) struct AnswerHooks<'a> {
    /// Rated on the confidence scale (`review_log.mode = confidence`).
    pub confidence: bool,
    /// Keep the card out of the engine's intraday re-queueing (learn mode re-queues itself).
    pub no_requeue: bool,
    /// Practice only: the schedule (real or simulated) stays as it is and the review is
    /// logged with the practice variant of its mode (`trial_<mode>`; learn mode's
    /// multiple-choice answers on cards already in review).
    pub practice: bool,
    /// Runs on the session state after the answer is recorded, before it is saved: the
    /// answered card, its note and queue category.
    #[allow(clippy::type_complexity)]
    pub after: Option<Box<dyn FnOnce(&mut SessionState, CardId, fc_core::NoteId, queue::QueueKind) + 'a>>,
}

/// Rates `req.card_id` in its session: scheduling (or trial simulation), sibling burying,
/// review log, session queue and counters — one undo step through `study.restore`. Shared
/// by `study.answer` and the v0.10 modes.
pub(crate) fn answer_core(
    ctx: &mut CommandCtx<'_>,
    req: &AnswerCard,
    hooks: AnswerHooks<'_>,
) -> AppResult<CommandOutcome<SessionDto>> {
    let mut session = Session::load(ctx.reads(), ctx.registry, req.session_id)?;
    session.require_open()?;
    session.require_available(req.card_id, ctx.now)?;
    let queue_kind = session
        .state
        .queue
        .iter()
        .find(|e| e.card_id == req.card_id)
        .map_or(queue::QueueKind::New, |e| e.kind);
    let mut card = require_live_card(ctx.reads(), req.card_id)?;
    if EmptyCards::new(View::cmd(ctx)).is_empty(&card)? {
        return Err(not_available(card.id));
    }
    let card_before = snapshot(&card);
    let before = session.scheduling_of(&card);
    let params = session.config.params(ctx.reads(), card.deck_id)?;
    let mut outcome = scheduler(ctx.reads())?
        .schedule(&before, req.rating, ctx.now, &params)
        .map_err(scheduler_error)?;
    if hooks.practice {
        outcome.state = before.clone();
        outcome.scheduled_days = 0;
    } else if session.config.mode == StudyMode::Scheduled {
        // v0.6: load balancing, easy days, exam-date mode (no-op when `planning` is off).
        crate::modules::planning::plan_outcome(
            ctx.reads(),
            ctx.registry,
            &card,
            &mut outcome,
            session.config.study_day(),
            ctx.now,
            params.maximum_interval_days,
        )?;
    }
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
        _ if hooks.practice => {}
        StudyMode::Scheduled => {
            restore_cards.push(card_before);
            card.scheduling = outcome.state.clone();
            card.updated_at = ctx.now;
            ctx.tx.update_card(&card)?;
            changed.push(card.id);
            let bury = buries_siblings(session.config.kind)
                && effective_options(ctx.reads(), Some(card.deck_id))?.bury_siblings;
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
        if hooks.no_requeue {
            None
        } else {
            requeue_at(&outcome.state, day_end)
        },
    );
    // Its countdown ends with the answer (a card coming back later starts a new one).
    crate::modules::modes::timed::card_left_screen(&mut session.state, card.id);
    if let Some(after) = hooks.after {
        after(&mut session.state, card.id, card.note_id, queue_kind);
    }
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
        mode: {
            let mode = review_mode(&session.config, hooks.confidence);
            if hooks.practice && !fc_storage::repo::is_practice_mode(&mode) {
                format!("trial_{mode}")
            } else {
                mode
            }
        },
        stability_after: outcome.state.stability,
        difficulty_after: outcome.state.difficulty,
        created_at: ctx.now,
    };
    ctx.tx.insert_review_log(&review)?;
    drop_dead_ahead(View::cmd(ctx), &mut session, ctx.now)?;
    let undo = session.save(ctx.tx)?;
    let rating = serde_json::to_value(req.rating)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default();
    Ok(CommandOutcome {
        response: session_dto(View::cmd(ctx), &session, ctx.now)?,
        inverse: vec![inverse_call::<StudyRestore>(&RestoreStudy {
            cards: restore_cards,
            session: Some(undo.snapshot()?),
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

/// Removes cards deleted or emptied since the queue was built from the front of the queue:
/// the entries about to be shown, not the whole queue (v1.0 — an answer costs the same with
/// 20 or 20 000 queued cards). The rest leave when they come up (the session DTO skips them
/// the same way).
fn drop_dead_ahead(v: View<'_>, session: &mut Session, now: Timestamp) -> AppResult<()> {
    let mut empty = EmptyCards::new(v);
    while let Some(entry) = session.current(now) {
        let id = entry.card_id;
        match require_live_card(v.reads, id) {
            Ok(card) if !empty.is_empty(&card)? => break,
            _ => {
                queue::remove_cards(&mut session.state, &[id]);
            }
        }
    }
    Ok(())
}

pub(super) fn hint(req: HintRequest, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<HintDto>> {
    let mut session = Session::load(ctx.reads(), ctx.registry, req.session_id)?;
    session.require_open()?;
    session.require_available(req.card_id, ctx.now)?;
    let used = if session.state.hint_card == Some(req.card_id) {
        session.state.hints
    } else {
        0
    };
    let kind = HintKind::parse(&setting_str(ctx.reads(), &SettingScope::App, HINT_STRATEGY)?)
        .unwrap_or(HintKind::FirstLetters);
    // v1.3: the context sentence first when the note has one (engine::study_hint).
    let hint = super::engine::study_hint(View::cmd(ctx), req.card_id, kind, used + 1)?;
    let level = if hint.is_some() {
        session.state.hint_card = Some(req.card_id);
        session.state.hints = used + 1;
        session.save(ctx.tx)?;
        used + 1
    } else {
        used
    };
    Ok(CommandOutcome {
        response: super::engine::hint_dto(level, hint),
        inverse: Vec::new(),
        summary: summary::<StudyHint>(),
        events: Vec::new(),
    })
}

/// Judges an answer the way the card type engine says (v0.3 `CardTypeEngine::evaluate`):
/// choice cards are decided by the engine; typed answers go to the configured v0.2 answer
/// checker (`CheckText`; also for self-graded cards when `study.type_answer` is on).
pub(super) fn check_answer(req: CheckAnswer, ctx: &QueryCtx<'_>) -> AppResult<AnswerCheckDto> {
    let session = Session::load(ctx.tx, ctx.registry, req.session_id)?;
    session.require_available(req.card_id, ctx.now)?;
    let card = require_live_card(ctx.tx, req.card_id)?;
    let r = render_for_study(View::query(ctx), &card)?;
    let response = match req.choice {
        Some(i) => CardResponse::Choice(i as usize),
        None => CardResponse::Text(req.given.clone()),
    };
    let auto_grade = setting_bool(ctx.tx, &SettingScope::App, AUTO_GRADE)?;
    let (expected, given) = match r.card.engine.evaluate(&r.card.raw, &response) {
        Evaluation::Decided { correct } => {
            let expected = match &r.card.sides.interaction {
                CardInteraction::Choice { options, correct } => options
                    .get(*correct as usize)
                    .map(|o| html_to_text(o))
                    .unwrap_or_default(),
                _ => r.expected.clone(),
            };
            let grade = if correct { Grade::Correct } else { Grade::Wrong };
            return Ok(AnswerCheckDto {
                grade: grade_dto(grade),
                score: if correct { 1.0 } else { 0.0 },
                diff: Vec::new(),
                expected,
                suggested_rating: grade.suggested_rating(),
                auto_grade,
            });
        }
        Evaluation::CheckText { expected, given } => (expected, given),
        Evaluation::SelfGraded if req.choice.is_none() => (r.expected.clone(), req.given),
        Evaluation::SelfGraded => return Err(invalid("this card is not a choice card")),
    };
    let kind = CheckerKind::parse(&setting_str(ctx.tx, &SettingScope::App, ANSWER_CHECKER)?)
        .unwrap_or(CheckerKind::Multi);
    let opts = CheckOptions {
        fold_turkish: setting_bool(ctx.tx, &SettingScope::App, FOLD_TURKISH)?,
        ..CheckOptions::default()
    };
    let c = check(kind, &expected, &given, &opts);
    Ok(AnswerCheckDto {
        grade: grade_dto(c.grade),
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
        auto_grade,
    })
}

fn grade_dto(g: Grade) -> AnswerGrade {
    match g {
        Grade::Correct => AnswerGrade::Correct,
        Grade::Almost => AnswerGrade::Almost,
        Grade::Wrong => AnswerGrade::Wrong,
    }
}

pub(super) fn end(req: SessionRef, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<SessionSummary>> {
    let mut session = Session::load(ctx.reads(), ctx.registry, req.session_id)?;
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
    Ok(summary_dto(&Session::load(ctx.tx, ctx.registry, req.session_id)?))
}
