//! Statistics (docs/02 §17, v0.6). Every number is computed in Rust from `review_log` and card
//! states (`fc_core::stats`); clients only draw them. Each statistic can be scoped to a deck,
//! a query (tags, classifications, …) and a date range, and broken down by classification.

use crate::v1::search::CardRow;
use fc_core::stats::{CardStats, ReportCard, ReviewStats, StateCounts, Streak, SuccessRates};
use fc_core::{AttributeDefId, AttributeOptionId, CardId, DeckId, NoteId, Timestamp};
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

// ------------------------------------------------------------------ v1.3: three questions

/// `stats.today`: "how am I doing today?" — today's answers next to yesterday's and the
/// recent average, with a plain verdict the UI words.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct TodayRequest {
    /// Scope.
    #[serde(default)]
    pub scope: StatsScope,
    /// Client UTC offset in minutes (study-day boundary).
    #[serde(default)]
    pub tz_offset_mins: Option<i32>,
}

/// One study day's answers.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
pub struct DayFigures {
    /// Answers (scheduled and imported; practice answers are not statistics).
    pub answered: u32,
    /// Of which correct (anything but "Again").
    pub correct: u32,
    /// `correct / answered` (`None` without answers).
    pub rate: Option<f64>,
    /// Time spent, ms.
    pub time_ms: u64,
    /// New cards started.
    pub new_cards: u32,
}

/// Today's pace compared with yesterday (or, without answers yesterday, with the mean of
/// the last 7 study days before today).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum TodayPace {
    /// No answers today yet.
    NotStarted,
    /// No earlier answers to compare with.
    FirstDay,
    /// Clearly more than the reference (> +15 %).
    Ahead,
    /// About the same (±15 %).
    Same,
    /// Clearly fewer so far (< −15 %).
    Behind,
}

/// Response of `stats.today`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct TodayStats {
    /// `YYYY-MM-DD` of today.
    pub today_date: String,
    /// Today.
    pub today: DayFigures,
    /// Yesterday.
    pub yesterday: DayFigures,
    /// Mean answers per day over the 7 days before today (days without answers count as 0).
    pub week_mean_answered: f64,
    /// Verdict.
    pub pace: TodayPace,
    /// What `pace` compared with (answers): yesterday's, or the week mean.
    pub reference_answered: f64,
    /// Whether the reference is yesterday (`false`: the 7-day mean).
    pub reference_is_yesterday: bool,
    /// Streak.
    pub streak: Streak,
}

/// `stats.struggles`: "what am I forgetting?" — cards answered wrong ("Again", any study
/// mode) in the last `days` study days, most misses first, then most recent.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct StrugglesRequest {
    /// Scope.
    #[serde(default)]
    pub scope: StatsScope,
    /// Window in study days, today included (default 7; 1 = today only; at most 365).
    #[serde(default)]
    pub days: Option<u32>,
    /// At most this many items (default 20, at most 200).
    #[serde(default)]
    pub limit: Option<u32>,
    /// Client UTC offset in minutes.
    #[serde(default)]
    pub tz_offset_mins: Option<i32>,
}

/// A card answered wrong in the window.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct StruggleItem {
    /// Card.
    pub card_id: CardId,
    /// Note.
    pub note_id: NoteId,
    /// Deck.
    pub deck_id: DeckId,
    /// Deck path.
    pub deck_path: String,
    /// The word / prompt (plain text of the note's term field).
    pub term: String,
    /// Its meaning (plain text of the note's meaning field; may be empty).
    pub meaning: String,
    /// Wrong answers in the window.
    pub misses: u32,
    /// All answers in the window.
    pub answers: u32,
    /// Last wrong answer.
    pub last_miss_at: Timestamp,
    /// Last answer of any kind.
    pub last_seen_at: Timestamp,
    /// Lapses (forgotten after graduating), all time.
    pub lapses: u32,
    /// Suspended now.
    pub suspended: bool,
}

/// Response of `stats.struggles`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct StruggleList {
    /// Window used (study days).
    pub days: u32,
    /// Live cards answered wrong in the window (may be more than `items`).
    pub total: u32,
    /// Of which answered wrong today.
    pub today: u32,
    /// Items, most misses first, then most recent.
    pub items: Vec<StruggleItem>,
    /// Query that selects exactly the listed, not suspended cards (`cid:…`), for a practice
    /// session (`study.start` kind `custom`, mode `trial`); `None` when there are none.
    pub study_query: Option<String>,
}
