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

// ------------------------------------------------------------------ leech repair (v1.4 W3)

/// `stats.leech_repairs.plan`: a card by id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct LeechCardRef {
    /// Card.
    pub card_id: CardId,
}

/// One failed review of a leech (evidence).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(optional_fields = nullable)]
pub struct LeechFailedReview {
    /// When.
    pub at: Timestamp,
    /// Study mode of the review (`normal`, `learn`, …).
    pub mode: String,
    /// Show → rate, ms.
    pub duration_ms: u32,
    /// Show → reveal, ms.
    pub answer_duration_ms: Option<u32>,
    /// Hints used.
    pub hints_used: u32,
    /// The typed answer. Always `null` today: the study flow does not keep typed answers
    /// (see `typed_answers_recorded`).
    pub typed_answer: Option<String>,
}

/// Another note of the same word (a confusable meaning).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct LeechConfusable {
    /// The other note.
    pub note_id: NoteId,
    /// Its meaning (plain text).
    pub meaning: String,
}

/// A repair of a leech: each runs one existing undoable command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum LeechRepair {
    /// Show a better example on the card (`subtitle.encounters.set_primary`).
    SwitchExample {
        /// The encounter to show.
        capture_id: String,
    },
    /// Accept more answers in the written check (`accepted_answers.add`).
    AddAcceptedAnswers {
        /// Field (`null` = the meaning field).
        #[serde(default)]
        field: Option<String>,
        /// Answers.
        answers: Vec<String>,
    },
    /// An example with another meaning goes to another (or a new) note
    /// (`subtitle.encounters.move`): the card stops mixing two meanings.
    MoveMeaning {
        /// The encounter.
        capture_id: String,
        /// Target note (`null` = a new note).
        #[serde(default)]
        to_note_id: Option<NoteId>,
    },
    /// The reviewed result of the AI "rewrite a leech" task (`ai.preview` → `ai.run` with
    /// `rewrite_leech`), applied with `ai.notes.update`.
    AiRewrite {
        /// Field key → new value.
        fields: std::collections::BTreeMap<String, String>,
    },
}

/// Kind of a [`LeechRepair`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum LeechRepairKind {
    /// [`LeechRepair::SwitchExample`].
    SwitchExample,
    /// [`LeechRepair::AddAcceptedAnswers`].
    AddAcceptedAnswers,
    /// [`LeechRepair::MoveMeaning`].
    MoveMeaning,
    /// [`LeechRepair::AiRewrite`].
    AiRewrite,
}

impl LeechRepairKind {
    /// Storage string.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SwitchExample => "switch_example",
            Self::AddAcceptedAnswers => "add_accepted_answers",
            Self::MoveMeaning => "move_meaning",
            Self::AiRewrite => "ai_rewrite",
        }
    }

    /// From the storage string.
    pub fn parse(s: &str) -> Option<Self> {
        [
            Self::SwitchExample,
            Self::AddAcceptedAnswers,
            Self::MoveMeaning,
            Self::AiRewrite,
        ]
        .into_iter()
        .find(|k| k.as_str() == s)
    }
}

impl LeechRepair {
    /// Its kind.
    pub const fn kind(&self) -> LeechRepairKind {
        match self {
            Self::SwitchExample { .. } => LeechRepairKind::SwitchExample,
            Self::AddAcceptedAnswers { .. } => LeechRepairKind::AddAcceptedAnswers,
            Self::MoveMeaning { .. } => LeechRepairKind::MoveMeaning,
            Self::AiRewrite { .. } => LeechRepairKind::AiRewrite,
        }
    }
}

/// Why a repair is proposed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum LeechRepairWhy {
    /// Another encounter ranks clearly better than the one on the card.
    BetterExample,
    /// The meaning lists several senses; each alone is not accepted yet.
    SensesNotAccepted,
    /// A suggested meaning in context lists other correct answers not accepted yet.
    AlsoCorrectNotAccepted,
    /// An encounter was captured with a different meaning than the card's.
    MixedMeanings,
    /// Nothing specific found, or a fresh angle may help: the AI rewrite task.
    FreshAngle,
}

/// A field's value before and after a repair (preview).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct LeechFieldChange {
    /// Field key (`accepted_answers` for the accepted answer list).
    pub field: String,
    /// Before (plain text).
    pub before: String,
    /// After (plain text).
    pub after: String,
}

/// A proposed repair, previewed. Apply it with `stats.leech_repairs.apply` (`repair` as given).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(optional_fields = nullable)]
pub struct LeechRepairProposal {
    /// The repair (for [`LeechRepair::AiRewrite`] `fields` is empty: run `ai_preview` first,
    /// then apply the reviewed result).
    pub repair: LeechRepair,
    /// The existing command it runs.
    pub method: String,
    /// Why it is proposed.
    pub why: LeechRepairWhy,
    /// What changes on the card's note.
    pub preview: Vec<LeechFieldChange>,
    /// For the AI rewrite: the `ai.preview` request to show before anything leaves.
    pub ai_preview: Option<crate::v1::ai::PreviewAi>,
}

/// A repair as recorded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct LeechRepairDto {
    /// Record id.
    pub id: i64,
    /// Card.
    pub card_id: CardId,
    /// Its note.
    pub note_id: NoteId,
    /// Kind.
    pub kind: LeechRepairKind,
    /// The repair as applied.
    pub repair: LeechRepair,
    /// The card's lapses then.
    pub lapses: u32,
    /// When.
    pub applied_at: Timestamp,
}

/// Response of `stats.leech_repairs.plan`: the evidence and the proposed repairs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(optional_fields = nullable)]
pub struct LeechRepairPlan {
    /// Card.
    pub card_id: CardId,
    /// Its note.
    pub note_id: NoteId,
    /// Times forgotten.
    pub lapses: u32,
    /// Leech threshold (setting `stats.leech_threshold`).
    pub threshold: u32,
    /// `lapses >= threshold`.
    pub is_leech: bool,
    /// The latest failed reviews, newest first (at most 5; practice reviews included).
    pub failed_reviews: Vec<LeechFailedReview>,
    /// Whether typed answers are kept (`false` today).
    pub typed_answers_recorded: bool,
    /// The field the card asks for (the note type's meaning field).
    pub answer_field: Option<String>,
    /// Its value (plain text).
    pub answer: String,
    /// The senses in it (`a; b, c` → three).
    pub senses: Vec<String>,
    /// Answers the written check also accepts.
    pub accepted_answers: Vec<String>,
    /// The note's encounters ranked (`null`: no encounters, or the `examples` module is off).
    pub examples: Option<crate::v1::examples::RankedExamples>,
    /// Other notes of the same word.
    pub confusables: Vec<LeechConfusable>,
    /// Proposed repairs, most specific first.
    pub proposals: Vec<LeechRepairProposal>,
    /// Repairs applied to this card before, oldest first.
    pub repairs: Vec<LeechRepairDto>,
}

/// `stats.leech_repairs.apply`: applies one repair to a leech and records it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ApplyLeechRepair {
    /// Card.
    pub card_id: CardId,
    /// The repair.
    pub repair: LeechRepair,
}

/// Internal `stats.leech_repairs.restore` (the inverse of a repair): removes (`remove`) or
/// puts back (`put`) recorded repairs exactly.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct RestoreLeechRepairs {
    /// Record ids to remove.
    #[serde(default)]
    pub remove: Vec<i64>,
    /// Records to put back.
    #[serde(default)]
    pub put: Vec<LeechRepairDto>,
}

/// `stats.leech_repairs.list`: what happened after the recorded repairs.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct LeechRepairsRequest {
    /// One card (`null` = every repaired card).
    #[serde(default)]
    pub card_id: Option<CardId>,
    /// Newest repairs first, at most this many (default 100).
    #[serde(default)]
    pub limit: Option<u32>,
}

/// Pass rate of a run of scheduled reviews (practice reviews do not count).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(optional_fields = nullable)]
pub struct LeechPassRate {
    /// Reviews counted.
    pub reviews: u32,
    /// Not "again".
    pub passed: u32,
    /// `passed / reviews` (`null` without reviews).
    pub rate: Option<f64>,
}

/// One review after a repair.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct LeechReviewAfter {
    /// When.
    pub at: Timestamp,
    /// Remembered (not "again").
    pub passed: bool,
    /// Show → rate, ms.
    pub duration_ms: u32,
    /// Hints used.
    pub hints_used: u32,
}

/// A repair and the card's reviews around it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct LeechRepairOutcome {
    /// The repair.
    pub repair: LeechRepairDto,
    /// The last scheduled reviews before it (at most 10).
    pub before: LeechPassRate,
    /// The scheduled reviews since (until the card's next repair).
    pub after: LeechPassRate,
    /// Those reviews, oldest first (at most 20).
    pub next_reviews: Vec<LeechReviewAfter>,
    /// Honest caveat: fewer than 10 reviews after the repair — "n is small", the difference
    /// says little yet.
    pub small_sample: bool,
}

/// Response of `stats.leech_repairs.list`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct LeechRepairReport {
    /// Newest repair first.
    pub items: Vec<LeechRepairOutcome>,
}
