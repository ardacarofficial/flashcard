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
use fc_api::v1::cardtypes::CardInteraction;
use fc_providers::strategy::{CardResponse, Evaluation};
use fc_study::answer::{check, CheckOptions, CheckerKind, Grade};
use fc_study::text::html_to_text;
use fc_study::day::MAX_TZ_OFFSET_MINS;
use fc_study::hint::HintKind;
use fc_study::session::{self as queue, BuildOptions, Order, SessionState};

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
    // Query-language filter (v0.4): text, or a saved query ("filtered deck").
    let query = match (filter.query.as_deref().map(str::trim), filter.saved_query_id) {
        (Some(q), Some(_)) if !q.is_empty() => {
            return Err(invalid("give either filter.query or filter.saved_query_id"))
        }
        (_, Some(id)) => Some(crate::modules::browse::require_saved_query(ctx.reads(), id)?.query),
        (Some(q), None) if !q.is_empty() => Some(q.to_owned()),
        _ => None,
    };
    if query.is_some() && req.kind == StudyKind::Mistakes {
        return Err(invalid("a mistakes session takes no query"));
    }
    let matching = match &query {
        Some(q) => {
            let compiled = crate::modules::browse::compile(ctx.reads(), q, ctx.now)?;
            Some(ctx.reads().search_card_ids(&compiled, ctx.now)?)
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
        StudyKind::Normal | StudyKind::Custom => matching,
    };
    let scope_decks = deck_ids.clone();
    let mut candidates = ctx.reads().study_cards(&StudyCardFilter {
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
    // Empty cards (v0.3: the note no longer yields them) are never studied.
    let mut empty = EmptyCards::new(View::cmd(ctx));
    let mut keep = Vec::with_capacity(candidates.len());
    for c in candidates {
        if !empty.is_empty(&c)? {
            keep.push(c);
        }
    }
    candidates = keep;
    let scope = deck_scope(req.deck_id);
    let config = SessionConfig {
        kind: req.kind,
        mode,
        deck_id: req.deck_id,
        tz_offset_mins: tz,
        day_start_hour: u8::try_from(setting_u32(ctx.reads(), &SettingScope::App, DAY_START_HOUR)?)
            .unwrap_or(fc_study::day::DEFAULT_ROLLOVER_HOUR),
    };
    // Deck options (v0.4 presets, inherited from parent decks): order, sibling burying.
    let preset = effective_options(ctx.reads(), req.deck_id)?;
    let order = match req.order {
        Some(o) => order_of(o),
        None => Order::Preset {
            new: preset.new_order,
            review: preset.review_order,
        },
    };
    // Normal sessions respect the deck preset's daily limits (minus what was studied today).
    let daily = match req.kind {
        StudyKind::Normal => Some(daily_left(ctx, req.deck_id, scope_decks.as_deref(), &config)?),
        StudyKind::Custom | StudyKind::Mistakes => None,
    };
    let new_limit = match (req.new_limit, daily) {
        (Some(n), _) => Some(n),
        (None, Some((new_left, _))) => {
            Some(setting_u32(ctx.reads(), &scope, NEW_PER_SESSION)?.min(new_left))
        }
        (None, None) => None,
    };
    let review_limit = daily.map(|(_, reviews_left)| reviews_left);
    let bury_siblings = req.kind != StudyKind::Mistakes && preset.bury_siblings;
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
            review_limit,
            order,
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
            query,
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
        response: session_dto(View::cmd(ctx), &session, ctx.now)?,
        inverse: Vec::new(),
        summary: summary::<StudyStart>(),
        events: Vec::new(),
    })
}

/// New and review cards still allowed today by the preset of `deck`: its `new_per_day` /
/// `reviews_per_day` minus the distinct cards first studied / reviewed (scheduled mode) since
/// the study day began, counted over `decks` (all decks when `None`).
fn daily_left(
    ctx: &CommandCtx<'_>,
    deck: Option<fc_core::DeckId>,
    decks: Option<&[fc_core::DeckId]>,
    config: &SessionConfig,
) -> AppResult<(u32, u32)> {
    let preset = effective_options(ctx.reads(), deck)?;
    let day = config.study_day();
    let since = day.start(day.index(ctx.now));
    let mut new = std::collections::HashSet::new();
    let mut reviewed = std::collections::HashSet::new();
    for r in ctx.reads().review_logs_since(since, review_mode(StudyMode::Scheduled))? {
        let set = match r.state_before {
            CardState::New => &mut new,
            CardState::Review => &mut reviewed,
            CardState::Learning | CardState::Relearning => continue,
        };
        if set.contains(&r.card_id) {
            continue;
        }
        let in_scope = match decks {
            None => true,
            Some(decks) => ctx
                .reads()
                .get_card(r.card_id)?
                .is_some_and(|c| decks.contains(&c.deck_id)),
        };
        if in_scope {
            set.insert(r.card_id);
        }
    }
    let count = |s: &std::collections::HashSet<CardId>| u32::try_from(s.len()).unwrap_or(u32::MAX);
    Ok((
        preset.new_per_day.saturating_sub(count(&new)),
        preset.reviews_per_day.saturating_sub(count(&reviewed)),
    ))
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
    session_dto(View::query(ctx), &s, ctx.now)
}

pub(super) fn active(_: Empty, ctx: &QueryCtx<'_>) -> AppResult<ActiveSession> {
    let session = match ctx.tx.latest_open_session()? {
        Some(r) => Some(session_dto(
            View::query(ctx),
            &Session::load(ctx.tx, r.id)?,
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

pub(super) fn answer(req: AnswerCard, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<SessionDto>> {
    let mut session = Session::load(ctx.reads(), req.session_id)?;
    session.require_open()?;
    session.require_available(req.card_id, ctx.now)?;
    let session_before = session.snapshot();
    drop_dead_cards(ctx, &mut session)?;
    let mut card = require_live_card(ctx.reads(), req.card_id)?;
    if EmptyCards::new(View::cmd(ctx)).is_empty(&card)? {
        return Err(not_available(card.id));
    }
    let card_before = snapshot(&card);
    let before = session.scheduling_of(&card);
    let params = session.config.params(ctx.reads(), card.deck_id)?;
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
        response: session_dto(View::cmd(ctx), &session, ctx.now)?,
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

/// Text a hint is computed from: the card type's expected typed answer, else the text of
/// the answer part.
fn expected_answer(v: View<'_>, card: CardId) -> AppResult<String> {
    let card = require_live_card(v.reads, card)?;
    Ok(render_for_study(v, &card)?.expected)
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
    let text = fc_study::hint::hint(kind, &expected_answer(View::cmd(ctx), req.card_id)?, used + 1);
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

/// Judges an answer the way the card type engine says (v0.3 `CardTypeEngine::evaluate`):
/// choice cards are decided by the engine; typed answers go to the configured v0.2 answer
/// checker (`CheckText`; also for self-graded cards when `study.type_answer` is on).
pub(super) fn check_answer(req: CheckAnswer, ctx: &QueryCtx<'_>) -> AppResult<AnswerCheckDto> {
    let session = Session::load(ctx.tx, req.session_id)?;
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
