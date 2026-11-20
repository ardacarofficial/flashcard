//! `timed` module: timed mode (docs/02 §6.1, docs/03 §9). `study.start { modes.timed:
//! { seconds, on_timeout } }` gives every card a time limit measured on the **backend clock**
//! from when the screen put it up: `modes.timed.start { card_id }` starts that card's countdown
//! (kept per card in `ModesState.timer`; a no-op while it runs, refused once it ran out, so it
//! can never buy more time). The countdown ends with the card's answer, and an undo forgets
//! it (the card back on screen starts afresh); any other card on screen (after suspend, bury,
//! postpone, a learning card coming due) has no countdown until the screen starts one, so a
//! card nobody saw never runs out. A button answer later than the limit (+ grace) is refused
//! with `CONFLICT_TIME_UP`, one without a started countdown with `VALIDATION_SESSION_MODE`
//! (`timer_not_started`); `modes.timed.expire` (refused before the limit:
//! `CONFLICT_TIMER_RUNNING`) answers the card "Again" through the session engine
//! (`review_log.mode = timed`) or skips it (no review, schedule unchanged). Both are one undo
//! step.

use super::session_mode;
use crate::error::AppResult;
use crate::handler::{inverse_call, CommandCtx, CommandOutcome};
use crate::module::{choice_setting, ModuleDescriptor};
use crate::modules::study::ext::*;
use crate::registry::Registry;
use crate::support::summary;
use fc_api::v1::methods::{ModesTimedExpire, ModesTimedStart, StudyRestore};
use fc_api::v1::modes::{ExpireCard, StartTimer};
use fc_api::v1::study::{AnswerCard, RestoreStudy, SessionDto};
use fc_api::{ApiError, ErrorCode};
use fc_core::card::Rating;
use fc_core::settings::{SettingDef, SettingKind, SettingScopeKind};
use fc_core::{CardId, Timestamp};
use fc_study::modes::timed::{CardTimer, TimeoutAction, Timed, MAX_SECONDS, MIN_SECONDS};
use fc_study::session::SessionState;
use fc_study::session as queue;

/// Module id.
pub const ID: &str = "timed";
/// Default seconds per card (start form).
pub const SECONDS: &str = "timed.seconds";
/// Default timeout action (start form).
pub const ON_TIMEOUT: &str = "timed.on_timeout";

/// Descriptor.
pub fn descriptor() -> ModuleDescriptor {
    ModuleDescriptor {
        id: ID,
        name_key: "modules.timed.name",
        description_key: "modules.timed.description",
        enabled_by_default: true,
        can_disable: true,
        settings: vec![
            SettingDef {
                key: SECONDS.to_owned(),
                kind: SettingKind::Integer {
                    min: Some(i64::from(MIN_SECONDS)),
                    max: Some(i64::from(MAX_SECONDS)),
                },
                default: serde_json::Value::from(10),
                label_key: format!("settings.{SECONDS}.label"),
                description_key: Some(format!("settings.{SECONDS}.description")),
                advanced: false,
                scopes: vec![SettingScopeKind::App],
            },
            choice_setting(ON_TIMEOUT, &["again", "skip"], "again"),
        ],
        ui: vec![],
    }
}

/// Registers the module.
pub fn register(reg: &mut Registry) {
    reg.module(descriptor());
    reg.command::<ModesTimedExpire>(expire);
    reg.command::<ModesTimedStart>(start);
}

fn timed_of(s: &Session) -> AppResult<Timed> {
    s.config
        .modes
        .as_ref()
        .and_then(|m| m.timed)
        .ok_or_else(|| session_mode("not_timed", "not a timed session"))
}

/// When `card`'s countdown started (`None`: it has not).
pub(super) fn started(s: &Session, card: CardId) -> Option<Timestamp> {
    CardTimer::of(s.state.modes.as_ref().and_then(|m| m.timer.as_ref()), card)
}

/// The countdown of `card` ends (it was answered or left the session).
pub(crate) fn card_left_screen(state: &mut SessionState, card: CardId) {
    if let Some(m) = state.modes.as_mut() {
        if m.timer.is_some_and(|t| t.card_id == card) {
            m.timer = None;
        }
    }
}

/// Undo put an earlier state back: the card on screen starts afresh.
pub(crate) fn forget_timer(state: &mut SessionState) {
    if let Some(m) = state.modes.as_mut() {
        m.timer = None;
    }
}

fn not_started(card: CardId) -> crate::error::AppError {
    crate::error::AppError::Api(
        ApiError::new(
            ErrorCode::ValidationSessionMode,
            "the card's countdown has not started (modes.timed.start)",
        )
        .with_detail("reason", "timer_not_started")
        .with_detail("card_id", card.to_string()),
    )
}

/// Refuses a button answer of `card` that comes later than its time limit, or whose
/// countdown never started (no-op in untimed sessions).
pub(super) fn check_in_time(s: &Session, card: CardId, now: Timestamp) -> AppResult<()> {
    let Some(t) = s.config.modes.as_ref().and_then(|m| m.timed) else {
        return Ok(());
    };
    match started(s, card) {
        None => Err(not_started(card)),
        Some(at) if t.is_late(at, now) => Err(ApiError::new(
            ErrorCode::ConflictTimeUp,
            "the card's time ran out (modes.timed.expire)",
        )
        .with_detail("card_id", card.to_string())
        .into()),
        Some(_) => Ok(()),
    }
}

fn expire(req: ExpireCard, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<SessionDto>> {
    let mut session = Session::load(ctx.reads(), req.session_id)?;
    let timed = timed_of(&session)?;
    session.require_open()?;
    if session.current(ctx.now).map(|e| e.card_id) != Some(req.card_id) {
        return Err(not_available(req.card_id));
    }
    let shown = started(&session, req.card_id).ok_or_else(|| not_started(req.card_id))?;
    if !timed.has_expired(shown, ctx.now) {
        let remaining = timed.deadline(shown).as_millis() - ctx.now.as_millis();
        return Err(ApiError::new(ErrorCode::ConflictTimerRunning, "the card still has time")
            .with_detail("remaining_ms", remaining.to_string())
            .into());
    }
    let limit_ms = timed.seconds.saturating_mul(1_000);
    match timed.on_timeout {
        TimeoutAction::Again => {
            let outcome = answer_core(
                ctx,
                &AnswerCard {
                    session_id: req.session_id,
                    card_id: req.card_id,
                    rating: Rating::Again,
                    duration_ms: limit_ms,
                    reveal_ms: None,
                },
                AnswerHooks {
                    after: Some(Box::new(|state, _, _, _| {
                        if let Some(m) = state.modes.as_mut() {
                            m.timed_out += 1;
                        } else {
                            state.modes = Some(fc_study::modes::ModesState {
                                timed_out: 1,
                                ..Default::default()
                            });
                        }
                    })),
                    ..AnswerHooks::default()
                },
            )?;
            Ok(CommandOutcome {
                summary: summary::<ModesTimedExpire>(),
                ..outcome
            })
        }
        TimeoutAction::Skip => {
            let before = session.snapshot();
            queue::remove_cards(&mut session.state, &[req.card_id]);
            card_left_screen(&mut session.state, req.card_id);
            let m = session.state.modes.get_or_insert_with(Default::default);
            m.timed_out += 1;
            session.state.hint_card = None;
            session.state.hints = 0;
            session.state.last_event_at = Some(ctx.now);
            session.save(ctx.tx)?;
            Ok(CommandOutcome {
                response: session_dto(View::cmd(ctx), &session, ctx.now)?,
                inverse: vec![inverse_call::<StudyRestore>(&RestoreStudy {
                    cards: Vec::new(),
                    session: Some(before),
                    remove_reviews: Vec::new(),
                    add_reviews: Vec::new(),
                })?],
                summary: summary::<ModesTimedExpire>(),
                events: Vec::new(),
            })
        }
    }
}

fn start(req: StartTimer, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<SessionDto>> {
    let mut session = Session::load(ctx.reads(), req.session_id)?;
    let timed = timed_of(&session)?;
    session.require_open()?;
    if session.current(ctx.now).map(|e| e.card_id) != Some(req.card_id) {
        return Err(not_available(req.card_id));
    }
    match started(&session, req.card_id) {
        // Never restarts a countdown: no extra time for a card already on screen.
        Some(at) if ctx.now.as_millis() >= timed.deadline(at).as_millis() => {
            return Err(ApiError::new(
                ErrorCode::ConflictTimeUp,
                "the card's time ran out (modes.timed.expire)",
            )
            .with_detail("card_id", req.card_id.to_string())
            .into())
        }
        Some(_) => {}
        None => {
            session.state.modes.get_or_insert_with(Default::default).timer = Some(CardTimer {
                card_id: req.card_id,
                started_at: ctx.now,
            });
            session.save(ctx.tx)?;
        }
    }
    Ok(CommandOutcome {
        response: session_dto(View::cmd(ctx), &session, ctx.now)?,
        inverse: Vec::new(),
        summary: summary::<ModesTimedStart>(),
        events: Vec::new(),
    })
}
