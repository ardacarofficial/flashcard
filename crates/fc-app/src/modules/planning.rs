//! `planning` module (v0.6, docs/02 §5): **load balancing**, **easy days** and **exam-date
//! mode**. The decisions are the pure `fc_study::planning` functions; this module supplies the
//! inputs (settings, due loads, exams) and owns the exam commands.
//!
//! The study engine calls [`plan_outcome`] after the scheduler, for scheduled answers only:
//! a review card's due day may move inside its fuzz range (to the least loaded / non-easy day)
//! and, for cards of an upcoming exam, into the days right before the exam. Interval previews
//! on the buttons show the scheduler's interval (the plan only shifts it within its range, or
//! pulls it before an exam). Switching the module off restores plain scheduling.
//!
//! v1.3: `planning.forecast` answers "how much will I study a day at this pace?" with the pure
//! `fc_study::planning::forecast` model (the scope's cards, preset weights and retention, the
//! answer time of the last 30 days), and `planning.apply_pace` writes the chosen pace into the
//! deck's preset (one undo step; the forecast's `target` names the preset and decks first).

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
    reg.query::<PlanningForecast>(forecast);
    reg.command::<PlanningApplyPace>(apply_pace);
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
    // `cal.index(t) = (t + shift).div_euclid(day)` and `cal.start(0) = -shift`.
    let shift_ms = -cal.start(0).as_millis();
    let load: HashMap<i64, u32> = reads
        .due_counts_by_day(from, to, shift_ms)?
        .into_iter()
        .collect();
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

// ------------------------------------------------------------------ v1.3 forecast + pace

/// Default forecast length (days): four weeks.
const FORECAST_DAYS: u32 = 28;
/// Longest forecast (days).
const MAX_FORECAST_DAYS: u32 = 365;
/// "In 3 weeks": the outlook week ends on this day offset.
const OUTLOOK_DAY: u32 = 21;
/// Window of the measured answer time (study days).
const TIME_WINDOW_DAYS: i64 = 30;

/// Live decks whose effective preset is `preset` (`None`: decks of `root`'s subtree using the
/// built-in defaults, which a preset assigned to `root` would reach).
fn decks_with_preset(
    reads: &dyn Reads,
    index: &DeckIndex,
    preset: Option<fc_core::PresetId>,
    root: fc_core::DeckId,
) -> AppResult<Vec<DeckBrief>> {
    let candidates: Vec<fc_core::DeckId> = match preset {
        Some(_) => index.live().map(|d| d.id).collect(),
        None => index.live_subtree(root),
    };
    let mut out = Vec::new();
    for id in candidates {
        if crate::modules::presets::effective(reads, id)?.preset_id == preset {
            out.push(DeckBrief {
                id,
                path: index.path(id),
            });
        }
    }
    out.sort_by_key(|a| a.path.to_lowercase());
    Ok(out)
}

/// What `planning.apply_pace` on `deck` changes.
fn pace_target(reads: &dyn Reads, index: &DeckIndex, deck: fc_core::DeckId) -> AppResult<PaceTarget> {
    index.require_live(deck)?;
    let eff = crate::modules::presets::effective(reads, deck)?;
    let preset_name = match eff.preset_id {
        Some(p) => reads.get_preset(p)?.map(|p| p.name),
        None => None,
    };
    Ok(PaceTarget {
        deck: DeckBrief {
            id: deck,
            path: index.path(deck),
        },
        preset_id: eff.preset_id,
        preset_name,
        current_new_per_day: eff.config.new_per_day,
        decks: decks_with_preset(reads, index, eff.preset_id, deck)?,
    })
}

fn validate_pace(n: u32) -> AppResult<()> {
    if n > fc_core::preset::MAX_NEW_PER_DAY {
        return Err(crate::support::invalid("new_per_day must be 0–9999"));
    }
    Ok(())
}

/// A card as the forecast model sees it (suspended cards are left out by the caller).
pub(crate) fn forecast_card(
    c: &Card,
    cal: StudyDay,
    today: i64,
) -> fc_study::planning::forecast::ForecastCard {
    use fc_study::planning::forecast::{ForecastCard, ForecastState};
    let s = &c.scheduling;
    let state = match s.state {
        CardState::New => ForecastState::New,
        CardState::Learning | CardState::Relearning => ForecastState::Learning,
        CardState::Review => ForecastState::Review,
    };
    ForecastCard {
        state,
        due_in: s.due.map_or(0, |d| cal.index(d) - today),
        stability: s.stability,
        difficulty: s.difficulty,
        elapsed: s.last_review.map(|l| today - cal.index(l)),
    }
}

fn forecast(req: ForecastRequest, ctx: &QueryCtx<'_>) -> AppResult<WorkloadForecast> {
    use crate::modules::stats::facts::scope_days;
    use fc_core::stats::iso_date;
    use fc_study::planning::forecast::{self as model, ForecastParams};
    let reads = ctx.tx;
    let cal = calendar(reads, req.tz_offset_mins)?;
    let today = cal.index(ctx.now);
    let index = DeckIndex::load(reads)?;
    if let Some(n) = req.new_per_day {
        validate_pace(n)?;
    }
    let exam_day = match req.exam_date.as_deref().map(str::trim).filter(|d| !d.is_empty()) {
        Some(d) => Some((d.to_owned(), parse_date("exam_date", d)?)),
        None => None,
    };

    // Today's queue (v1.3): the study planner's, so day 0 is the number Home and the
    // launcher show; the collection's limits are those of the decks its queue is made of.
    let tz = crate::modules::study::today::client_tz(reads, req.tz_offset_mins)?;
    let mut planner = crate::modules::study::today::Planner::new(
        crate::modules::study::ext::View::query(ctx),
        ctx.now,
        tz,
    )?;
    let (today_new, today_learning, today_review) =
        crate::modules::study::today::today_counts(&mut planner, req.deck_id)?;
    let roots = match req.deck_id {
        Some(_) => Vec::new(),
        None => planner.collection_roots()?,
    };
    drop(planner);
    // Scope: cards, options, the current pace and the reviews a day.
    let (cards, config, current, target, reviews_per_day) = match req.deck_id {
        Some(d) => {
            let target = pace_target(reads, &index, d)?;
            let cards = reads.study_cards(&fc_storage::repo::StudyCardFilter {
                deck_ids: Some(index.live_subtree(d)),
                ..Default::default()
            })?;
            let config = crate::modules::presets::effective(reads, d)?.config;
            let current = config.new_per_day;
            let reviews = config.reviews_per_day;
            (cards, config, current, Some(target), reviews)
        }
        None => {
            let cards = crate::modules::stats::facts::scope_card_rows(reads, None)?
                .into_iter()
                .filter(|c| !c.suspended)
                .collect::<Vec<_>>();
            let (mut current, mut reviews) = (0u32, 0u32);
            for root in roots {
                let c = crate::modules::presets::effective(reads, root)?.config;
                current = current.saturating_add(c.new_per_day);
                reviews = reviews.saturating_add(c.reviews_per_day);
            }
            let config = fc_core::preset::PresetConfig {
                desired_retention: crate::modules::stats::target_retention(reads, None, &cards)?,
                ..fc_core::preset::PresetConfig::default()
            };
            (cards, config, current, None, reviews)
        }
    };
    let new_per_day = req.new_per_day.unwrap_or(current);

    // Answer time and today's new cards, from the scope's history.
    let ids: Option<Vec<fc_core::CardId>> = req.deck_id.map(|_| cards.iter().map(|c| c.id).collect());
    let (days_hist, _) = scope_days(reads, cal, ids.as_deref(), false)?;
    let (mut ms, mut answers, mut new_today) = (0u64, 0u64, 0u32);
    for d in &days_hist {
        if d.day > today - TIME_WINDOW_DAYS && d.day <= today {
            ms += d.time_ms;
            answers += u64::from(d.reviews);
        }
        if d.day == today {
            new_today = d.new_cards;
        }
    }
    let measured = answers > 0 && ms > 0;
    #[allow(clippy::cast_precision_loss)] // answer counts and ms totals are far below 2^52
    let secs = if measured {
        ms as f64 / answers as f64 / 1000.0
    } else {
        model::DEFAULT_SECS_PER_ANSWER
    };

    let days_left = exam_day.as_ref().map(|(_, d)| d - today);
    let horizon = req
        .days
        .unwrap_or(FORECAST_DAYS)
        .max(days_left.map_or(0, |l| u32::try_from(l + 1).unwrap_or(0)))
        .clamp(1, MAX_FORECAST_DAYS);
    let fc_cards: Vec<_> = cards.iter().map(|c| forecast_card(c, cal, today)).collect();
    let result = model::forecast(
        &fc_cards,
        &ForecastParams {
            weights: config.fsrs_weights.clone(),
            desired_retention: config.desired_retention,
            max_interval: config.maximum_interval_days,
            learning_steps: u32::try_from(config.learning_steps_mins.len()).unwrap_or(0),
            relearning_steps: u32::try_from(config.relearning_steps_mins.len()).unwrap_or(0),
            new_per_day,
            new_done_today: new_today,
            days: horizon,
            secs_per_answer: secs,
            reviews_per_day: Some(reviews_per_day),
            // Day 0: today's queue; another pace changes only its new cards.
            today: Some(model::TodayQueue {
                new: if new_per_day == current {
                    today_new
                } else {
                    new_per_day.saturating_sub(new_today)
                },
                started: today_learning.saturating_add(today_review),
            }),
        },
    );
    let day_dto = |p: &model::ForecastPoint| ForecastDayDto {
        offset: p.offset,
        date: iso_date(today + i64::from(p.offset)),
        cards: p.cards,
        reviews: p.review_cards,
        new_cards: p.introduced,
        minutes: p.minutes,
    };
    let outlook = |offset: u32| {
        let last = u32::try_from(result.days.len().saturating_sub(1)).unwrap_or(0);
        let offset = offset.min(last);
        let (cards_per_day, minutes_per_day) = result.week_ending(offset);
        ForecastOutlook {
            offset,
            date: iso_date(today + i64::from(offset)),
            cards_per_day,
            minutes_per_day,
        }
    };
    let days: Vec<ForecastDayDto> = result.days.iter().map(day_dto).collect();
    #[allow(clippy::cast_precision_loss)] // at most 7 days a chunk
    let weeks: Vec<ForecastWeek> = result
        .days
        .chunks(7)
        .enumerate()
        .map(|(i, w)| {
            let n = w.len().max(1) as f64;
            let first = w.first().map_or(0, |p| p.offset);
            let last = w.last().map_or(0, |p| p.offset);
            ForecastWeek {
                week: u32::try_from(i + 1).unwrap_or(u32::MAX),
                start_date: iso_date(today + i64::from(first)),
                end_date: iso_date(today + i64::from(last)),
                cards_per_day: w.iter().map(|p| p.cards).sum::<f64>() / n,
                minutes_per_day: w.iter().map(|p| p.minutes).sum::<f64>() / n,
                new_cards: w.iter().map(|p| p.introduced).sum(),
            }
        })
        .collect();
    let peak = result
        .days
        .iter()
        .fold(None::<&model::ForecastPoint>, |best, p| match best {
            Some(b) if b.cards >= p.cards => Some(b),
            _ => Some(p),
        })
        .map(day_dto)
        .unwrap_or_else(|| day_dto(&model::ForecastPoint::default()));
    let unseen = cards.iter().filter(|c| c.scheduling.state == CardState::New).count();
    let exam = match (exam_day, days_left) {
        (Some((date, _)), Some(left)) => {
            let before = u32::try_from((left - 1).max(0)).unwrap_or(0);
            let new_left = result.days.get(before as usize).map_or(0.0, |p| p.unseen);
            // A count of cards: finite, ≥ 0 and far below u32::MAX.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let new_left = new_left.round().max(0.0) as u32;
            let needed = (left >= 1).then(|| {
                let total = unseen as u64 + u64::from(new_today);
                let per = total.div_ceil(u64::try_from(left).unwrap_or(1));
                u32::try_from(per)
                    .unwrap_or(u32::MAX)
                    .min(fc_core::preset::MAX_NEW_PER_DAY)
            });
            Some(ExamOutlook {
                date,
                days_left: left,
                new_left,
                new_per_day_needed: needed,
                last_week: outlook(before),
            })
        }
        _ => None,
    };
    Ok(WorkloadForecast {
        deck_id: req.deck_id,
        new_per_day,
        current_new_per_day: current,
        new_done_today: new_today,
        unseen: u32::try_from(unseen).unwrap_or(u32::MAX),
        started: u32::try_from(cards.len() - unseen).unwrap_or(u32::MAX),
        secs_per_answer: secs,
        measured_time: measured,
        retention: config.desired_retention,
        outlook: outlook(OUTLOOK_DAY),
        peak,
        new_finished_on: result
            .new_finished_on
            .map(|d| iso_date(today + i64::from(d))),
        days,
        weeks,
        exam,
        target,
    })
}

/// Runs `M` inside the current transaction and batch (its own oplog entry and events).
fn call<M: fc_api::ApiMethod>(ctx: &mut CommandCtx<'_>, req: &M::Request) -> AppResult<M::Response> {
    let v = ctx.call(&inverse_call::<M>(req)?)?;
    crate::support::from_json(&v)
}

/// `base`, or `base 2`, `base 3`… — the first name no preset has (case-insensitive).
fn free_preset_name(reads: &dyn Reads, base: &str) -> AppResult<String> {
    use fc_core::query::text::fold;
    let taken: Vec<String> = reads.list_presets()?.iter().map(|p| fold(&p.name)).collect();
    let base = base.trim();
    let base = if base.is_empty() { "Plan" } else { base };
    let mut name = base.to_owned();
    let mut n = 2;
    while taken.contains(&fold(&name)) {
        name = format!("{base} {n}");
        n += 1;
    }
    Ok(name)
}

fn apply_pace(req: ApplyPace, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<PaceApplied>> {
    use fc_api::v1::presets::{AssignPreset, CreatePreset, PresetDto, UpdatePreset};
    validate_pace(req.new_per_day)?;
    let index = DeckIndex::load(ctx.reads())?;
    let before = pace_target(ctx.reads(), &index, req.deck_id)?;
    let deck_name = index
        .get(req.deck_id)
        .map(|d| d.name.clone())
        .unwrap_or_default();
    let exam_date = match req.exam_date.as_deref().map(str::trim).filter(|d| !d.is_empty()) {
        Some(d) => {
            parse_date("exam_date", d)?;
            Some(d.to_owned())
        }
        None => None,
    };
    let mark = ctx.oplog_mark();
    let mut created_preset = false;
    match before.preset_id {
        Some(id) => {
            let preset = ctx
                .reads()
                .get_preset(id)?
                .ok_or_else(|| err_id(ErrorCode::NotFoundPreset, format!("preset {id} not found"), id))?;
            if preset.config.new_per_day != req.new_per_day {
                call::<PresetsUpdate>(
                    ctx,
                    &UpdatePreset {
                        id,
                        name: preset.name,
                        config: fc_core::preset::PresetConfig {
                            new_per_day: req.new_per_day,
                            ..preset.config
                        },
                    },
                )?;
            }
        }
        None => {
            let base = req.preset_name.clone().unwrap_or_else(|| deck_name.clone());
            let name = free_preset_name(ctx.reads(), &base)?;
            let p: PresetDto = call::<PresetsCreate>(
                ctx,
                &CreatePreset {
                    name,
                    config: Some(fc_core::preset::PresetConfig {
                        new_per_day: req.new_per_day,
                        ..fc_core::preset::PresetConfig::default()
                    }),
                },
            )?;
            call::<PresetsAssign>(
                ctx,
                &AssignPreset {
                    deck_ids: vec![req.deck_id],
                    preset_id: Some(p.id),
                },
            )?;
            created_preset = true;
        }
    }
    let mut exam = None;
    if let Some(date) = exam_date {
        let exists = ctx
            .reads()
            .list_exams()?
            .iter()
            .any(|e| e.deck_id == Some(req.deck_id) && e.date == date);
        if !exists {
            let name = req
                .exam_name
                .clone()
                .filter(|n| !n.trim().is_empty())
                .unwrap_or_else(|| deck_name.clone());
            exam = Some(call::<PlanningExamsCreate>(
                ctx,
                &CreateExam {
                    name,
                    date,
                    deck_id: Some(req.deck_id),
                    query: None,
                },
            )?);
        }
    }
    let named = summary::<PlanningApplyPace>()
        .param("deck", &before.deck.path)
        .param("count", req.new_per_day);
    ctx.name_nested_since(mark, named.clone());
    let index = DeckIndex::load(ctx.reads())?;
    let target = pace_target(ctx.reads(), &index, req.deck_id)?;
    Ok(CommandOutcome {
        response: PaceApplied {
            target,
            previous_new_per_day: before.current_new_per_day,
            created_preset,
            exam,
        },
        // Undone through the nested entries (same batch).
        inverse: Vec::new(),
        summary: named,
        events: Vec::new(),
    })
}
