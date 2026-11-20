//! v0.10 study modes (docs/02 §6.1, §6.2, §14, §27): six modules, each switchable in settings.
//!
//! * `learn` ([`learn`]): Quizlet-style learn sessions (`study.start { kind: learn }`,
//!   `modes.learn.answer`).
//! * `practice_test` ([`practice_test`]): test papers (`study.start { kind: test }`,
//!   `modes.test.paper/submit`).
//! * `confidence` ([`confidence`]): the 1–5 rating scale (`modes.confidence.answer`).
//! * `timed` ([`timed`]): per-card time limit (`study.start { modes.timed }`,
//!   `modes.timed.start/expire`).
//! * `focus` ([`focus`]): distraction-free study screen (settings only; the screen is the
//!   frontend's).
//! * `inline_cards` ([`inline_cards`]): `question :: answer` lines → notes
//!   (`modes.inline.preview/create`), usable during study and reading.
//! * `match` ([`matching`], v1.1): pair terms with definitions against the clock
//!   (`study.start { kind: match }`, `modes.match.start/pick/finish`); practice only.
//!
//! **Building blocks** (CLAUDE.md rule 3): every mode is a session kind or session option of
//! the v0.2 engine. Answers go through `study::ext::answer_core` (scheduler, planning, sibling
//! burying, review log, one undo step through `study.restore`) with [`AnswerHooks`]; the
//! engine calls the four hooks below at start, before a button answer and when it draws the
//! session. Pure logic is `fc_study::modes` and `fc_core::inline_cards`.
//!
//! [`AnswerHooks`]: crate::modules::study::ext::AnswerHooks

pub mod confidence;
pub mod focus;
pub mod inline_cards;
pub mod learn;
pub mod matching;
pub mod practice_test;
pub mod timed;
#[cfg(test)]
mod tests;

use crate::error::{AppError, AppResult};
use crate::handler::CommandCtx;
use crate::module::ModuleDescriptor;
use crate::modules::study::ext::{Session, SessionConfig, View};
use crate::registry::Registry;
use crate::settings::resolve_stored;
use crate::support::module_enabled;
use fc_api::v1::modes::{RatingScale, SessionModesDto, TimeoutAction, TimerDto};
use fc_api::v1::study::{AnswerCard, StartSession, StudyCardDto, StudyKind};
use fc_api::{ApiError, ErrorCode};
use fc_core::card::Card;
use fc_core::rng::Rng;
use fc_core::settings::SettingScope;
use fc_core::Timestamp;
use fc_storage::Reads;
use fc_study::modes::{timed::Timed, ModesConfig};
use fc_study::session::SessionState;
use serde_json::Value;

/// Registers the seven modules.
pub fn register(reg: &mut Registry) {
    learn::register(reg);
    practice_test::register(reg);
    confidence::register(reg);
    timed::register(reg);
    focus::register(reg);
    inline_cards::register(reg);
    matching::register(reg);
}

// ------------------------------------------------------------------ shared helpers

/// A module setting at `scope`: stored value along the scope chain, else the default from
/// the module's descriptor.
pub(crate) fn setting(
    reads: &dyn Reads,
    scope: &SettingScope,
    key: &str,
    descriptor: fn() -> ModuleDescriptor,
) -> AppResult<Value> {
    if let Some((v, _)) = resolve_stored(reads, scope, key)? {
        return Ok(v);
    }
    descriptor()
        .settings
        .into_iter()
        .find(|d| d.key == key)
        .map(|d| d.default)
        .ok_or_else(|| AppError::api(ErrorCode::Internal, format!("unknown setting {key}")))
}

/// `VALIDATION_SESSION_MODE` with a reason.
/// The oplog summary of a mode's answer: its own key, keeping the rating `answer_core`
/// recorded (automation's `card_answered` trigger reads it — phase 4).
pub(crate) fn answer_summary<M: fc_api::ApiMethod>(
    answered: &fc_api::v1::common::I18nMessage,
) -> fc_api::v1::common::I18nMessage {
    let s = crate::support::summary::<M>();
    match answered.params.get("rating") {
        Some(r) => s.param("rating", r),
        None => s,
    }
}

pub(crate) fn session_mode(reason: &str, message: impl Into<String>) -> AppError {
    AppError::Api(
        ApiError::new(ErrorCode::ValidationSessionMode, message).with_detail("reason", reason),
    )
}

/// `CONFLICT_MODULE_DISABLED` unless module `id` is on (read in this transaction).
fn require_module(ctx: &CommandCtx<'_>, id: &str) -> AppResult<()> {
    if module_enabled(ctx.reads(), ctx.registry, id)? {
        Ok(())
    } else {
        Err(crate::registry::module_disabled(id))
    }
}

// ------------------------------------------------------------------ engine hooks

/// `study.start` hook: checks the kind's/options' modules and resolves the mode options
/// persisted with the session (`None` = a plain session).
pub(crate) fn prepare_start(
    ctx: &CommandCtx<'_>,
    req: &StartSession,
) -> AppResult<Option<ModesConfig>> {
    let opts = req.modes.clone().unwrap_or_default();
    match req.kind {
        StudyKind::Learn => require_module(ctx, learn::ID)?,
        StudyKind::Test => require_module(ctx, practice_test::ID)?,
        StudyKind::Match => require_module(ctx, matching::ID)?,
        StudyKind::Normal | StudyKind::Custom | StudyKind::Mistakes => {}
    }
    let card_by_card = matches!(
        req.kind,
        StudyKind::Normal | StudyKind::Custom | StudyKind::Mistakes
    );
    if opts.test.is_some() && req.kind != StudyKind::Test {
        return Err(session_mode("kind", "test options need a test session"));
    }
    let timed = match opts.timed {
        None => None,
        Some(_) if !card_by_card => {
            return Err(session_mode("kind", "learn and test sessions are not timed"))
        }
        Some(t) => {
            require_module(ctx, timed::ID)?;
            let t = Timed {
                seconds: t.seconds,
                on_timeout: match t.on_timeout {
                    TimeoutAction::Again => fc_study::modes::timed::TimeoutAction::Again,
                    TimeoutAction::Skip => fc_study::modes::timed::TimeoutAction::Skip,
                },
            };
            if !t.valid() {
                return Err(crate::support::invalid(format!(
                    "timed.seconds must be {}–{}",
                    fc_study::modes::timed::MIN_SECONDS,
                    fc_study::modes::timed::MAX_SECONDS
                )));
            }
            Some(t)
        }
    };
    let confidence = match opts.rating_scale {
        Some(RatingScale::Confidence) if !card_by_card => {
            return Err(session_mode("kind", "learn and test sessions are judged, not rated"))
        }
        Some(RatingScale::Confidence) => {
            require_module(ctx, confidence::ID)?;
            true
        }
        Some(RatingScale::Buttons) => false,
        None => {
            card_by_card
                && module_enabled(ctx.reads(), ctx.registry, confidence::ID)?
                && confidence::scale_setting(ctx.reads(), req.deck_id)? == RatingScale::Confidence
        }
    };
    Ok((timed.is_some() || confidence).then_some(ModesConfig { timed, confidence }))
}

/// `study.start` hook after the queue is built: learn progress, test paper.
pub(crate) fn init_state(
    v: View<'_>,
    req: &StartSession,
    config: &SessionConfig,
    state: &mut SessionState,
    rng: &mut dyn Rng,
) -> AppResult<()> {
    match config.kind {
        StudyKind::Learn => learn::init(v, state, rng),
        StudyKind::Test => practice_test::init(v, req, state, rng),
        StudyKind::Match => matching::init(v, state),
        StudyKind::Normal | StudyKind::Custom | StudyKind::Mistakes => Ok(()),
    }
}

/// `study.answer` hook: learn cards are judged, not rated (`modes.learn.answer`); a timed
/// card past its time is answered by `modes.timed.expire` (`CONFLICT_TIME_UP`).
pub(crate) fn check_button_answer(ctx: &CommandCtx<'_>, req: &AnswerCard) -> AppResult<()> {
    let session = Session::load(ctx.reads(), ctx.registry, req.session_id)?;
    if session.config.kind == StudyKind::Learn {
        return Err(session_mode(
            "kind",
            "learn cards are answered with modes.learn.answer",
        ));
    }
    timed::check_in_time(&session, req.card_id, ctx.now)
}

/// `SessionDto.modes`: what the study screen needs to draw a mode (`None` for plain
/// sessions). `shown` is the card on screen, `current` its DTO.
pub(crate) fn session_modes(
    v: View<'_>,
    s: &Session,
    shown: Option<&Card>,
    current: Option<&StudyCardDto>,
    _now: Timestamp,
) -> AppResult<Option<SessionModesDto>> {
    let config = s.config.modes.clone().unwrap_or_default();
    let state = s.state.modes.clone().unwrap_or_default();
    let plain = s.config.modes.is_none()
        && s.state.modes.is_none()
        && !matches!(s.config.kind, StudyKind::Learn | StudyKind::Test);
    if plain {
        return Ok(None);
    }
    let confidence = match current {
        Some(c) if config.confidence => confidence::levels(v.reads, &c.intervals)?,
        _ => Vec::new(),
    };
    let timer = config.timed.map(|t| TimerDto {
        seconds: t.seconds,
        on_timeout: match t.on_timeout {
            fc_study::modes::timed::TimeoutAction::Again => TimeoutAction::Again,
            fc_study::modes::timed::TimeoutAction::Skip => TimeoutAction::Skip,
        },
        deadline: current
            .and_then(|c| timed::started(s, c.card_id))
            .map(|at| t.deadline(at)),
    });
    let learn = match s.config.kind {
        StudyKind::Learn => Some(learn::dto(v, s, shown)?),
        _ => None,
    };
    let test = state.test.as_ref().map(practice_test::info);
    Ok(Some(SessionModesDto {
        rating_scale: if config.confidence {
            RatingScale::Confidence
        } else {
            RatingScale::Buttons
        },
        confidence,
        timer,
        timed_out: state.timed_out,
        learn,
        test,
    }))
}
