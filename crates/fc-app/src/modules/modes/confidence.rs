//! `confidence` module: confidence-based rating (docs/02 §27, Brainscape style). A session
//! started with `modes.rating_scale = confidence` (or by the `confidence.rating_scale` setting
//! of the deck, preset or app) shows five "how sure was I?" buttons; the mapping table
//! (`confidence.map.1` … `.5`) turns the level into one of the four ratings and
//! `modes.confidence.answer` answers through the session engine (`review_log.mode =
//! confidence`, or `trial_confidence`).

use super::{session_mode, setting};
use crate::error::AppResult;
use crate::handler::{CommandCtx, CommandOutcome};
use crate::module::{choice_setting, ModuleDescriptor};
use crate::modules::study::ext::*;
use crate::registry::Registry;
use crate::support::invalid;
use fc_api::v1::methods::ModesConfidenceAnswer;
use fc_api::v1::modes::{ConfidenceAnswer, ConfidenceLevelDto, RatingScale};
use fc_api::v1::study::{AnswerCard, IntervalPreview, SessionDto};
use fc_core::card::Rating;
use fc_core::settings::{SettingScope, SettingScopeKind};
use fc_core::DeckId;
use fc_storage::Reads;
use fc_study::modes::confidence::{self as pure, DEFAULT_MAP, LEVELS};

/// Module id.
pub const ID: &str = "confidence";
/// Default rating scale (`buttons` / `confidence`; app, preset or deck).
pub const RATING_SCALE: &str = "confidence.rating_scale";

const RATING_IDS: [&str; 4] = ["again", "hard", "good", "easy"];

fn map_key(level: u8) -> String {
    format!("confidence.map.{level}")
}

fn rating_id(r: Rating) -> &'static str {
    match r {
        Rating::Again => "again",
        Rating::Hard => "hard",
        Rating::Good => "good",
        Rating::Easy => "easy",
    }
}

/// Descriptor.
pub fn descriptor() -> ModuleDescriptor {
    let mut scale = choice_setting(RATING_SCALE, &["buttons", "confidence"], "buttons");
    scale.scopes = vec![
        SettingScopeKind::App,
        SettingScopeKind::Preset,
        SettingScopeKind::Deck,
    ];
    let mut settings = vec![scale];
    for (i, r) in DEFAULT_MAP.iter().enumerate() {
        let mut def = choice_setting(&map_key(i as u8 + 1), &RATING_IDS, rating_id(*r));
        def.advanced = true;
        settings.push(def);
    }
    ModuleDescriptor {
        id: ID,
        name_key: "modules.confidence.name",
        description_key: "modules.confidence.description",
        enabled_by_default: true,
        can_disable: true,
        settings,
        ui: vec![],
    }
}

/// Registers the module.
pub fn register(reg: &mut Registry) {
    reg.module(descriptor());
    reg.command::<ModesConfidenceAnswer>(answer);
}

/// The rating scale the settings choose for a session on `deck`.
pub(super) fn scale_setting(reads: &dyn Reads, deck: Option<DeckId>) -> AppResult<RatingScale> {
    let scope = deck.map_or(SettingScope::App, SettingScope::Deck);
    Ok(
        match setting(reads, &scope, RATING_SCALE, descriptor)?.as_str() {
            Some("confidence") => RatingScale::Confidence,
            _ => RatingScale::Buttons,
        },
    )
}

/// The mapping table (settings).
pub(crate) fn mapping(reads: &dyn Reads) -> AppResult<[Rating; LEVELS as usize]> {
    let mut map = DEFAULT_MAP;
    for (i, slot) in map.iter_mut().enumerate() {
        let v = setting(reads, &SettingScope::App, &map_key(i as u8 + 1), descriptor)?;
        if let Some(r) = v.as_str().and_then(pure::parse_rating) {
            *slot = r;
        }
    }
    Ok(map)
}

/// The five buttons with the interval each would give (from the four-button previews).
pub(super) fn levels(
    reads: &dyn Reads,
    intervals: &[IntervalPreview],
) -> AppResult<Vec<ConfidenceLevelDto>> {
    let map = mapping(reads)?;
    Ok((1..=LEVELS)
        .filter_map(|level| {
            let rating = pure::rating(level, &map)?;
            let interval_ms = intervals
                .iter()
                .find(|p| p.rating == rating)
                .map_or(0, |p| p.interval_ms);
            Some(ConfidenceLevelDto {
                level,
                rating,
                interval_ms,
            })
        })
        .collect())
}

fn answer(req: ConfidenceAnswer, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<SessionDto>> {
    let session = Session::load(ctx.reads(), ctx.registry, req.session_id)?;
    if !session.config.modes.as_ref().is_some_and(|m| m.confidence) {
        return Err(session_mode(
            "scale",
            "the session is not rated on the confidence scale",
        ));
    }
    let rating = pure::rating(req.level, &mapping(ctx.reads())?)
        .ok_or_else(|| invalid(format!("level must be 1–{LEVELS}")))?;
    let card = AnswerCard {
        session_id: req.session_id,
        card_id: req.card_id,
        rating,
        duration_ms: req.duration_ms,
        reveal_ms: req.reveal_ms,
        typed_answer: None,
    };
    super::check_button_answer(ctx, &card)?;
    let outcome = answer_core(
        ctx,
        &card,
        AnswerHooks {
            confidence: true,
            ..AnswerHooks::default()
        },
    )?;
    Ok(CommandOutcome {
        summary: super::answer_summary::<ModesConfidenceAnswer>(&outcome.summary)
            .param("level", req.level),
        ..outcome
    })
}
