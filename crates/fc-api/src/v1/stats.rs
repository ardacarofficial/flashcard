//! Statistics (docs/02 §17, v0.6). Every number is computed in Rust from `review_log` and card
//! states (`fc_core::stats`); clients only draw them. Each statistic can be scoped to a deck,
//! a query (tags, classifications, …) and a date range, and broken down by classification.

use crate::v1::search::CardRow;
use fc_core::stats::{CardStats, ReportCard, ReviewStats, StateCounts, SuccessRates};
use fc_core::{AttributeDefId, AttributeOptionId, DeckId};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Which cards statistics look at. Empty = the whole collection (reviews of trashed and
/// purged cards included: review history outlives its card); otherwise the live cards
/// matching every given condition.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct StatsScope {
    /// Cards in this deck or its sub-decks.
    #[serde(default)]
    pub deck_id: Option<DeckId>,
    /// Cards with this tag (or a sub-tag).
    #[serde(default)]
    pub tag: Option<String>,
    /// Cards matching this query (v0.4 query language, e.g. `seviye:B1`).
    #[serde(default)]
    pub query: Option<String>,
}

impl StatsScope {
    /// No condition.
    pub fn is_global(&self) -> bool {
        self.deck_id.is_none()
            && self.tag.as_deref().is_none_or(|t| t.trim().is_empty())
            && self.query.as_deref().is_none_or(|q| q.trim().is_empty())
    }
}

/// `stats.overview`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct StatsRequest {
    /// Scope.
    #[serde(default)]
    pub scope: StatsScope,
    /// Client UTC offset in minutes (study-day boundary). Default 0.
    #[serde(default)]
    pub tz_offset_mins: Option<i32>,
    /// Last N days (`null` = all time).
    #[serde(default)]
    pub days: Option<u32>,
    /// Forecast length in days (default 30, at most 365).
    #[serde(default)]
    pub forecast_days: Option<u32>,
    /// Recompute from `review_log`, ignoring the cache.
    #[serde(default)]
    pub fresh: Option<bool>,
}

/// Response of `stats.overview`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct StatsOverview {
    /// Today's study-day index.
    pub today: i64,
    /// `YYYY-MM-DD` of today.
    pub today_date: String,
    /// First day of the range (`None` = all time).
    pub from_day: Option<i64>,
    /// Review statistics.
    pub reviews: ReviewStats,
    /// Card statistics.
    pub cards: CardStats,
    /// Desired retention the scope aims for: a deck scope's effective preset; otherwise the
    /// effective presets of the scope's cards (their common value, or the card-weighted mean
    /// when decks differ); the built-in default only for an empty scope.
    pub target_retention: f64,
    /// Review statistics came from the cache (+ rows appended since).
    pub from_cache: bool,
}

/// `stats.breakdown`: statistics per value of a classification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct StatsBreakdownRequest {
    /// Scope.
    #[serde(default)]
    pub scope: StatsScope,
    /// Classification (enum, ordered enum or multi enum).
    pub attr_id: AttributeDefId,
    /// Client UTC offset in minutes.
    #[serde(default)]
    pub tz_offset_mins: Option<i32>,
    /// Last N days of reviews (`null` = all time).
    #[serde(default)]
    pub days: Option<u32>,
}

/// One value of a classification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct BreakdownRow {
    /// Option (`None` = cards without a value).
    pub option_id: Option<AttributeOptionId>,
    /// Option label.
    pub label: Option<String>,
    /// Option color.
    pub color: Option<String>,
    /// Memory states.
    pub states: StateCounts,
    /// Report card.
    pub report: ReportCard,
    /// Success rates in the range.
    pub success: SuccessRates,
    /// True retention in the range.
    pub true_retention: Option<f64>,
    /// Answers in the range.
    pub reviews: u32,
    /// Workload: cards due today (incl. overdue).
    pub due_today: u32,
    /// Workload: cards due in the next 7 days (today included).
    pub due_week: u32,
}

/// Response of `stats.breakdown`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct StatsBreakdown {
    /// Classification.
    pub attr_id: AttributeDefId,
    /// Its name.
    pub name: String,
    /// One row per option (in option order), then cards without a value.
    pub rows: Vec<BreakdownRow>,
}

/// `stats.leeches`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct LeechRequest {
    /// Scope.
    #[serde(default)]
    pub scope: StatsScope,
    /// Lapse threshold (default: setting `stats.leech_threshold`).
    #[serde(default)]
    pub threshold: Option<u32>,
    /// At most this many (default 100, at most 1000).
    #[serde(default)]
    pub limit: Option<u32>,
}

/// Response of `stats.leeches`: cards with at least `threshold` lapses, most lapses first.
/// Actions on every leech of a scope (not only the listed page) are `stats.apply_to_leeches`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct LeechList {
    /// Threshold used.
    pub threshold: u32,
    /// Leeches in the scope (may be more than `items`).
    pub total: u32,
    /// Leeches.
    pub items: Vec<CardRow>,
}

/// What `stats.apply_to_leeches` does to the leeches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum LeechAction {
    /// Suspend the cards (`bulk.set_suspended`).
    Suspend,
    /// Tag their notes (`bulk.add_tags`).
    Tag,
}

/// `stats.apply_to_leeches`: one action on **every** leech of a scope, selected in Rust by the
/// same rule as `stats.leeches` (lapses ≥ threshold). One undo step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ApplyToLeeches {
    /// Scope.
    #[serde(default)]
    pub scope: StatsScope,
    /// Lapse threshold (default: setting `stats.leech_threshold`).
    #[serde(default)]
    pub threshold: Option<u32>,
    /// Action.
    pub action: LeechAction,
    /// Tag for [`LeechAction::Tag`] (default `leech`).
    #[serde(default)]
    pub tag: Option<String>,
}

/// Response of `stats.apply_to_leeches`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct LeechActionResult {
    /// Threshold used.
    pub threshold: u32,
    /// Leech cards matched.
    pub cards: u32,
    /// Cards (suspend) or notes (tag) changed.
    pub changed: u32,
    /// Left alone (already suspended / tagged).
    pub skipped: u32,
}

/// `stats.deck_reports`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct DeckReportsRequest {
    /// Client UTC offset in minutes.
    #[serde(default)]
    pub tz_offset_mins: Option<i32>,
}

/// The report card of a deck (with its sub-decks).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct DeckReport {
    /// Deck.
    pub deck_id: DeckId,
    /// Full path (`A::B`).
    pub path: String,
    /// Parent deck.
    pub parent_id: Option<DeckId>,
    /// Report card.
    pub report: ReportCard,
    /// Memory states.
    pub states: StateCounts,
    /// Cards due today (incl. overdue).
    pub due_today: u32,
}

/// Response of `stats.deck_reports` (live decks in tree order).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct DeckReports {
    /// Decks.
    pub items: Vec<DeckReport>,
}

/// `stats.refresh_cache`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct RefreshStatsCache {
    /// Client UTC offset in minutes (the cache is per study calendar).
    #[serde(default)]
    pub tz_offset_mins: Option<i32>,
}

/// What `stats.refresh_cache` did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct StatsCacheInfo {
    /// The cache was rebuilt from scratch (instead of extended).
    pub rebuilt: bool,
    /// Days cached.
    pub days: u32,
    /// Reviews folded in by this call.
    pub folded: u32,
}
