//! `study.options` (v1.3): what the study launcher offers for a deck — per scope the exact
//! card count a session started now would contain, why a scope is empty and when that
//! changes, the unfinished sessions to continue and the kinds that can start.

use super::engine::*;
use super::plan::{client_tz, is_practice, PlanRequest, Planner, SCOPES};
use crate::error::AppResult;
use crate::handler::QueryCtx;
use crate::support::module_enabled;
use fc_api::v1::study::*;

pub(super) fn options(req: StudyOptionsRequest, ctx: &QueryCtx<'_>) -> AppResult<StudyOptions> {
    let tz = client_tz(ctx.tx, req.tz_offset_mins)?;
    let v = View::query(ctx);
    let mut planner = Planner::new(v, ctx.now, tz)?;
    // Fails for a missing or trashed deck (as `study.start` would).
    planner.decks(req.deck_id, req.include_subdecks)?;
    let deck_name = req.deck_id.map(|d| planner.index().path(d));
    let filter = StudyFilter::default();
    let mut scopes = Vec::with_capacity(SCOPES.len());
    for scope in SCOPES {
        let plan_req = PlanRequest::launcher(scope, req.deck_id, req.include_subdecks, &filter);
        let plan = planner.plan(&plan_req)?;
        let count = planner.count(&plan.queue);
        let (empty_reason, next_available_at) = if count == 0 {
            let (r, at) = planner.empty_reason(&plan_req, &plan.queue)?;
            (Some(r), at)
        } else {
            (None, None)
        };
        scopes.push(StudyScopeOption {
            scope,
            count,
            mode: if is_practice(scope) {
                StudyMode::Trial
            } else {
                StudyMode::Scheduled
            },
            empty_reason,
            next_available_at,
        });
    }
    let mut modes = vec![StudyKind::Normal];
    for (kind, module) in [
        (StudyKind::Learn, crate::modules::modes::learn::ID),
        (StudyKind::Test, crate::modules::modes::practice_test::ID),
        (StudyKind::Match, crate::modules::modes::matching::ID),
    ] {
        if module_enabled(ctx.tx, ctx.registry, module)? {
            modes.push(kind);
        }
    }
    let day_start = {
        let day = fc_study::day::StudyDay::new(tz, planner.day_start_hour());
        day.start(day.index(ctx.now))
    };
    let mut resumable = Vec::new();
    for record in ctx.tx.open_sessions()? {
        let s = Session::load(ctx.tx, ctx.registry, record.id)?;
        if req.deck_id.is_some() && s.config.deck_id != req.deck_id {
            continue;
        }
        // A session on a deck that was deleted since is not offered.
        let deck_name = match s.config.deck_id {
            Some(d) => match planner.index().get(d) {
                Some(deck) if deck.deleted_at.is_none() => Some(planner.index().path(d)),
                _ => continue,
            },
            None => None,
        };
        // Today's queue of a flashcard session is the day's: one from an earlier study day
        // is not continued (a new start builds today's).
        if s.config.kind == StudyKind::Normal && s.record.started_at < day_start {
            continue;
        }
        let dto = session_dto(v, &s, ctx.now)?;
        let progress = match s.config.kind {
            StudyKind::Test | StudyKind::Match => crate::modules::modes::mode_progress(&s),
            StudyKind::Normal | StudyKind::Custom | StudyKind::Mistakes | StudyKind::Learn => {
                let c = dto.counts;
                Some((s.state.answered, c.new + c.learning + c.review))
            }
        };
        let Some((answered, remaining)) = progress else {
            continue;
        };
        if remaining == 0 {
            continue;
        }
        resumable.push(ResumableSession {
            session: dto,
            deck_name,
            answered,
            remaining,
        });
    }
    Ok(StudyOptions {
        deck_id: req.deck_id,
        deck_name,
        resumable,
        scopes,
        modes,
    })
}
