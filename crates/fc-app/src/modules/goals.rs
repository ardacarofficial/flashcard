//! `goals` module (v0.6, docs/02 §18): "learn 500 words by the end of the month", "study 30
//! minutes a day". Goals live in the `goals` table (0001); progress and the pace forecast are
//! `fc_core::goal::goal_progress` over the scope's review statistics. Every command is undoable
//! (delete ↔ internal `goals.restore`).

use crate::error::{AppError, AppResult};
use crate::handler::{inverse_call, CommandCtx, CommandOutcome, QueryCtx};
use crate::module::{main_menu, ModuleDescriptor};
use crate::modules::stats::facts::{
    calendar, canonical_query, card_facts, parse_date, rendered_query, scope_card_rows,
    scope_days,
};
use crate::registry::Registry;
use crate::support::{err_id, summary};
use fc_api::v1::common::Empty;
use fc_api::v1::events::DomainEvent;
use fc_api::v1::methods::*;
use fc_api::v1::planning::*;
use fc_api::{ApiError, ErrorCode};
use fc_core::goal::{goal_progress, GoalInputs, GoalKind};
use fc_core::stats::{iso_date, report_card, review_stats};
use fc_core::time::MILLIS_PER_DAY;
use fc_core::{GoalId, Timestamp};
use fc_storage::repo::GoalRecord;
use fc_storage::Reads;

/// Module id.
pub const ID: &str = "goals";

/// Descriptor.
pub fn descriptor() -> ModuleDescriptor {
    ModuleDescriptor {
        id: ID,
        name_key: "modules.goals.name",
        description_key: "modules.goals.description",
        enabled_by_default: true,
        can_disable: true,
        settings: vec![],
        ui: vec![main_menu("goals.screen", "nav.goals", "target", "/goals", 42)],
    }
}

/// Registers the module and its methods.
pub fn register(reg: &mut Registry) {
    reg.module(descriptor());
    reg.query::<GoalsList>(list);
    reg.command::<GoalsCreate>(create);
    reg.command::<GoalsUpdate>(update);
    reg.command::<GoalsDelete>(delete);
    reg.command::<GoalsRestore>(restore);
}

/// Deadlines are dates: stored as UTC midnight of the date's day index.
fn deadline_ts(day: i64) -> Timestamp {
    Timestamp(day.saturating_mul(MILLIS_PER_DAY))
}

fn deadline_day(t: Timestamp) -> i64 {
    t.as_millis().div_euclid(MILLIS_PER_DAY)
}

fn require(reads: &dyn Reads, id: GoalId) -> AppResult<GoalRecord> {
    reads
        .get_goal(id)?
        .ok_or_else(|| err_id(ErrorCode::NotFoundGoal, format!("goal {id} not found"), id))
}

fn kind_of(g: &GoalRecord) -> AppResult<GoalKind> {
    GoalKind::parse(&g.kind)
        .ok_or_else(|| AppError::api(ErrorCode::Internal, format!("unknown goal kind {}", g.kind)))
}

fn dto(reads: &dyn Reads, g: &GoalRecord, now: Timestamp) -> AppResult<GoalDto> {
    Ok(GoalDto {
        query: rendered_query(reads, g.query.as_deref(), now)?,
        ..stored_dto(g)?
    })
}

/// DTO with the query as stored (canonical), for inverses.
fn stored_dto(g: &GoalRecord) -> AppResult<GoalDto> {
    Ok(GoalDto {
        id: g.id,
        kind: kind_of(g)?,
        target: g.target,
        deadline: g.deadline.map(|d| iso_date(deadline_day(d))),
        query: g.query.clone(),
        created_at: g.created_at,
        updated_at: g.updated_at,
    })
}

fn check_target(kind: GoalKind, target: f64) -> AppResult<()> {
    if kind.valid_target(target) {
        Ok(())
    } else {
        Err(AppError::Api(
            ApiError::new(ErrorCode::ValidationGoalTarget, format!("bad target {target}"))
                .with_detail("kind", kind.as_str()),
        ))
    }
}

fn deadline(raw: Option<&str>) -> AppResult<Option<Timestamp>> {
    raw.map(str::trim)
        .filter(|d| !d.is_empty())
        .map(|d| parse_date("deadline", d).map(deadline_ts))
        .transpose()
}

// ------------------------------------------------------------------ queries

fn list(req: ListGoals, ctx: &QueryCtx<'_>) -> AppResult<GoalList> {
    let reads = ctx.tx;
    let cal = calendar(reads, req.tz_offset_mins)?;
    let today = cal.index(ctx.now);
    let mut items = Vec::new();
    for g in reads.list_goals()? {
        let kind = kind_of(&g)?;
        // A scope that no longer compiles counts nothing rather than failing the list.
        let ids = g.query.as_ref().map(|q| {
            crate::modules::browse::compile(reads, q, ctx.now)
                .and_then(|f| Ok(reads.search_card_ids(&f, ctx.now)?))
                .unwrap_or_default()
        });
        let (days, _) = scope_days(reads, cal, ids.as_deref(), false)?;
        let streak = review_stats(&days, today, None).streak;
        let settled_pct = if kind == GoalKind::DeckCompletion {
            let cards = scope_card_rows(reads, ids.as_deref())?;
            report_card(&card_facts(reads, &cards, cal, ctx.now)?).settled_pct
        } else {
            0.0
        };
        let progress = goal_progress(
            kind,
            g.target,
            cal.index(g.created_at),
            g.deadline.map(deadline_day),
            today,
            GoalInputs {
                days: &days,
                streak,
                settled_pct,
            },
        );
        items.push(GoalItem {
            goal: dto(reads, &g, ctx.now)?,
            progress,
        });
    }
    Ok(GoalList { items })
}

// ------------------------------------------------------------------ commands

fn create(req: CreateGoal, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<GoalDto>> {
    check_target(req.kind, req.target)?;
    let g = GoalRecord {
        id: GoalId::generate(ctx.ids),
        kind: req.kind.as_str().to_owned(),
        target: req.target,
        deadline: deadline(req.deadline.as_deref())?,
        query: canonical_query(ctx.reads(), req.query.as_deref(), ctx.now, true)?,
        created_at: ctx.now,
        updated_at: ctx.now,
    };
    ctx.tx.upsert_goal(&g)?;
    Ok(CommandOutcome {
        response: dto(ctx.reads(), &g, ctx.now)?,
        inverse: vec![inverse_call::<GoalsDelete>(&GoalRef { id: g.id })?],
        summary: summary::<GoalsCreate>(),
        events: vec![DomainEvent::GoalsChanged],
    })
}

fn update(req: UpdateGoal, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<GoalDto>> {
    let old = require(ctx.reads(), req.id)?;
    check_target(kind_of(&old)?, req.target)?;
    let strict = ctx.batch_kind == fc_api::v1::oplog::OplogKind::Do;
    let g = GoalRecord {
        target: req.target,
        deadline: deadline(req.deadline.as_deref())?,
        query: canonical_query(ctx.reads(), req.query.as_deref(), ctx.now, strict)?,
        updated_at: ctx.now,
        ..old.clone()
    };
    ctx.tx.upsert_goal(&g)?;
    Ok(CommandOutcome {
        response: dto(ctx.reads(), &g, ctx.now)?,
        inverse: vec![inverse_call::<GoalsRestore>(&RestoreGoal {
            goal: stored_dto(&old)?,
        })?],
        summary: summary::<GoalsUpdate>(),
        events: vec![DomainEvent::GoalsChanged],
    })
}

fn delete(req: GoalRef, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<Empty>> {
    let old = require(ctx.reads(), req.id)?;
    ctx.tx.delete_goal(old.id)?;
    Ok(CommandOutcome {
        response: Empty {},
        inverse: vec![inverse_call::<GoalsRestore>(&RestoreGoal {
            goal: stored_dto(&old)?,
        })?],
        summary: summary::<GoalsDelete>(),
        events: vec![DomainEvent::GoalsChanged],
    })
}

fn restore(req: RestoreGoal, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<GoalDto>> {
    let d = req.goal;
    let previous = ctx.reads().get_goal(d.id)?;
    let g = GoalRecord {
        id: d.id,
        kind: d.kind.as_str().to_owned(),
        target: d.target,
        deadline: d
            .deadline
            .as_deref()
            .map(|s| parse_date("deadline", s).map(deadline_ts))
            .transpose()?,
        query: d.query,
        created_at: d.created_at,
        updated_at: d.updated_at,
    };
    ctx.tx.upsert_goal(&g)?;
    let inverse = match previous {
        Some(p) => inverse_call::<GoalsRestore>(&RestoreGoal {
            goal: stored_dto(&p)?,
        })?,
        None => inverse_call::<GoalsDelete>(&GoalRef { id: g.id })?,
    };
    Ok(CommandOutcome {
        response: dto(ctx.reads(), &g, ctx.now)?,
        inverse: vec![inverse],
        summary: summary::<GoalsRestore>(),
        events: vec![DomainEvent::GoalsChanged],
    })
}
