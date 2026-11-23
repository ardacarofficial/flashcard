//! `match` module (v1.1, docs/11 §3): pair terms with definitions against the clock.
//! `study.start { kind: match }` picks the cards like a custom session (deck, query, filter)
//! and keeps them as the session's pool; `modes.match.start` deals a round of up to six pairs
//! from it (shuffled with a generator seeded from the session and round, so a round is
//! reproducible), `modes.match.pick` checks two tiles, `modes.match.finish` times the round on
//! the backend clock, keeps the deck's best time (the `match.best_ms` setting at deck scope,
//! app scope for the whole collection) and logs one review per card with
//! `review_log.mode = match`.
//!
//! **Practice:** a match never changes the schedule; its reviews are not counted by daily
//! limits, statistics or FSRS optimization (`fc_storage::repo::is_practice_mode`). Dealing and
//! picking are session bookkeeping; finishing is one undo step (reviews removed, round
//! unfinished, best time restored).

use super::session_mode;
use crate::error::AppResult;
use crate::handler::{inverse_call, CommandCtx, CommandOutcome};
use crate::module::{main_menu, ModuleDescriptor};
use crate::modules::study::ext::*;
use crate::registry::Registry;
use crate::support::content;
use crate::support::notes::{require_note, require_notetype};
use crate::support::summary;
use fc_api::v1::cardtypes::CardInteraction;
use fc_api::v1::methods::{
    ModesMatchFinish, ModesMatchPick, ModesMatchStart, SettingsReset, SettingsSet, StudyRestore,
};
use fc_api::v1::modes::{
    MatchPick, MatchPickResult, MatchRef, MatchResultDto, MatchRoundDto, MatchSide, MatchTileDto,
};
use fc_api::v1::settings::{ResetSetting, SetSetting};
use fc_api::v1::study::{RestoreStudy, StudyKind};
use fc_core::rng::SplitMix64;
use fc_core::settings::{SettingDef, SettingKind, SettingScope, SettingScopeKind};
use fc_core::ReviewLogId;
use fc_storage::repo::ReviewLogRecord;
use fc_study::modes::matching::{self as pure, MatchItem, MatchState, PickError, Round, Side};
use fc_study::modes::ModesState;
use fc_study::session::SessionState;
use fc_study::text::html_to_text;

/// Module id.
pub const ID: &str = "match";
/// Best finished round time, ms, per deck (deck scope; app scope = whole collection).
pub const BEST_MS: &str = "match.best_ms";
/// `review_log.mode` of match answers (practice).
pub const REVIEW_MODE: &str = "match";

/// Descriptor.
pub fn descriptor() -> ModuleDescriptor {
    ModuleDescriptor {
        id: ID,
        name_key: "modules.match.name",
        description_key: "modules.match.description",
        enabled_by_default: true,
        can_disable: true,
        settings: vec![SettingDef {
            key: BEST_MS.to_owned(),
            kind: SettingKind::Integer {
                min: Some(0),
                max: None,
            },
            default: serde_json::Value::from(0),
            label_key: format!("settings.{BEST_MS}.label"),
            description_key: Some(format!("settings.{BEST_MS}.description")),
            advanced: true,
            scopes: vec![SettingScopeKind::App, SettingScopeKind::Deck],
        }],
        ui: vec![main_menu("study.match", "nav.match", "match", "/match", 8)],
    }
}

/// Registers the module.
pub fn register(reg: &mut Registry) {
    reg.module(descriptor());
    reg.command::<ModesMatchStart>(start);
    reg.command::<ModesMatchPick>(pick);
    reg.command::<ModesMatchFinish>(finish);
}

/// `study.start` hook: the queued cards become the pool (term = the `term`/`surface` field and
/// definition = the asked-for or `meaning` field by role, else question / answer); the queue is emptied (a match is played by rounds, not card by card).
pub(super) fn init(v: View<'_>, state: &mut SessionState) -> AppResult<()> {
    let mut pool = Vec::new();
    for e in &state.queue {
        if pool.len() >= pure::MAX_POOL {
            break;
        }
        let card = require_live_card(v.reads, e.card_id)?;
        // v1.3: a vocabulary card's tiles are its word and its meaning (field roles), never
        // the rendered front (which may carry the sentence) or the whole back.
        let note = require_note(v.reads, card.note_id)?;
        let nt = require_notetype(v.reads, note.notetype_id)?;
        let (term, definition) = match content::term_and_definition(&nt, &note, &card.template_key) {
            Some(pair) => pair,
            None => {
                let r = render_for_study(v, &card)?;
                let definition = match &r.card.sides.interaction {
                    CardInteraction::Choice { options, correct } => options
                        .get(*correct as usize)
                        .map(|o| html_to_text(o))
                        .unwrap_or_default(),
                    CardInteraction::SelfGrade | CardInteraction::Type { .. } => r.expected.clone(),
                };
                (html_to_text(&r.card.sides.question), definition)
            }
        };
        pool.push(MatchItem {
            card_id: card.id,
            term: term.trim().to_owned(),
            definition: definition.trim().to_owned(),
        });
    }
    state.queue.clear();
    state.modes = Some(ModesState {
        matching: Some(MatchState {
            pool,
            ..MatchState::default()
        }),
        ..ModesState::default()
    });
    Ok(())
}

fn match_state(s: &Session) -> AppResult<&MatchState> {
    s.state
        .modes
        .as_ref()
        .and_then(|m| m.matching.as_ref())
        .filter(|_| s.config.kind == StudyKind::Match)
        .ok_or_else(|| session_mode("not_match", "not a match session"))
}

fn match_state_mut(s: &mut Session) -> AppResult<&mut MatchState> {
    match_state(s)?;
    s.state
        .modes
        .as_mut()
        .and_then(|m| m.matching.as_mut())
        .ok_or_else(|| session_mode("not_match", "not a match session"))
}

/// Where the session's best time is kept.
fn best_scope(s: &Session) -> SettingScope {
    s.config
        .deck_id
        .map_or(SettingScope::App, SettingScope::Deck)
}

fn best_ms(ctx: &CommandCtx<'_>, scope: &SettingScope) -> AppResult<Option<u32>> {
    Ok(ctx
        .reads()
        .get_setting(scope, BEST_MS)?
        .and_then(|v| v.as_u64())
        .filter(|ms| *ms > 0)
        .map(|ms| u32::try_from(ms).unwrap_or(u32::MAX)))
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

fn round_dto(s: &Session, m: &MatchState, best: Option<u32>) -> MatchRoundDto {
    let cards = count(pure::usable(&m.pool).len());
    let Some(r) = &m.round else {
        return MatchRoundDto {
            session_id: s.record.id,
            round: m.rounds,
            tiles: Vec::new(),
            pairs: 0,
            matched: 0,
            mistakes: 0,
            started_at: s.record.started_at,
            finished_ms: None,
            best_ms: best,
            cards,
        };
    };
    MatchRoundDto {
        session_id: s.record.id,
        round: m.rounds,
        tiles: r
            .tiles
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let pair = &r.pairs[t.pair as usize];
                MatchTileDto {
                    id: count(i),
                    side: match t.side {
                        Side::Term => MatchSide::Term,
                        Side::Definition => MatchSide::Definition,
                    },
                    text: match t.side {
                        Side::Term => pair.item.term.clone(),
                        Side::Definition => pair.item.definition.clone(),
                    },
                    matched: pair.matched,
                }
            })
            .collect(),
        pairs: count(r.pairs.len()),
        matched: count(r.matched()),
        mistakes: r.mistakes,
        started_at: r.started_at,
        finished_ms: r.finished_ms,
        best_ms: best,
        cards,
    }
}

fn start(req: MatchRef, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<MatchRoundDto>> {
    let mut session = Session::load(ctx.reads(), ctx.registry, req.session_id)?;
    session.require_open()?;
    let best = best_ms(ctx, &best_scope(&session))?;
    // Seeded by session and round: a round is reproducible from the stored session.
    let seed = session.record.id.0.as_u64_pair().1;
    let now = ctx.now;
    let m = match_state_mut(&mut session)?;
    let round_seed = seed ^ u64::from(m.rounds + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    // Fewer than two usable cards: no round (an empty board says so).
    if let Some(round) = pure::deal(&m.pool, now, &mut SplitMix64::new(round_seed)) {
        m.rounds += 1;
        m.round = Some(round);
        session.state.last_event_at = Some(now);
        session.save(ctx.tx)?;
    }
    let m = match_state(&session)?;
    let dto = round_dto(&session, m, best);
    Ok(CommandOutcome {
        response: dto,
        inverse: Vec::new(),
        summary: summary::<ModesMatchStart>(),
        events: Vec::new(),
    })
}

fn open_round(m: &mut MatchState) -> AppResult<&mut Round> {
    m.round
        .as_mut()
        .ok_or_else(|| session_mode("no_round", "deal a round first (modes.match.start)"))
}

fn pick_error(e: PickError) -> crate::error::AppError {
    match e {
        PickError::OutOfRange => crate::support::invalid("no such tile"),
        PickError::SameSide => session_mode("same_side", "pick a term and a definition"),
        PickError::Matched => session_mode("matched", "the tile was matched already"),
        PickError::Finished => session_mode("finished", "the round is over"),
    }
}

fn pick(req: MatchPick, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<MatchPickResult>> {
    let mut session = Session::load(ctx.reads(), ctx.registry, req.session_id)?;
    session.require_open()?;
    let best = best_ms(ctx, &best_scope(&session))?;
    let round = open_round(match_state_mut(&mut session)?)?;
    let correct = round
        .pick(req.first as usize, req.second as usize, ctx.now)
        .map_err(pick_error)?;
    let complete = round.complete();
    session.state.last_event_at = Some(ctx.now);
    session.save(ctx.tx)?;
    let m = match_state(&session)?;
    Ok(CommandOutcome {
        response: MatchPickResult {
            correct,
            complete,
            round: round_dto(&session, m, best),
        },
        inverse: Vec::new(),
        summary: summary::<ModesMatchPick>(),
        events: Vec::new(),
    })
}

fn finish(req: MatchRef, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<MatchResultDto>> {
    let mut session = Session::load(ctx.reads(), ctx.registry, req.session_id)?;
    session.require_open()?;
    let scope = best_scope(&session);
    let previous = best_ms(ctx, &scope)?;
    let stored_best = ctx.reads().get_setting(&scope, BEST_MS)?;
    let now = ctx.now;
    let round = open_round(match_state_mut(&mut session)?)?;
    if round.finished_ms.is_some() {
        return Err(session_mode("finished", "the round is over"));
    }
    if !round.complete() {
        return Err(session_mode("incomplete", "match every pair first"));
    }
    let time = round.elapsed_ms(now);
    round.finished_ms = Some(time);
    let round = round.clone();
    // One practice review per card: the card's state as it is (a match never schedules).
    let per = time / count(round.pairs.len().max(1));
    let sched = scheduler(ctx.reads())?;
    let mut written = Vec::new();
    for p in &round.pairs {
        let Some(card) = ctx.reads().get_card(p.item.card_id)? else {
            continue;
        };
        if card.deleted_at.is_some() {
            continue;
        }
        let rating = pure::rating(p);
        let params = session.config.params(ctx.reads(), card.deck_id)?;
        let elapsed = sched
            .schedule(&card.scheduling, rating, now, &params)
            .map_err(scheduler_error)?
            .elapsed_days;
        let review = ReviewLogRecord {
            id: ReviewLogId::generate(ctx.ids),
            card_id: card.id,
            session_id: Some(session.record.id),
            rating,
            state_before: card.scheduling.state,
            state_after: card.scheduling.state,
            scheduled_days: 0,
            elapsed_days: elapsed,
            duration_ms: per,
            answer_duration_ms: None,
            hints_used: 0,
            mode: REVIEW_MODE.to_owned(),
            stability_after: card.scheduling.stability,
            difficulty_after: card.scheduling.difficulty,
            created_at: now,
        };
        ctx.tx.insert_review_log(&review)?;
        written.push(review.id);
    }
    let pairs = count(round.pairs.len());
    let clean = count(round.pairs.iter().filter(|p| p.misses == 0).count());
    session.state.answered = session.state.answered.saturating_add(pairs);
    session.state.correct = session.state.correct.saturating_add(clean);
    session.state.time_ms = session.state.time_ms.saturating_add(u64::from(time));
    session.state.last_event_at = Some(now);
    let before = session.save(ctx.tx)?.snapshot()?;
    let mut inverse = vec![inverse_call::<StudyRestore>(&RestoreStudy {
        cards: Vec::new(),
        session: Some(before),
        remove_reviews: written,
        add_reviews: Vec::new(),
        ..Default::default()
    })?];
    let new_best = previous.is_none_or(|b| time < b);
    if new_best {
        ctx.tx
            .set_setting(&scope, BEST_MS, &serde_json::Value::from(time), now)?;
        inverse.push(match stored_best {
            Some(value) => inverse_call::<SettingsSet>(&SetSetting {
                scope: scope.clone(),
                key: BEST_MS.to_owned(),
                value,
            })?,
            None => inverse_call::<SettingsReset>(&ResetSetting {
                scope: scope.clone(),
                key: BEST_MS.to_owned(),
            })?,
        });
    }
    let best = if new_best { Some(time) } else { previous };
    let m = match_state(&session)?;
    Ok(CommandOutcome {
        response: MatchResultDto {
            time_ms: time,
            mistakes: round.mistakes,
            pairs,
            previous_best_ms: previous,
            new_best,
            round: round_dto(&session, m, best),
        },
        inverse,
        summary: summary::<ModesMatchFinish>().param("ms", time),
        events: Vec::new(),
    })
}
