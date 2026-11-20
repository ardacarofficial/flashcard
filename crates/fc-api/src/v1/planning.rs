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
