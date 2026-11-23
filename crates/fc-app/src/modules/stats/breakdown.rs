//! `stats.breakdown` (docs/02 §8.1): a scope split by the values of a choice classification.
//!
//! The cards of every value come from the search; the reviews are folded into per-value day
//! counts (answers and passes per review kind — all a row of the breakdown shows). v1.0: those
//! day counts are cached (`stats.refresh_cache` stores them for the whole collection, one
//! cache per classification and calendar) and used while
//!
//! * the review log only grew since (the overview cache's rule: same generation, same rows up
//!   to the cached sequence; appended rows are folded on top), and
//! * the breakdown's card sets have the cached **fingerprint** — which cards had which value
//!   when the cache was built. Any classification edit, trashed or restored card, or other
//!   scope changes the fingerprint, so a stale cache is never used.
//!
//! Cache + tail equals a recomputation (`stats/tests.rs`).

use super::facts::*;
use super::range_start;
use crate::error::{AppError, AppResult};
use crate::handler::QueryCtx;
use crate::support::err_id;
use fc_api::v1::stats::*;
use fc_api::{ApiError, ErrorCode};
use fc_core::attribute::{AttributeDef, AttributeKind, AttributeOption};
use fc_core::query::{AttrAlt, AttrTest, Filter, Pred};
use fc_core::stats::{
    card_stats, report_card, review_stats, state_counts, CardFact, DayAggregate, PassCount,
    ReviewKind,
};
use fc_core::{CardId, Timestamp};
use fc_storage::repo::StatsCacheRecord;
use fc_storage::{Reads, Writes};
use fc_study::day::StudyDay;
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};

/// Answers / passes of one day by review kind: learn, young, mature, relearn.
type Kinds = [PassCount; 4];

/// Per value (and "no value", last): day → counts.
type GroupDays = Vec<BTreeMap<i64, Kinds>>;

fn kind_index(k: ReviewKind) -> usize {
    match k {
        ReviewKind::Learn => 0,
        ReviewKind::Young => 1,
        ReviewKind::Mature => 2,
        ReviewKind::Relearn => 3,
    }
}

/// The cards of a breakdown: per value (in option order, then "no value") the scope's live
/// cards that have it, and the card facts of the scope.
struct Parts {
    groups: Vec<(Option<AttributeOption>, Vec<CardId>)>,
    facts: HashMap<CardId, CardFact>,
    /// The scope's cards (`None` = whole collection).
    ids: Option<Vec<CardId>>,
}

impl Parts {
    fn load(
        reads: &dyn Reads,
        def: &AttributeDef,
        scope: &StatsScope,
        cal: StudyDay,
        now: Timestamp,
    ) -> AppResult<Self> {
        let scope = scope_filter(reads, scope, now)?;
        // Everything of the scope once, then partitioned per option.
        let ids = match &scope {
            Some(f) => Some(reads.search_card_ids(f, now)?),
            None => None,
        };
        let cards = scope_card_rows(reads, ids.as_deref())?;
        let facts: HashMap<CardId, CardFact> = card_facts(reads, &cards, cal, now)?
            .into_iter()
            .map(|f| (f.card_id, f))
            .collect();
        let alt = |test: AttrTest| {
            Filter::Pred(Pred::Attr(vec![AttrAlt {
                attr_id: def.id,
                target: def.target,
                test,
            }]))
        };
        let mut options: Vec<AttributeOption> = def.options.clone();
        options.sort_by_key(|o| o.order);
        let mut groups = Vec::with_capacity(options.len() + 1);
        for o in options {
            let matched = matching(reads, alt(AttrTest::Options(vec![o.id])), scope.as_ref(), now)?;
            groups.push((Some(o), matched));
        }
        let without = matching(reads, Filter::negate(alt(AttrTest::Any)), scope.as_ref(), now)?;
        groups.push((None, without));
        // Only cards with facts (live cards of the scope) count.
        for (_, g) in &mut groups {
            g.retain(|id| facts.contains_key(id));
            g.sort_unstable();
        }
        Ok(Self { groups, facts, ids })
    }

    /// Groups of each card (a card of a multi-choice value is in several).
    fn members(&self) -> HashMap<CardId, Vec<usize>> {
        let mut member: HashMap<CardId, Vec<usize>> = HashMap::new();
        for (g, (_, ids)) in self.groups.iter().enumerate() {
            for id in ids {
                member.entry(*id).or_default().push(g);
            }
        }
        member
    }

    /// Which cards are in which group, as a short text.
    fn fingerprint(&self) -> String {
        self.fingerprint_except(&HashSet::new())
    }

    /// [`Self::fingerprint`] leaving out `skip`.
    fn fingerprint_except(&self, skip: &HashSet<CardId>) -> String {
        let mut bytes = Vec::new();
        for (option, ids) in &self.groups {
            bytes.extend_from_slice(group_key(option.as_ref()).as_bytes());
            bytes.push(0xff);
            for id in ids.iter().filter(|id| !skip.contains(id)) {
                bytes.extend_from_slice(id.0.as_bytes());
            }
            bytes.push(0xfe);
        }
        // Two different hashes of the same bytes: 128 bits plus the length.
        let flipped: Vec<u8> = bytes.iter().map(|b| b ^ 0x5a).collect();
        format!(
            "{}-{:016x}{:016x}",
            bytes.len(),
            fc_core::note::fnv1a64(&bytes),
            fc_core::note::fnv1a64(&flipped)
        )
    }
}

/// The fingerprint a cache of `parts` is stored with (v1.4 Z2): `m<card mark>/` + the
/// fingerprint of the cards inserted up to the mark, where the store keeps an insertion
/// order. A card added later has no review in the cache (its reviews came after it), so a
/// later check leaves it out: capturing a word no longer throws the cache away.
fn stored_fingerprint(reads: &dyn Reads, parts: &Parts) -> AppResult<String> {
    Ok(match reads.card_seq_mark()? {
        Some(mark) => format!("m{mark}/{}", parts.fingerprint()),
        None => parts.fingerprint(),
    })
}

/// Whether the cards of `parts` are grouped as when `cache` was stored: every card the cache
/// can hold reviews of has the group it had, and no card inserted since has a review in it.
fn same_groups(reads: &dyn Reads, parts: &Parts, cache: &StatsCacheRecord) -> AppResult<bool> {
    let Some(stored) = cache.fingerprint.as_deref() else {
        return Ok(false);
    };
    let Some((mark, fingerprint)) = stored
        .strip_prefix('m')
        .and_then(|s| s.split_once('/'))
        .and_then(|(m, f)| Some((m.parse::<i64>().ok()?, f)))
    else {
        return Ok(stored == parts.fingerprint());
    };
    let added = reads.cards_after_seq(mark)?;
    if reads.review_count_upto_for(cache.upto_seq, &added)? != 0 {
        return Ok(false);
    }
    let added: HashSet<CardId> = added.into_iter().collect();
    Ok(fingerprint == parts.fingerprint_except(&added))
}

/// Cache key of a group: the option id, `""` for "no value".
fn group_key(option: Option<&AttributeOption>) -> String {
    option.map(|o| o.id.to_string()).unwrap_or_default()
}

/// Calendar key of the breakdown cache of `def`.
fn cache_key(def: &AttributeDef, cal: StudyDay) -> String {
    format!("breakdown:{}:{}", def.id, calendar_key(cal))
}

/// Folds the scope's review rows after `after_seq` into `days`.
fn fold(
    reads: &dyn Reads,
    parts: &Parts,
    cal: StudyDay,
    after_seq: i64,
    days: &mut GroupDays,
) -> AppResult<()> {
    let member = parts.members();
    reads.for_each_stats_review_row(after_seq, parts.ids.as_deref(), &mut |_, r| {
        let Some(gs) = member.get(&r.card_id) else {
            return;
        };
        let f = review_fact_of_row(r, cal);
        let (k, passed) = (kind_index(f.kind()), f.passed());
        for g in gs {
            let c = &mut days[*g].entry(f.day).or_default()[k];
            c.count = c.count.saturating_add(1);
            c.pass = c.pass.saturating_add(u32::from(passed));
        }
    })?;
    Ok(())
}

fn internal(e: impl std::fmt::Display) -> AppError {
    AppError::api(ErrorCode::Internal, e.to_string())
}

/// Cache rows: per day, `{group key: [count, pass] × 4 kinds}`.
fn encode(parts: &Parts, days: &GroupDays) -> AppResult<Vec<(i64, Value)>> {
    let mut by_day: BTreeMap<i64, serde_json::Map<String, Value>> = BTreeMap::new();
    for ((option, _), g) in parts.groups.iter().zip(days) {
        for (day, kinds) in g {
            let flat: Vec<u32> = kinds.iter().flat_map(|c| [c.count, c.pass]).collect();
            let v = serde_json::to_value(flat).map_err(internal)?;
            by_day
                .entry(*day)
                .or_default()
                .insert(group_key(option.as_ref()), v);
        }
    }
    Ok(by_day
        .into_iter()
        .map(|(d, m)| (d, Value::Object(m)))
        .collect())
}

fn decode(parts: &Parts, rows: &[(i64, Value)]) -> AppResult<GroupDays> {
    let index: HashMap<String, usize> = parts
        .groups
        .iter()
        .enumerate()
        .map(|(i, (o, _))| (group_key(o.as_ref()), i))
        .collect();
    let mut days: GroupDays = vec![BTreeMap::new(); parts.groups.len()];
    for (day, v) in rows {
        let Value::Object(m) = v else {
            return Err(internal("breakdown cache: not an object"));
        };
        for (key, flat) in m {
            let g = *index
                .get(key)
                .ok_or_else(|| internal("breakdown cache: unknown group"))?;
            let flat: Vec<u32> = serde_json::from_value(flat.clone()).map_err(internal)?;
            if flat.len() != 8 {
                return Err(internal("breakdown cache: bad counts"));
            }
            let mut kinds = Kinds::default();
            for (k, c) in kinds.iter_mut().enumerate() {
                *c = PassCount {
                    count: flat[2 * k],
                    pass: flat[2 * k + 1],
                };
            }
            days[g].insert(*day, kinds);
        }
    }
    Ok(days)
}

/// The cached day counts of `parts` plus the rows appended since, when the cache is valid.
fn cached(
    reads: &dyn Reads,
    def: &AttributeDef,
    parts: &Parts,
    cal: StudyDay,
) -> AppResult<Option<GroupDays>> {
    let Some(cache) = reads.stats_cache(&cache_key(def, cal))? else {
        return Ok(None);
    };
    let mark = reads.review_log_mark()?;
    let valid = cache.generation == mark.generation
        && cache.upto_seq <= mark.max_seq
        && reads.review_count_upto(cache.upto_seq)? == cache.row_count
        && same_groups(reads, parts, &cache)?;
    if !valid {
        return Ok(None);
    }
    let mut days = decode(parts, &cache.days)?;
    fold(reads, parts, cal, cache.upto_seq, &mut days)?;
    Ok(Some(days))
}

fn fresh(reads: &dyn Reads, parts: &Parts, cal: StudyDay) -> AppResult<GroupDays> {
    let mut days: GroupDays = vec![BTreeMap::new(); parts.groups.len()];
    fold(reads, parts, cal, 0, &mut days)?;
    Ok(days)
}

/// A choice classification by id.
fn choice_def(reads: &dyn Reads, id: fc_core::AttributeDefId) -> AppResult<AttributeDef> {
    let def = reads
        .get_attribute_def(id)?
        .filter(|d| d.deleted_at.is_none())
        .ok_or_else(|| {
            err_id(
                ErrorCode::NotFoundAttribute,
                format!("classification {id} not found"),
                id,
            )
        })?;
    if !is_choice(&def) {
        return Err(AppError::Api(
            ApiError::new(
                ErrorCode::ValidationBreakdownAttribute,
                "only choice classifications can be broken down",
            )
            .with_detail("id", id.to_string()),
        ));
    }
    Ok(def)
}

fn is_choice(def: &AttributeDef) -> bool {
    matches!(
        def.kind,
        AttributeKind::Enum | AttributeKind::OrderedEnum | AttributeKind::MultiEnum
    )
}

/// `stats.breakdown`; `use_cache: false` always recomputes (tests compare both paths).
pub(super) fn breakdown_with(
    req: &StatsBreakdownRequest,
    ctx: &QueryCtx<'_>,
    use_cache: bool,
) -> AppResult<StatsBreakdown> {
    let reads = ctx.tx;
    let def = choice_def(reads, req.attr_id)?;
    let cal = calendar(reads, req.tz_offset_mins)?;
    let today = cal.index(ctx.now);
    let from_day = range_start(today, req.days);
    let parts = Parts::load(reads, &def, &req.scope, cal, ctx.now)?;
    let days = match use_cache.then(|| cached(reads, &def, &parts, cal)).transpose()? {
        Some(Some(days)) => days,
        _ => fresh(reads, &parts, cal)?,
    };
    let rows = parts
        .groups
        .iter()
        .zip(days)
        .map(|((option, ids), days)| {
            let group_facts: Vec<CardFact> =
                ids.iter().filter_map(|i| parts.facts.get(i).cloned()).collect();
            let days: Vec<DayAggregate> = days
                .into_iter()
                .map(|(day, k)| {
                    let mut d = DayAggregate::empty(day);
                    d.reviews = k.iter().map(|c| c.count).fold(0, u32::saturating_add);
                    [d.learn, d.young, d.mature, d.relearn] = k;
                    d
                })
                .collect();
            let stats = review_stats(&days, today, from_day);
            let forecast = card_stats(&group_facts, today, 7);
            BreakdownRow {
                option_id: option.as_ref().map(|o| o.id),
                label: option.as_ref().map(|o| o.label.clone()),
                color: option.as_ref().and_then(|o| o.color.clone()),
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

/// Whether `req` would be answered from the cache (tests).
#[cfg(test)]
pub(super) fn uses_cache(req: &StatsBreakdownRequest, ctx: &QueryCtx<'_>) -> AppResult<bool> {
    let reads = ctx.tx;
    let def = choice_def(reads, req.attr_id)?;
    let cal = calendar(reads, req.tz_offset_mins)?;
    let parts = Parts::load(reads, &def, &req.scope, cal, ctx.now)?;
    Ok(cached(reads, &def, &parts, cal)?.is_some())
}

pub(super) fn breakdown(req: StatsBreakdownRequest, ctx: &QueryCtx<'_>) -> AppResult<StatsBreakdown> {
    breakdown_with(&req, ctx, true)
}

/// `stats.refresh_cache`: stores the whole-collection breakdown of every choice
/// classification on `cal` (skipped when its cache is valid and nothing was appended).
pub(super) fn refresh(tx: &mut dyn Writes, cal: StudyDay, now: Timestamp) -> AppResult<()> {
    let defs: Vec<AttributeDef> = tx
        .list_attribute_defs(false)?
        .into_iter()
        .filter(is_choice)
        .collect();
    for def in defs {
        let reads: &dyn Reads = &*tx;
        let parts = Parts::load(reads, &def, &StatsScope::default(), cal, now)?;
        let mark = reads.review_log_mark()?;
        let days = match cached(reads, &def, &parts, cal)? {
            Some(days) => days,
            None => fresh(reads, &parts, cal)?,
        };
        let record = StatsCacheRecord {
            fingerprint: Some(stored_fingerprint(reads, &parts)?),
            calendar: cache_key(&def, cal),
            upto_seq: mark.max_seq,
            row_count: reads.review_count_upto(mark.max_seq)?,
            generation: mark.generation,
            days: encode(&parts, &days)?,
            updated_at: now,
        };
        tx.put_stats_cache(&record)?;
    }
    Ok(())
}
