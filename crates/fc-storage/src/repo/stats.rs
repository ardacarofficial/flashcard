//! Statistics & smart planning (v0.6): review rows for statistics, the per-day statistics
//! cache, card due times (load balancing / forecast), exam dates and goals (migration 0500;
//! goals use the 0001 `goals` table).

use super::*;
use crate::repo::ReviewLogRecord;
use fc_core::{ExamId, GoalId};

/// Where the review log stands, for cache validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ReviewLogMark {
    /// Highest sequence number (SQLite: `review_log` rowid). Appended rows get higher ones.
    pub max_seq: i64,
    /// Changes whenever an existing row changes or disappears (appends leave it alone).
    pub generation: i64,
}

/// The statistics cache of one study calendar.
#[derive(Debug, Clone, PartialEq)]
pub struct StatsCacheRecord {
    /// Calendar key (`"<tz offset mins>:<rollover hour>"`).
    pub calendar: String,
    /// Highest review sequence folded in.
    pub upto_seq: i64,
    /// Review rows (any mode) with sequence ≤ `upto_seq` when it was built.
    pub row_count: i64,
    /// [`ReviewLogMark::generation`] when it was built.
    pub generation: i64,
    /// `(study day, aggregate JSON)`, by day.
    pub days: Vec<(i64, Value)>,
    /// Built at.
    pub updated_at: Timestamp,
}

/// One `exam_dates` row.
#[derive(Debug, Clone, PartialEq)]
pub struct ExamRecord {
    /// Id.
    pub id: ExamId,
    /// Name.
    pub name: String,
    /// Local date, `YYYY-MM-DD`.
    pub date: String,
    /// Deck subtree it applies to.
    pub deck_id: Option<DeckId>,
    /// Canonical query it applies to.
    pub query: Option<String>,
    /// Created.
    pub created_at: Timestamp,
    /// Updated.
    pub updated_at: Timestamp,
}

/// One `goals` row.
#[derive(Debug, Clone, PartialEq)]
pub struct GoalRecord {
    /// Id.
    pub id: GoalId,
    /// Kind (`fc_core::goal::GoalKind` string).
    pub kind: String,
    /// Target value.
    pub target: f64,
    /// Deadline: start of the deadline's study day (the goal counts through that day).
    pub deadline: Option<Timestamp>,
    /// Canonical scope query (`None` = whole collection).
    pub query: Option<String>,
    /// Created (the goal counts from this time's study day).
    pub created_at: Timestamp,
    /// Updated.
    pub updated_at: Timestamp,
}

/// Statistics & planning reads.
pub trait StatsRead {
    /// Review rows other than `mode = trial` with sequence > `after_seq` (of `cards` only,
    /// when given), in sequence order, with their sequence numbers.
    fn stats_reviews(
        &self,
        after_seq: i64,
        cards: Option<&[CardId]>,
    ) -> StorageResult<Vec<(i64, ReviewLogRecord)>>;
    /// Current [`ReviewLogMark`].
    fn review_log_mark(&self) -> StorageResult<ReviewLogMark>;
    /// Review rows (any mode) with sequence ≤ `seq`.
    fn review_count_upto(&self, seq: i64) -> StorageResult<i64>;
    /// The cache of `calendar`.
    fn stats_cache(&self, calendar: &str) -> StorageResult<Option<StatsCacheRecord>>;
    /// Due times in `[from, to)` of live, unsuspended, scheduled (non-new) cards of live notes.
    fn due_times(&self, from: Timestamp, to: Timestamp) -> StorageResult<Vec<Timestamp>>;
    /// Exams by date.
    fn list_exams(&self) -> StorageResult<Vec<ExamRecord>>;
    /// Exam by id.
    fn get_exam(&self, id: ExamId) -> StorageResult<Option<ExamRecord>>;
    /// Goals by creation.
    fn list_goals(&self) -> StorageResult<Vec<GoalRecord>>;
    /// Goal by id.
    fn get_goal(&self, id: GoalId) -> StorageResult<Option<GoalRecord>>;
}

/// Statistics & planning writes.
pub trait StatsWrite: StatsRead {
    /// Replaces the cache of `record.calendar`.
    fn put_stats_cache(&mut self, record: &StatsCacheRecord) -> StorageResult<()>;
    /// Inserts or replaces an exam.
    fn upsert_exam(&mut self, exam: &ExamRecord) -> StorageResult<()>;
    /// Deletes an exam; whether it existed.
    fn delete_exam(&mut self, id: ExamId) -> StorageResult<bool>;
    /// Inserts or replaces a goal.
    fn upsert_goal(&mut self, goal: &GoalRecord) -> StorageResult<()>;
    /// Deletes a goal; whether it existed.
    fn delete_goal(&mut self, id: GoalId) -> StorageResult<bool>;
}
