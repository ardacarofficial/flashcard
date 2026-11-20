//! `stats` module (v0.6, docs/02 §17): statistics, computed in Rust from `review_log` and the
//! card states by the pure functions of `fc_core::stats`.
//!
//! * Every statistic takes a **scope** (deck subtree, tag, query — so a classification value
//!   like `seviye:B1` too) and a date range; `stats.breakdown` splits a scope by the values of
//!   a classification (retention per level, workload per exam…).
//! * Whole-collection review statistics go through a per-study-day **cache**
//!   (`stats_day_cache`, migration 0500): cached day aggregates plus the rows appended since
//!   equal a recomputation (`fc_core::stats::DayAggregate` is a monoid); a changed or removed
//!   row (undo, purge, restore) invalidates it. `stats.refresh_cache` stores the extended cache;
//!   a stale one is never used.
//! * Imported reviews (`mode = imported`) count; practice reviews do not (`trial`, the v0.10
//!   `trial_<mode>` variants and `test`: `fc_storage::repo::is_practice_mode`).
//!
//! `facts` holds the shared machinery the other v0.6 modules (`goals`, `planning`,
//! `optimizer`) use too.

pub(crate) mod facts;
#[cfg(test)]
mod tests;

use crate::error::{AppError, AppResult};
use crate::handler::{CommandCtx, CommandOutcome, QueryCtx};
use crate::module::{main_menu, ModuleDescriptor};
use crate::registry::Registry;
use crate::support::decks::DeckIndex;
use crate::support::{err_id, summary};
use facts::*;
use fc_api::v1::methods::*;
use fc_api::v1::stats::*;
use fc_api::{ApiError, ErrorCode};
use fc_core::attribute::AttributeKind;
use fc_core::query::{AttrAlt, AttrTest, CardNum, CmpOp, Filter, Pred};
use fc_core::settings::{SettingDef, SettingKind, SettingScope, SettingScopeKind};
use fc_core::stats::{
    aggregate_days, card_stats, iso_date, report_card, review_stats, state_counts, CardFact,
};
use fc_core::{CardId, DeckId};
use fc_storage::repo::{CardSearch, CardSort};
use std::collections::{HashMap, HashSet};

/// Module id.
pub const ID: &str = "stats";

/// Lapses that make a card a leech.
pub const LEECH_THRESHOLD: &str = "stats.leech_threshold";

/// Default forecast length (days).
const DEFAULT_FORECAST_DAYS: u32 = 30;
/// Longest forecast (days).
const MAX_FORECAST_DAYS: u32 = 365;

/// Descriptor.
pub fn descriptor() -> ModuleDescriptor {
    ModuleDescriptor {
        id: ID,
        name_key: "modules.stats.name",
        description_key: "modules.stats.description",
        enabled_by_default: true,
        can_disable: true,
        settings: vec![SettingDef {
            key: LEECH_THRESHOLD.to_owned(),
            kind: SettingKind::Integer {
                min: Some(1),
                max: Some(100),
            },
            default: serde_json::Value::from(8),
            label_key: format!("settings.{LEECH_THRESHOLD}.label"),
            description_key: Some(format!("settings.{LEECH_THRESHOLD}.description")),
            advanced: false,
            scopes: vec![SettingScopeKind::App],
        }],
        ui: vec![main_menu("stats.overview", "nav.stats", "chart", "/stats", 40)],
    }
}

/// Registers the module and its methods.
pub fn register(reg: &mut Registry) {
    reg.module(descriptor());
    reg.query::<StatsGetOverview>(overview);
    reg.query::<StatsGetBreakdown>(breakdown);
    reg.query::<StatsLeeches>(leeches);
    reg.command::<StatsApplyToLeeches>(apply_to_leeches);
    reg.query::<StatsDeckReports>(deck_reports);
    reg.command::<StatsRefreshCache>(refresh);
}

fn range_start(today: i64, days: Option<u32>) -> Option<i64> {
    days.filter(|d| *d > 0).map(|d| today - i64::from(d) + 1)
}

// ------------------------------------------------------------------ overview

/// `stats.overview` with an explicit cache choice (tests compare both paths).
pub(crate) fn overview_of(ctx: &QueryCtx<'_>, req: &StatsRequest) -> AppResult<StatsOverview> {
    let reads = ctx.tx;
    let cal = calendar(reads, req.tz_offset_mins)?;
    let today = cal.index(ctx.now);
    let from_day = range_start(today, req.days);
    let ids = scope_cards(reads, &req.scope, ctx.now)?;
    let (days, origin) = scope_days(reads, cal, ids.as_deref(), req.fresh.unwrap_or(false))?;
    let cards = scope_card_rows(reads, ids.as_deref())?;
    let facts = card_facts(reads, &cards, cal, ctx.now)?;
    let forecast = req
        .forecast_days
        .unwrap_or(DEFAULT_FORECAST_DAYS)
        .clamp(1, MAX_FORECAST_DAYS);
    let target_retention = target_retention(reads, req.scope.deck_id, &cards)?;
    Ok(StatsOverview {
        today,
        today_date: iso_date(today),
        from_day,
        reviews: review_stats(&days, today, from_day),
        cards: card_stats(&facts, today, forecast),
        target_retention,
        from_cache: origin.from_cache,
    })
}

/// The retention the scope aims for: a deck scope's effective preset; otherwise the
/// effective presets of the scope's cards (their one value, or the card-weighted mean when
/// decks differ); built-in defaults only for an empty scope.
pub(crate) fn target_retention(
    reads: &dyn fc_storage::Reads,
    deck: Option<fc_core::DeckId>,
    cards: &[fc_core::card::Card],
) -> AppResult<f64> {
    let mut presets = Presets::new(reads);
    if let Some(d) = deck {
        return Ok(presets.of(d)?.desired_retention);
    }
    let mut per_deck: std::collections::BTreeMap<fc_core::DeckId, u64> = Default::default();
    for c in cards {
        *per_deck.entry(c.deck_id).or_default() += 1;
    }
    let mut values: Vec<(f64, u64)> = Vec::with_capacity(per_deck.len());
    for (d, n) in per_deck {
        values.push((presets.of(d)?.desired_retention, n));
    }
    let Some(&(first, _)) = values.first() else {
        return Ok(fc_core::preset::PresetConfig::default().desired_retention);
    };
    if values.iter().all(|(r, _)| *r == first) {
        return Ok(first);
    }
    let total: u64 = values.iter().map(|(_, n)| n).sum();
    #[allow(clippy::cast_precision_loss)] // card counts are far below 2^52
    let mean = values.iter().map(|(r, n)| r * *n as f64).sum::<f64>() / total as f64;
    Ok(mean)
}

fn overview(req: StatsRequest, ctx: &QueryCtx<'_>) -> AppResult<StatsOverview> {
    overview_of(ctx, &req)
}

// ------------------------------------------------------------------ breakdown

fn breakdown(req: StatsBreakdownRequest, ctx: &QueryCtx<'_>) -> AppResult<StatsBreakdown> {
    let reads = ctx.tx;
    let def = reads
        .get_attribute_def(req.attr_id)?
        .filter(|d| d.deleted_at.is_none())
        .ok_or_else(|| {
            err_id(
                ErrorCode::NotFoundAttribute,
                format!("classification {} not found", req.attr_id),
                req.attr_id,
            )
        })?;
    if !matches!(
        def.kind,
        AttributeKind::Enum | AttributeKind::OrderedEnum | AttributeKind::MultiEnum
    ) {
        return Err(AppError::Api(
            ApiError::new(
                ErrorCode::ValidationBreakdownAttribute,
                "only choice classifications can be broken down",
            )
            .with_detail("id", req.attr_id.to_string()),
        ));
    }
    let cal = calendar(reads, req.tz_offset_mins)?;
    let today = cal.index(ctx.now);
    let from_day = range_start(today, req.days);
    let scope = scope_filter(reads, &req.scope, ctx.now)?;
    // Everything of the scope once, then partitioned per option in memory.
    let ids = match &scope {
        Some(f) => Some(reads.search_card_ids(f, ctx.now)?),
        None => None,
    };
    let cards = scope_card_rows(reads, ids.as_deref())?;
    let facts: HashMap<CardId, CardFact> = card_facts(reads, &cards, cal, ctx.now)?
        .into_iter()
        .map(|f| (f.card_id, f))
        .collect();
    let mut reviews: HashMap<CardId, Vec<fc_core::stats::ReviewFact>> = HashMap::new();
    for (_, r) in reads.stats_reviews(0, ids.as_deref())? {
        reviews
            .entry(r.card_id)
            .or_default()
            .push(review_fact(&r, cal));
    }
    let alt = |test: AttrTest| {
        Filter::Pred(Pred::Attr(vec![AttrAlt {
            attr_id: def.id,
            target: def.target,
            test,
        }]))
    };
    let mut options: Vec<_> = def.options.clone();
    options.sort_by_key(|o| o.order);
    let mut groups: Vec<(Option<&fc_core::attribute::AttributeOption>, Vec<CardId>)> = Vec::new();
    for o in &options {
        let matched = matching(reads, alt(AttrTest::Options(vec![o.id])), scope.as_ref(), ctx.now)?;
        groups.push((Some(o), matched));
    }
    let without = matching(
        reads,
        Filter::negate(alt(AttrTest::Any)),
        scope.as_ref(),
        ctx.now,
    )?;
    groups.push((None, without));
    let rows = groups
        .into_iter()
        .map(|(option, ids)| {
            let ids: Vec<CardId> = ids.into_iter().filter(|i| facts.contains_key(i)).collect();
            let group_facts: Vec<CardFact> = ids.iter().filter_map(|i| facts.get(i).cloned()).collect();
            let group_reviews: Vec<fc_core::stats::ReviewFact> = ids
                .iter()
                .filter_map(|i| reviews.get(i))
                .flatten()
                .cloned()
                .collect();
            let stats = review_stats(&aggregate_days(&group_reviews), today, from_day);
            let forecast = card_stats(&group_facts, today, 7);
            BreakdownRow {
                option_id: option.map(|o| o.id),
                label: option.map(|o| o.label.clone()),
                color: option.and_then(|o| o.color.clone()),
                states: state_counts(&group_facts),
                report: report_card(&group_facts),
                success: stats.success,
                true_retention: stats.true_retention,
                reviews: stats.totals.reviews,
                due_today: forecast.forecast.first().map_or(0, |d| d.due),
                due_week: forecast.forecast.iter().map(|d| d.due).sum(),
            }
        })
        .collect();
    Ok(StatsBreakdown {
        attr_id: def.id,
        name: def.name,
        rows,
    })
}

// ------------------------------------------------------------------ leeches

fn leech_threshold(reads: &dyn fc_storage::Reads) -> AppResult<u32> {
    Ok(crate::settings::resolve_stored(reads, &SettingScope::App, LEECH_THRESHOLD)?
        .and_then(|(v, _)| v.as_u64())
        .and_then(|n| u32::try_from(n).ok())
        .filter(|n| *n >= 1)
        .unwrap_or(8))
}

/// The leech rule: cards of `scope` with at least `threshold` (default: the setting) lapses.
fn leech_filter(
    reads: &dyn fc_storage::Reads,
    scope: &StatsScope,
    threshold: Option<u32>,
    now: fc_core::Timestamp,
) -> AppResult<(u32, Filter)> {
    let threshold = match threshold {
        Some(t) if t >= 1 => t,
        Some(_) => return Err(crate::support::invalid("threshold must be at least 1")),
        None => leech_threshold(reads)?,
    };
    let lapses = Filter::Pred(Pred::Num(CardNum::Lapses, CmpOp::Ge, f64::from(threshold)));
    let filter = match scope_filter(reads, scope, now)? {
        Some(s) => Filter::and(vec![s, lapses]),
        None => lapses,
    };
    Ok((threshold, filter))
}

fn leeches(req: LeechRequest, ctx: &QueryCtx<'_>) -> AppResult<LeechList> {
    let reads = ctx.tx;
    let (threshold, filter) = leech_filter(reads, &req.scope, req.threshold, ctx.now)?;
    let page = reads.search_cards(&CardSearch {
        filter,
        now: ctx.now,
        sort: CardSort::Lapses,
        descending: true,
        notes_only: false,
        offset: 0,
        limit: req.limit.unwrap_or(100).clamp(1, 1000),
    })?;
    let index = DeckIndex::load(reads)?;
    Ok(LeechList {
        threshold,
        total: page.total,
        items: page
            .hits
            .into_iter()
            .map(|h| crate::modules::browse::row(&index, h, ctx.now))
            .collect(),
    })
}

/// `stats.apply_to_leeches`: selects every leech of the scope here (not a page of ids from the
/// client) and runs one nested `bulk.*` command on them, checked like a `batch.execute` call
/// (permission, `browse` module switch). The nested command records the single undo step.
fn apply_to_leeches(
    req: ApplyToLeeches,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<LeechActionResult>> {
    use fc_api::v1::bulk::{NoteTags, SetSuspended};
    use fc_api::ApiMethod;
    let (threshold, filter) = leech_filter(ctx.reads(), &req.scope, req.threshold, ctx.now)?;
    let card_ids = ctx.reads().search_card_ids(&filter, ctx.now)?;
    let call = match req.action {
        LeechAction::Suspend => crate::handler::inverse_call::<BulkSetSuspended>(&SetSuspended {
            card_ids: card_ids.clone(),
            suspended: true,
        })?,
        LeechAction::Tag => {
            let mut seen = HashSet::new();
            let note_ids = scope_card_rows(ctx.reads(), Some(&card_ids))?
                .into_iter()
                .map(|c| c.note_id)
                .filter(|n| seen.insert(*n))
                .collect();
            crate::handler::inverse_call::<BulkAddTags>(&NoteTags {
                note_ids,
                tags: vec![req.tag.clone().unwrap_or_else(|| "leech".to_owned())],
            })?
        }
    };
    let cards = u32::try_from(card_ids.len()).unwrap_or(u32::MAX);
    let mut response = LeechActionResult {
        threshold,
        cards,
        changed: 0,
        skipped: 0,
    };
    if !card_ids.is_empty() {
        let method = match req.action {
            LeechAction::Suspend => BulkSetSuspended::NAME,
            LeechAction::Tag => BulkAddTags::NAME,
        };
        let d = *ctx
            .registry
            .descriptor(method)
            .ok_or_else(|| AppError::api(ErrorCode::Internal, format!("{method} not registered")))?;
        let enabled = crate::support::module_states(ctx.reads(), ctx.registry)?
            .get(d.module)
            .copied()
            .unwrap_or(false);
        crate::registry::ensure_invocable(ctx.registry, &d, ctx.actor, enabled)?;
        let r: fc_api::v1::bulk::BulkResult = serde_json::from_value(ctx.call(&call)?)
            .map_err(|e| AppError::api(ErrorCode::Internal, format!("bulk result: {e}")))?;
        response.changed = r.changed;
        response.skipped = r.skipped;
    }
    Ok(CommandOutcome {
        response,
        inverse: Vec::new(),
        summary: summary::<StatsApplyToLeeches>().param("count", cards),
        events: Vec::new(),
    })
}

// ------------------------------------------------------------------ deck reports

fn deck_reports(req: DeckReportsRequest, ctx: &QueryCtx<'_>) -> AppResult<DeckReports> {
    let reads = ctx.tx;
    let cal = calendar(reads, req.tz_offset_mins)?;
    let today = cal.index(ctx.now);
    let index = DeckIndex::load(reads)?;
    let cards = scope_card_rows(reads, None)?;
    let facts = card_facts(reads, &cards, cal, ctx.now)?;
    // Each card counts for its deck and every ancestor.
    let mut per_deck: HashMap<DeckId, Vec<CardFact>> = HashMap::new();
    for (card, fact) in cards.iter().zip(facts) {
        let chain = index.ancestry(card.deck_id).unwrap_or_default();
        let mut seen = HashSet::new();
        for d in chain {
            if seen.insert(d) {
                per_deck.entry(d).or_default().push(fact.clone());
            }
        }
    }
    let mut out = Vec::new();
    // Depth-first in sibling order (tree order).
    let mut order = Vec::new();
    fn walk(index: &DeckIndex, parent: Option<DeckId>, order: &mut Vec<DeckId>) {
        for d in index.children(parent) {
            order.push(d.id);
            walk(index, Some(d.id), order);
        }
    }
    walk(&index, None, &mut order);
    for id in order {
        let Some(d) = index.get(id) else { continue };
        let facts = per_deck.remove(&id).unwrap_or_default();
        let s = card_stats(&facts, today, 1);
        out.push(DeckReport {
            deck_id: id,
            path: index.path(id),
            parent_id: d.parent_id,
            report: s.report,
            states: s.states,
            due_today: s.forecast.first().map_or(0, |f| f.due),
        });
    }
    Ok(DeckReports { items: out })
}

// ------------------------------------------------------------------ cache

fn refresh(
    req: RefreshStatsCache,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<StatsCacheInfo>> {
    let cal = calendar(ctx.reads(), req.tz_offset_mins)?;
    let (rebuilt, days, folded) = refresh_cache(ctx.tx, cal, ctx.now)?;
    Ok(CommandOutcome {
        response: StatsCacheInfo {
            rebuilt,
            days,
            folded,
        },
        // Derived data: nothing to undo.
        inverse: Vec::new(),
        summary: summary::<StatsRefreshCache>(),
        events: Vec::new(),
    })
}
