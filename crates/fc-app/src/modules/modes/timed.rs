//! `timed` module: timed mode (docs/02 §6.1, docs/03 §9). `study.start { modes.timed:
//! { seconds, on_timeout } }` gives every card a time limit measured on the **backend clock**
//! from when it was put on screen (previous answer, start, or `modes.timed.restart` after a
//! resume). A button answer later than the limit (+ grace) is refused with
//! `CONFLICT_TIME_UP`; `modes.timed.expire` (refused before the limit: `CONFLICT_TIMER_RUNNING`)
//! answers the card "Again" through the session engine (`review_log.mode = timed`) or skips it
//! (no review, schedule unchanged). Both are one undo step.

use super::{session_mode, shown_at};
use crate::error::AppResult;
use crate::handler::{inverse_call, CommandCtx, CommandOutcome};
use crate::module::{choice_setting, ModuleDescriptor};
use crate::modules::study::ext::*;
use crate::registry::Registry;
use crate::support::summary;
use fc_api::v1::methods::{ModesTimedExpire, ModesTimedRestart, StudyRestore};
use fc_api::v1::modes::ExpireCard;
use fc_api::v1::study::{AnswerCard, RestoreStudy, SessionDto, SessionRef};
use fc_api::{ApiError, ErrorCode};
use fc_core::card::Rating;
use fc_core::settings::{SettingDef, SettingKind, SettingScopeKind};
use fc_core::{CardId, Timestamp};
use fc_study::modes::timed::{TimeoutAction, Timed, MAX_SECONDS, MIN_SECONDS};
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
    reg.command::<ModesTimedRestart>(restart);
}

fn timed_of(s: &Session) -> AppResult<Timed> {
    s.config
        .modes
        .as_ref()
        .and_then(|m| m.timed)
        .ok_or_else(|| session_mode("not_timed", "not a timed session"))
}

/// Refuses a button answer of `card` that comes later than its time limit (no-op in untimed
/// sessions).
pub(super) fn check_in_time(s: &Session, card: CardId, now: Timestamp) -> AppResult<()> {
    match s.config.modes.as_ref().and_then(|m| m.timed) {
        Some(t) if t.is_late(shown_at(s), now) => Err(ApiError::new(
            ErrorCode::ConflictTimeUp,
            "the card's time ran out (modes.timed.expire)",
        )
        .with_detail("card_id", card.to_string())
        .into()),
        _ => Ok(()),
    }
}

fn expire(req: ExpireCard, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<SessionDto>> {
    let mut session = Session::load(ctx.reads(), req.session_id)?;
    let timed = timed_of(&session)?;
    session.require_open()?;
    if session.current(ctx.now).map(|e| e.card_id) != Some(req.card_id) {
        return Err(not_available(req.card_id));
    }
    let shown = shown_at(&session);
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

fn restart(req: SessionRef, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<SessionDto>> {
    let mut session = Session::load(ctx.reads(), req.session_id)?;
    timed_of(&session)?;
    session.require_open()?;
    session.state.last_event_at = Some(ctx.now);
    session.save(ctx.tx)?;
    Ok(CommandOutcome {
        response: session_dto(View::cmd(ctx), &session, ctx.now)?,
        inverse: Vec::new(),
        summary: summary::<ModesTimedRestart>(),
        events: Vec::new(),
    })
}
