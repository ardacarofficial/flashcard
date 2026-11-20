//! `planning` module (v0.6, docs/02 §5): **load balancing**, **easy days** and **exam-date
//! mode**. The decisions are the pure `fc_study::planning` functions; this module supplies the
//! inputs (settings, due loads, exams) and owns the exam commands.
//!
//! The study engine calls [`plan_outcome`] after the scheduler, for scheduled answers only:
//! a review card's due day may move inside its fuzz range (to the least loaded / non-easy day)
//! and, for cards of an upcoming exam, into the days right before the exam. Interval previews
//! on the buttons show the scheduler's interval (the plan only shifts it within its range, or
//! pulls it before an exam). Switching the module off restores plain scheduling.

use crate::error::{AppError, AppResult};
use crate::handler::{inverse_call, CommandCtx, CommandOutcome, QueryCtx};
use crate::module::{bool_setting, main_menu, ModuleDescriptor};
use crate::modules::stats::facts::{calendar, canonical_query, parse_date, rendered_query};
use crate::registry::Registry;
use crate::support::decks::DeckIndex;
use crate::support::{err_id, module_states, summary};
use fc_api::v1::common::Empty;
use fc_api::v1::events::DomainEvent;
use fc_api::v1::methods::*;
use fc_api::v1::planning::*;
use fc_api::{ApiError, ErrorCode};
use fc_core::card::{Card, CardState};
use fc_core::query::{Filter, Pred};
use fc_core::settings::{SettingDef, SettingKind, SettingScope, SettingScopeKind};
use fc_core::stats::parse_iso_date;
use fc_core::{ExamId, Timestamp};
use fc_providers::strategy::ReviewOutcome;
use fc_storage::repo::ExamRecord;
use fc_storage::Reads;
use fc_study::day::StudyDay;
use fc_study::planning::{plan_interval, EasyDay, ExamDay, PlanOptions};
use std::collections::HashMap;

/// Module id.
pub const ID: &str = "planning";

/// Spread reviews over equivalent days.
pub const LOAD_BALANCE: &str = "planning.load_balance";
/// Easy-day setting keys, Monday first (`normal` / `reduced` / `minimum`).
pub const EASY_DAYS: [&str; 7] = [
    "planning.easy_days.mon",
    "planning.easy_days.tue",
    "planning.easy_days.wed",
    "planning.easy_days.thu",
    "planning.easy_days.fri",
    "planning.easy_days.sat",
    "planning.easy_days.sun",
];
/// Values of an easy-day setting.
pub const EASY_DAY_VALUES: [&str; 3] = ["normal", "reduced", "minimum"];
/// Longest exam name (chars).
const MAX_NAME: usize = 100;

fn easy_day_setting(key: &str) -> SettingDef {
    SettingDef {
        key: key.to_owned(),
        kind: SettingKind::Choice {
            options: EASY_DAY_VALUES.iter().map(|v| (*v).to_owned()).collect(),
        },
        default: serde_json::Value::String("normal".into()),
        // One label per weekday, one shared description.
        label_key: format!("settings.{key}.label"),
        description_key: Some("settings.planning.easy_days.description".to_owned()),
        advanced: false,
        scopes: vec![SettingScopeKind::App],
    }
}

/// Descriptor.
pub fn descriptor() -> ModuleDescriptor {
    let mut settings = vec![bool_setting(LOAD_BALANCE, true, &[SettingScopeKind::App])];
    settings.extend(EASY_DAYS.iter().map(|k| easy_day_setting(k)));
    ModuleDescriptor {
        id: ID,
        name_key: "modules.planning.name",
        description_key: "modules.planning.description",
        enabled_by_default: true,
        can_disable: true,
        settings,
        ui: vec![main_menu("planning.screen", "nav.planning", "calendar", "/planning", 41)],
    }
}

/// Registers the module and its methods.
pub fn register(reg: &mut Registry) {
    reg.module(descriptor());
    reg.query::<PlanningExamsList>(list);
    reg.command::<PlanningExamsCreate>(create);
    reg.command::<PlanningExamsUpdate>(update);
    reg.command::<PlanningExamsDelete>(delete);
    reg.command::<PlanningExamsRestore>(restore);
}

// ------------------------------------------------------------------ planning hook

fn setting_value(reads: &dyn Reads, key: &str) -> AppResult<Option<serde_json::Value>> {
    Ok(crate::settings::resolve_stored(reads, &SettingScope::App, key)?.map(|(v, _)| v))
}

/// Current plan options (settings).
pub(crate) fn options(reads: &dyn Reads) -> AppResult<PlanOptions> {
    let load_balance = setting_value(reads, LOAD_BALANCE)?
        .and_then(|v| v.as_bool())
        .unwrap_or(true);
    let mut easy_days = [EasyDay::Normal; 7];
    for (slot, key) in easy_days.iter_mut().zip(EASY_DAYS) {
        if let Some(v) = setting_value(reads, key)? {
            *slot = EasyDay::parse(v.as_str().unwrap_or_default());
        }
    }
    Ok(PlanOptions {
        load_balance,
        easy_days,
    })
}

/// Filter of an exam's cards.
fn exam_filter(reads: &dyn Reads, e: &ExamRecord, now: Timestamp) -> AppResult<Filter> {
    let mut parts = Vec::new();
    if let Some(d) = e.deck_id {
        let index = DeckIndex::load(reads)?;
        parts.push(Filter::Pred(Pred::Decks(index.live_subtree(d))));
    }
    if let Some(q) = &e.query {
        parts.push(crate::modules::browse::compile(reads, q, now)?);
    }
    Ok(Filter::and(parts))
}

/// The nearest upcoming exam (after `today`) `card` belongs to.
fn exam_of(
    reads: &dyn Reads,
    card: &Card,
    today: i64,
    now: Timestamp,
) -> AppResult<Option<ExamDay>> {
    let mut best: Option<i64> = None;
    for e in reads.list_exams()? {
        let Some(day) = parse_iso_date(&e.date) else { continue };
        if day <= today || best.is_some_and(|b| b <= day) {
            continue;
        }
        // A query that no longer compiles (e.g. its deck was purged) matches nothing.
        let Ok(filter) = exam_filter(reads, &e, now) else { continue };
        let f = Filter::and(vec![filter, Filter::Pred(Pred::CardIds(vec![card.id]))]);
        if !reads.search_card_ids(&f, now)?.is_empty() {
            best = Some(day);
        }
    }
    Ok(best.map(|day| ExamDay { day }))
}

/// Moves the due day of a scheduled review answer per load balancing, easy days and exam mode
/// (see the module docs). No-op when the module is off or the card is not in review.
pub(crate) fn plan_outcome(
    reads: &dyn Reads,
    registry: &Registry,
    card: &Card,
    outcome: &mut ReviewOutcome,
    cal: StudyDay,
    now: Timestamp,
    max_interval: u32,
) -> AppResult<()> {
    if outcome.state.state != CardState::Review || outcome.scheduled_days == 0 {
        return Ok(());
    }
    if !module_states(reads, registry)?
        .get(ID)
        .copied()
        .unwrap_or(false)
    {
        return Ok(());
    }
    let opts = options(reads)?;
    let today = cal.index(now);
    let exam = exam_of(reads, card, today, now)?;
    if !opts.is_active() && exam.is_none() {
        return Ok(());
    }
    let (lo, hi) =
        fc_study::planning::candidate_range(today, outcome.scheduled_days, max_interval, exam);
    let from = cal.start(today + i64::from(lo));
    let to = cal.start(today + i64::from(hi) + 1);
    let mut load: HashMap<i64, u32> = HashMap::new();
    for t in reads.due_times(from, to)? {
        *load.entry(cal.index(t)).or_insert(0) += 1;
    }
    let ivl = plan_interval(
        today,
        outcome.scheduled_days,
        max_interval,
        exam,
        &opts,
        &|d| load.get(&d).copied().unwrap_or(0),
    );
    if ivl != outcome.scheduled_days {
        outcome.scheduled_days = ivl;
        outcome.state.due = Some(cal.start(today + i64::from(ivl)));
    }
    Ok(())
}

// ------------------------------------------------------------------ exams

fn require(reads: &dyn Reads, id: ExamId) -> AppResult<ExamRecord> {
    reads
        .get_exam(id)?
        .ok_or_else(|| err_id(ErrorCode::NotFoundExam, format!("exam {id} not found"), id))
}

fn dto(reads: &dyn Reads, e: &ExamRecord, now: Timestamp) -> AppResult<ExamDto> {
    Ok(ExamDto {
        id: e.id,
        name: e.name.clone(),
        date: e.date.clone(),
        deck_id: e.deck_id,
        query: rendered_query(reads, e.query.as_deref(), now)?,
        created_at: e.created_at,
        updated_at: e.updated_at,
    })
}

fn clean_name(raw: &str) -> AppResult<String> {
    let name = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.is_empty() || name.chars().count() > MAX_NAME {
        return Err(AppError::Api(ApiError::new(
            ErrorCode::ValidationExamName,
            "exam name must be 1–100 characters",
        )));
    }
    Ok(name)
}

/// Validated row from request fields.
#[allow(clippy::too_many_arguments)]
fn record(
    ctx: &CommandCtx<'_>,
    id: ExamId,
    name: &str,
    date: &str,
    deck_id: Option<fc_core::DeckId>,
    query: Option<&str>,
    created_at: Timestamp,
) -> AppResult<ExamRecord> {
    let strict = ctx.batch_kind == fc_api::v1::oplog::OplogKind::Do;
    parse_date("date", date)?;
    if let Some(d) = deck_id {
        DeckIndex::load(ctx.reads())?.require_live(d)?;
    }
    Ok(ExamRecord {
        id,
        name: clean_name(name)?,
        date: date.trim().to_owned(),
        deck_id,
        query: canonical_query(ctx.reads(), query, ctx.now, strict)?,
        created_at,
        updated_at: ctx.now,
    })
}

fn list(req: ListExams, ctx: &QueryCtx<'_>) -> AppResult<ExamList> {
    let reads = ctx.tx;
    let cal = calendar(reads, req.tz_offset_mins)?;
    let today = cal.index(ctx.now);
    let mut items = Vec::new();
    for e in reads.list_exams()? {
        let day = parse_iso_date(&e.date).unwrap_or(today);
        // A scope that no longer resolves matches nothing.
        let cards = match exam_filter(reads, &e, ctx.now) {
            Ok(f) => reads.search_card_ids(&f, ctx.now)?.len(),
            Err(_) => 0,
        };
        items.push(ExamItem {
            exam: dto(reads, &e, ctx.now)?,
            days_left: day - today,
            cards: u32::try_from(cards).unwrap_or(u32::MAX),
        });
    }
    Ok(ExamList { items })
}

fn create(req: CreateExam, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<ExamDto>> {
    let id = ExamId::generate(ctx.ids);
    let e = record(
        ctx,
        id,
        &req.name,
        &req.date,
        req.deck_id,
        req.query.as_deref(),
        ctx.now,
    )?;
    ctx.tx.upsert_exam(&e)?;
    Ok(CommandOutcome {
        response: dto(ctx.reads(), &e, ctx.now)?,
        inverse: vec![inverse_call::<PlanningExamsDelete>(&ExamRef { id })?],
        summary: summary::<PlanningExamsCreate>().param("name", &e.name),
        events: vec![DomainEvent::ExamsChanged],
    })
}

fn update(req: UpdateExam, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<ExamDto>> {
    let old = require(ctx.reads(), req.id)?;
    let e = record(
        ctx,
        old.id,
        &req.name,
        &req.date,
        req.deck_id,
        req.query.as_deref(),
        old.created_at,
    )?;
    ctx.tx.upsert_exam(&e)?;
    Ok(CommandOutcome {
        response: dto(ctx.reads(), &e, ctx.now)?,
        inverse: vec![inverse_call::<PlanningExamsRestore>(&RestoreExam {
            exam: stored_dto(&old),
        })?],
        summary: summary::<PlanningExamsUpdate>().param("name", &e.name),
        events: vec![DomainEvent::ExamsChanged],
    })
}

/// The row as a DTO with its query as stored (canonical), for inverses.
fn stored_dto(e: &ExamRecord) -> ExamDto {
    ExamDto {
        id: e.id,
        name: e.name.clone(),
        date: e.date.clone(),
        deck_id: e.deck_id,
        query: e.query.clone(),
        created_at: e.created_at,
        updated_at: e.updated_at,
    }
}

fn delete(req: ExamRef, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<Empty>> {
    let old = require(ctx.reads(), req.id)?;
    ctx.tx.delete_exam(old.id)?;
    Ok(CommandOutcome {
        response: Empty {},
        inverse: vec![inverse_call::<PlanningExamsRestore>(&RestoreExam {
            exam: stored_dto(&old),
        })?],
        summary: summary::<PlanningExamsDelete>().param("name", &old.name),
        events: vec![DomainEvent::ExamsChanged],
    })
}

fn restore(req: RestoreExam, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<ExamDto>> {
    let d = req.exam;
    let previous = ctx.reads().get_exam(d.id)?;
    let e = ExamRecord {
        id: d.id,
        name: d.name,
        date: d.date,
        deck_id: d.deck_id,
        query: d.query,
        created_at: d.created_at,
        updated_at: d.updated_at,
    };
    ctx.tx.upsert_exam(&e)?;
    // Inverse: back to the previous row (update undone) or gone (delete undone).
    let inverse = match previous {
        Some(p) => inverse_call::<PlanningExamsRestore>(&RestoreExam {
            exam: stored_dto(&p),
        })?,
        None => inverse_call::<PlanningExamsDelete>(&ExamRef { id: e.id })?,
    };
    Ok(CommandOutcome {
        response: dto(ctx.reads(), &e, ctx.now)?,
        inverse: vec![inverse],
        summary: summary::<PlanningExamsRestore>().param("name", &e.name),
        events: vec![DomainEvent::ExamsChanged],
    })
}
