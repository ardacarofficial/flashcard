//! Smart planning (docs/02 §5, §18, v0.6): exam dates, goals, FSRS optimization and the
//! workload estimate of a desired retention. Load balancing and easy days are settings of the
//! `planning` module (`planning.load_balance`, `planning.easy_days.<weekday>`).

use crate::v1::jobs::JobDto;
use fc_core::goal::{GoalKind, GoalProgress};
use fc_core::{DeckId, ExamId, GoalId, PresetId, Timestamp};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

// ------------------------------------------------------------------ exams

/// An exam: cards of the deck (with sub-decks) **and** matching the query are planned so they
/// are reviewed shortly before `date` (exam-date mode). Neither = every card.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ExamDto {
    /// Id.
    pub id: ExamId,
    /// Name ("YKS").
    pub name: String,
    /// Local date, `YYYY-MM-DD`.
    pub date: String,
    /// Deck scope.
    pub deck_id: Option<DeckId>,
    /// Query scope, with current names (e.g. `sınav:TYT`; stored canonically, so renaming the
    /// classification or option keeps it working).
    pub query: Option<String>,
    /// Created.
    pub created_at: Timestamp,
    /// Updated.
    pub updated_at: Timestamp,
}

/// `planning.exams.list`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ListExams {
    /// Client UTC offset in minutes (for `days_left`).
    #[serde(default)]
    pub tz_offset_mins: Option<i32>,
}

/// An exam with its countdown.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ExamItem {
    /// Exam.
    pub exam: ExamDto,
    /// Study days until the exam (0 = today, negative = past).
    pub days_left: i64,
    /// Live cards it applies to.
    pub cards: u32,
}

/// Response of `planning.exams.list` (by date).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ExamList {
    /// Exams.
    pub items: Vec<ExamItem>,
}

/// `planning.exams.create`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct CreateExam {
    /// Name.
    pub name: String,
    /// Local date, `YYYY-MM-DD`.
    pub date: String,
    /// Deck scope.
    #[serde(default)]
    pub deck_id: Option<DeckId>,
    /// Query scope (e.g. `sınav:TYT`).
    #[serde(default)]
    pub query: Option<String>,
}

/// `planning.exams.update`: replaces every field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct UpdateExam {
    /// Id.
    pub id: ExamId,
    /// Name.
    pub name: String,
    /// Local date.
    pub date: String,
    /// Deck scope.
    #[serde(default)]
    pub deck_id: Option<DeckId>,
    /// Query scope.
    #[serde(default)]
    pub query: Option<String>,
}

/// An exam by id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ExamRef {
    /// Id.
    pub id: ExamId,
}

/// Internal `planning.exams.restore` (inverse of delete): the row exactly as it was (query in
/// canonical form).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct RestoreExam {
    /// The exam.
    pub exam: ExamDto,
}

// ------------------------------------------------------------------ goals

/// A goal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct GoalDto {
    /// Id.
    pub id: GoalId,
    /// What it measures.
    pub kind: GoalKind,
    /// Target (cards, minutes, days or percent).
    pub target: f64,
    /// Last day (local date, `YYYY-MM-DD`); `None` = no deadline (study minutes: per day).
    pub deadline: Option<String>,
    /// Scope query with current names (`None` = whole collection).
    pub query: Option<String>,
    /// Set at (counts from this study day).
    pub created_at: Timestamp,
    /// Updated.
    pub updated_at: Timestamp,
}

/// `goals.list`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ListGoals {
    /// Client UTC offset in minutes.
    #[serde(default)]
    pub tz_offset_mins: Option<i32>,
}

/// A goal with its progress.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct GoalItem {
    /// Goal.
    pub goal: GoalDto,
    /// Progress and pace forecast.
    pub progress: GoalProgress,
}

/// Response of `goals.list`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct GoalList {
    /// Goals, oldest first.
    pub items: Vec<GoalItem>,
}

/// `goals.create`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct CreateGoal {
    /// Kind.
    pub kind: GoalKind,
    /// Target.
    pub target: f64,
    /// Last day, `YYYY-MM-DD`.
    #[serde(default)]
    pub deadline: Option<String>,
    /// Scope query.
    #[serde(default)]
    pub query: Option<String>,
}

/// `goals.update`: replaces target, deadline and scope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct UpdateGoal {
    /// Id.
    pub id: GoalId,
    /// Target.
    pub target: f64,
    /// Last day.
    #[serde(default)]
    pub deadline: Option<String>,
    /// Scope query.
    #[serde(default)]
    pub query: Option<String>,
}

/// A goal by id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct GoalRef {
    /// Id.
    pub id: GoalId,
}

/// Internal `goals.restore` (inverse of delete): the row exactly as it was.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct RestoreGoal {
    /// The goal (query in canonical form).
    pub goal: GoalDto,
}

// ------------------------------------------------------------------ optimizer

/// `optimizer.run`: starts a background job that fits FSRS weights to the review history of
/// the decks using `preset_id` and saves them into the preset (`presets.update`: one undo
/// step). Cancellable with `jobs.cancel`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct RunOptimizer {
    /// Preset to optimize.
    pub preset_id: PresetId,
    /// Client UTC offset in minutes (review days).
    #[serde(default)]
    pub tz_offset_mins: Option<i32>,
}

/// Result JSON of an optimizer job (`JobDto.result`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct OptimizerResult {
    /// Preset.
    pub preset_id: PresetId,
    /// Training items used.
    pub items: u32,
    /// Cards whose history was used.
    pub cards: u32,
    /// Weights saved into the preset.
    pub weights: Vec<f64>,
    /// Weights before.
    pub previous: Vec<f64>,
}

/// Response of `optimizer.run`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct OptimizerStarted {
    /// The queued job.
    pub job: JobDto,
}

/// `optimizer.workload`: estimated daily reviews at several desired retentions.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct WorkloadRequest {
    /// Cards of this deck (with sub-decks) and its preset; `None` = whole collection with the
    /// default preset.
    #[serde(default)]
    pub deck_id: Option<DeckId>,
    /// Retentions to evaluate (each 0.70–0.99; default 0.70, 0.75, … 0.99 and the current
    /// one).
    #[serde(default)]
    pub retentions: Option<Vec<f64>>,
}

/// One point of the workload curve.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, TS)]
pub struct WorkloadPoint {
    /// Desired retention.
    pub retention: f64,
    /// Estimated reviews per day.
    pub reviews_per_day: f64,
    /// Relative to the current retention (1.0 = same).
    pub relative: f64,
}

/// Response of `optimizer.workload`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct WorkloadEstimate {
    /// Current desired retention of the scope's preset.
    pub current_retention: f64,
    /// Review cards the estimate is based on.
    pub review_cards: u32,
    /// New cards per day of the preset (counted with their learning answers).
    pub new_per_day: u32,
    /// Points, by retention.
    pub points: Vec<WorkloadPoint>,
}

// ------------------------------------------------------------------ workload forecast (v1.3)

/// `planning.forecast`: day-by-day workload (cards and minutes a day) of the cards as they are
/// now plus a new-cards-per-day pace (`fc_study::planning::forecast`: an expected-value FSRS
/// simulation with the scope's preset weights and retention). Changes nothing.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct ForecastRequest {
    /// Cards of this deck (with sub-decks) and its preset; `None` = the whole collection with
    /// each top-level deck's own new-card limit (summed).
    #[serde(default)]
    pub deck_id: Option<DeckId>,
    /// New cards per day to plan with (`None` = the scope's current limit; at most 9999).
    #[serde(default)]
    pub new_per_day: Option<u32>,
    /// Optional exam date (`YYYY-MM-DD`): the forecast runs at least until it and says what is
    /// left by then.
    #[serde(default)]
    pub exam_date: Option<String>,
    /// Days to forecast, today included (default 28; at least until the exam; at most 365).
    #[serde(default)]
    pub days: Option<u32>,
    /// Client UTC offset in minutes (study days).
    #[serde(default)]
    pub tz_offset_mins: Option<i32>,
}

/// One forecast day.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ForecastDayDto {
    /// Days from today (0 = today).
    pub offset: u32,
    /// Local date, `YYYY-MM-DD`.
    pub date: String,
    /// Cards to study that day as a launcher counts them (started cards within the decks'
    /// reviews/day + new cards introduced). Day 0 = today's real queue (`study.options`).
    pub cards: f64,
    /// Of which: started cards (reviews and learning cards).
    pub reviews: f64,
    /// New cards introduced that day.
    pub new_cards: f64,
    /// Expected minutes.
    pub minutes: f64,
}

/// Mean daily load of the week ending on a day.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ForecastOutlook {
    /// Days from today of the week's last day.
    pub offset: u32,
    /// Its date.
    pub date: String,
    /// Mean cards per day over that week (the 7 days ending on `date`).
    pub cards_per_day: f64,
    /// Mean minutes per day over that week.
    pub minutes_per_day: f64,
}

/// One week of the forecast (week 1 starts today).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ForecastWeek {
    /// 1-based week number.
    pub week: u32,
    /// First day.
    pub start_date: String,
    /// Last day (the forecast's last day for a short final week).
    pub end_date: String,
    /// Mean cards per day.
    pub cards_per_day: f64,
    /// Mean minutes per day.
    pub minutes_per_day: f64,
    /// New cards introduced during the week.
    pub new_cards: f64,
}

/// What an exam date means for the pace.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ExamOutlook {
    /// The date.
    pub date: String,
    /// Study days until it (0 = today, negative = past).
    pub days_left: i64,
    /// New cards not yet started on the exam day at this pace.
    pub new_left: u32,
    /// New cards per day that would start every new card by the day before the exam (`None`
    /// when the exam is today or past).
    pub new_per_day_needed: Option<u32>,
    /// Load of the last week before the exam.
    pub last_week: ForecastOutlook,
}

/// A deck, by id and path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct DeckBrief {
    /// Deck.
    pub id: DeckId,
    /// Full path (`A::B`).
    pub path: String,
}

/// Where `planning.apply_pace` writes the new-card limit of a deck.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PaceTarget {
    /// The deck the pace is for.
    pub deck: DeckBrief,
    /// The preset whose `new_per_day` changes (`None`: the deck has no preset yet — applying
    /// creates one for it).
    pub preset_id: Option<PresetId>,
    /// Its name.
    pub preset_name: Option<String>,
    /// Its current new-card limit.
    pub current_new_per_day: u32,
    /// Every live deck whose limit changes with it (decks sharing the preset; without a
    /// preset: the deck and its sub-decks that inherit).
    pub decks: Vec<DeckBrief>,
}

/// Response of `planning.forecast`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct WorkloadForecast {
    /// Scope.
    pub deck_id: Option<DeckId>,
    /// New cards per day the forecast used.
    pub new_per_day: u32,
    /// The scope's current limit.
    pub current_new_per_day: u32,
    /// New cards already started today (today's allowance is the rest).
    pub new_done_today: u32,
    /// Cards not studied yet (suspended cards are not counted anywhere).
    pub unseen: u32,
    /// Cards already started.
    pub started: u32,
    /// Seconds per answer the minutes are based on (scope's last 30 days, else 8).
    pub secs_per_answer: f64,
    /// Whether `secs_per_answer` came from the history.
    pub measured_time: bool,
    /// Desired retention of the scope.
    pub retention: f64,
    /// One entry per day, today first.
    pub days: Vec<ForecastDayDto>,
    /// The same days by week (7 days each, today first).
    pub weeks: Vec<ForecastWeek>,
    /// The heaviest day.
    pub peak: ForecastDayDto,
    /// Mean load of the week three weeks from now (or the forecast's last week if shorter):
    /// "at this pace, in 3 weeks: ~85 cards, ~14 min a day".
    pub outlook: ForecastOutlook,
    /// Date the last new card is started at this pace (`None`: none left, or not within the
    /// forecast).
    pub new_finished_on: Option<String>,
    /// Exam outlook, when `exam_date` was given.
    pub exam: Option<ExamOutlook>,
    /// What applying this pace would change (`None` for the whole collection: apply per deck).
    pub target: Option<PaceTarget>,
}

/// `planning.apply_pace`: sets the new-card limit of a deck's preset (creating and assigning a
/// preset when the deck has none), v1.4 also its review limit and desired retention (the
/// "Daily pace" settings page), and optionally adds an exam date for the deck. One undo
/// step; the forecast's `target` says beforehand what it touches.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct ApplyPace {
    /// Deck.
    pub deck_id: DeckId,
    /// New cards per day (0–9999).
    pub new_per_day: u32,
    /// Name of the preset to create when the deck has none (default: the deck's name).
    #[serde(default)]
    pub preset_name: Option<String>,
    /// Also add an exam on this date (`YYYY-MM-DD`) for the deck, unless it has one that day.
    #[serde(default)]
    pub exam_date: Option<String>,
    /// The exam's name (default: the deck's name).
    #[serde(default)]
    pub exam_name: Option<String>,
    /// v1.4: also set the preset's review limit (0–99999; `None` = unchanged).
    #[serde(default)]
    pub reviews_per_day: Option<u32>,
    /// v1.4: also set the preset's desired retention (0.70–0.99; `None` = unchanged).
    #[serde(default)]
    pub desired_retention: Option<f64>,
}

/// Response of `planning.apply_pace`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PaceApplied {
    /// What was changed (now with the new limit).
    pub target: PaceTarget,
    /// The previous limit.
    pub previous_new_per_day: u32,
    /// A preset was created for the deck.
    pub created_preset: bool,
    /// The exam added, if any.
    pub exam: Option<ExamDto>,
}
