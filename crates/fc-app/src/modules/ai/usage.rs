//! Usage ledger (`ai_usage`), cost estimates from the price table, the monthly budget and the
//! usage summary (docs/06 §1.3).

use super::BUDGET;
use crate::error::AppResult;
use crate::handler::{CommandCtx, CommandOutcome, QueryCtx};
use crate::registry::Registry;
use crate::support::summary;
use fc_api::v1::ai::{AiBudgetStatus, AiUsageGroup, AiUsageSummary, AiUsageTotals, RecordAiUsage};
use fc_api::v1::common::Empty;
use fc_api::v1::methods::{AiUsageGet as SummaryMethod, AiUsageRecord as RecordMethod};
use fc_core::settings::SettingScope;
use fc_core::Timestamp;
use fc_storage::repo::{AiPriceRecord, AiUsageRecord};
use fc_storage::Reads;
use std::collections::BTreeMap;

const DAY_MS: i64 = 86_400_000;

pub(super) fn register(reg: &mut Registry) {
    reg.query::<SummaryMethod>(|_: Empty, ctx: &QueryCtx<'_>| summarize(ctx.tx, ctx.now));
    reg.command::<RecordMethod>(record);
}

/// Price of `model`: an exact row, else the longest row that is a prefix of it (a dated id
/// `claude-sonnet-5-5-20261001` uses `claude-sonnet-5-5`).
pub(crate) fn price_for<'p>(prices: &'p [AiPriceRecord], model: &str) -> Option<&'p AiPriceRecord> {
    prices.iter().find(|p| p.model == model).or_else(|| {
        prices
            .iter()
            .filter(|p| model.starts_with(&p.model))
            .max_by_key(|p| p.model.len())
    })
}

/// Estimated USD cost (`None` = no price for the model).
pub(crate) fn cost(prices: &[AiPriceRecord], model: &str, input: u64, output: u64) -> Option<f64> {
    price_for(prices, model).map(|p| {
        (input as f64 * p.input_per_mtok + output as f64 * p.output_per_mtok) / 1_000_000.0
    })
}

/// Days since 1970-01-01 → (year, month, day) (Howard Hinnant's civil-from-days).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// (year, month, day) → days since 1970-01-01.
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let m = i64::from(m);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Start of the calendar month (UTC) containing `now`.
pub(crate) fn month_start(now: Timestamp) -> Timestamp {
    let (y, m, _) = civil_from_days(now.0.div_euclid(DAY_MS));
    Timestamp(days_from_civil(y, m, 1) * DAY_MS)
}

fn add(t: &mut AiUsageTotals, u: &AiUsageRecord) {
    t.requests += 1;
    t.input_tokens += u64::from(u.input_tokens);
    t.output_tokens += u64::from(u.output_tokens);
    t.cost_usd += u.cost_estimate.unwrap_or(0.0);
}

/// The monthly budget (`None` = none set).
pub(crate) fn budget_limit(reads: &dyn Reads) -> AppResult<Option<f64>> {
    let v = crate::settings::resolve_stored(reads, &SettingScope::App, BUDGET)?;
    Ok(v.and_then(|(v, _)| v.as_f64()).filter(|b| *b > 0.0))
}

/// Budget state given what this month spent (+ `extra`, e.g. a request about to be sent).
pub(crate) fn budget(reads: &dyn Reads, now: Timestamp, extra: f64) -> AppResult<AiBudgetStatus> {
    let spent: f64 = reads
        .list_ai_usage(month_start(now))?
        .iter()
        .map(|u| u.cost_estimate.unwrap_or(0.0))
        .sum();
    let limit = budget_limit(reads)?;
    let total = spent + extra;
    Ok(AiBudgetStatus {
        month_spent_usd: spent,
        limit_usd: limit,
        warning: limit.is_some_and(|l| total >= 0.8 * l),
        exceeded: limit.is_some_and(|l| total > l),
    })
}

fn groups(map: BTreeMap<String, AiUsageTotals>) -> Vec<AiUsageGroup> {
    let mut v: Vec<AiUsageGroup> = map
        .into_iter()
        .map(|(key, totals)| AiUsageGroup { key, totals })
        .collect();
    v.sort_by(|a, b| {
        b.totals
            .cost_usd
            .total_cmp(&a.totals.cost_usd)
            .then(b.totals.requests.cmp(&a.totals.requests))
            .then(a.key.cmp(&b.key))
    });
    v
}

fn summarize(reads: &dyn Reads, now: Timestamp) -> AppResult<AiUsageSummary> {
    let month_start = month_start(now);
    let week_start = Timestamp(now.0 - 7 * DAY_MS);
    let since = Timestamp(month_start.0.min(week_start.0));
    let mut month = AiUsageTotals::default();
    let mut week = AiUsageTotals::default();
    let mut by_feature: BTreeMap<String, AiUsageTotals> = BTreeMap::new();
    let mut by_model: BTreeMap<String, AiUsageTotals> = BTreeMap::new();
    for u in reads.list_ai_usage(since)? {
        if u.created_at >= week_start {
            add(&mut week, &u);
        }
        if u.created_at >= month_start {
            add(&mut month, &u);
            let feature = u.feature.strip_prefix("ai.").unwrap_or(&u.feature).to_owned();
            add(by_feature.entry(feature).or_default(), &u);
            add(by_model.entry(u.model.clone()).or_default(), &u);
        }
    }
    Ok(AiUsageSummary {
        month_start,
        month,
        week,
        by_feature: groups(by_feature),
        by_model: groups(by_model),
        budget: budget(reads, now, 0.0)?,
    })
}

fn record(req: RecordAiUsage, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<Empty>> {
    let prices = ctx.reads().list_ai_prices()?;
    let row = AiUsageRecord {
        id: ctx.ids.next_uuid().to_string(),
        provider: req.provider_id.to_string(),
        model: req.model.clone(),
        feature: req.feature.id().to_owned(),
        input_tokens: req.input_tokens,
        output_tokens: req.output_tokens,
        cost_estimate: cost(
            &prices,
            &req.model,
            u64::from(req.input_tokens),
            u64::from(req.output_tokens),
        ),
        created_at: ctx.now,
    };
    ctx.tx.insert_ai_usage(&row)?;
    Ok(CommandOutcome {
        response: Empty {},
        // A ledger row: not an undo step (undoing would hide money spent).
        inverse: vec![],
        summary: summary::<RecordMethod>(),
        events: vec![],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn month_start_is_the_first_utc_day() {
        // 2026-11-20T15:00:00Z → 2026-11-01T00:00:00Z
        assert_eq!(month_start(Timestamp(1_795_186_800_000)), Timestamp(1_793_491_200_000));
        // 2024-03-01T00:00:00Z stays; leap-year February.
        assert_eq!(month_start(Timestamp(1_709_251_200_000)), Timestamp(1_709_251_200_000));
        assert_eq!(month_start(Timestamp(1_709_251_199_999)), Timestamp(1_706_745_600_000));
        for days in [-800, 0, 59, 10_957, 20_000] {
            let (y, m, d) = civil_from_days(days);
            assert_eq!(days_from_civil(y, m, d), days);
        }
    }

    #[test]
    fn prices_match_exactly_then_by_longest_prefix() {
        let p = |m: &str, i: f64, o: f64| AiPriceRecord {
            model: m.into(),
            input_per_mtok: i,
            output_per_mtok: o,
            updated_at: Timestamp(0),
        };
        let prices = vec![p("claude", 1.0, 1.0), p("claude-sonnet-5-5", 3.0, 15.0)];
        assert_eq!(price_for(&prices, "claude-sonnet-5-5-20261001").unwrap().input_per_mtok, 3.0);
        assert_eq!(price_for(&prices, "claude-x").unwrap().input_per_mtok, 1.0);
        assert!(price_for(&prices, "llama3").is_none());
        let c = cost(&prices, "claude-sonnet-5-5", 1_000_000, 100_000).unwrap();
        assert!((c - 4.5).abs() < 1e-9);
    }
}
