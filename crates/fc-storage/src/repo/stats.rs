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
    /// What the aggregates were built for, beyond the review log (v1.0: the card sets of a
    /// `stats.breakdown`); `None` for the whole-collection cache.
    pub fingerprint: Option<String>,
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

/// The columns of a review row that statistics read (v1.0: a lean row for streaming the
/// whole history; see [`StatsRead::for_each_stats_review_row`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StatsReviewRow {
    /// Card.
    pub card_id: CardId,
    /// Button pressed.
    pub rating: fc_core::card::Rating,
    /// State before the answer.
    pub state_before: fc_core::card::CardState,
    /// Days since the previous review.
    pub elapsed_days: u32,
    /// Show → rate, ms.
    pub duration_ms: u32,
    /// `mode = imported`.
    pub imported: bool,
    /// When.
    pub created_at: Timestamp,
}

impl StatsReviewRow {
    /// The lean row of a full one.
    pub fn of(r: &ReviewLogRecord) -> Self {
        Self {
            card_id: r.card_id,
            rating: r.rating,
            state_before: r.state_before,
            elapsed_days: r.elapsed_days,
            duration_ms: r.duration_ms,
            imported: r.mode == super::REVIEW_MODE_IMPORTED,
            created_at: r.created_at,
        }
    }
}

/// Statistics & planning reads.
pub trait StatsRead {
    /// Review rows other than practice ones (`trial`, `trial_<mode>`, `test`:
    /// [`super::is_practice_mode`]) with sequence > `after_seq` (of `cards` only, when given),
    /// in sequence order, with their sequence numbers.
    fn stats_reviews(
        &self,
        after_seq: i64,
        cards: Option<&[CardId]>,
    ) -> StorageResult<Vec<(i64, ReviewLogRecord)>>;
    /// [`StatsRead::stats_reviews`] one row at a time, without collecting them (v1.0: the
    /// two million rows of a long-used collection need not sit in memory together).
    fn for_each_stats_review(
        &self,
        after_seq: i64,
        cards: Option<&[CardId]>,
        f: &mut dyn FnMut(i64, &ReviewLogRecord),
    ) -> StorageResult<()> {
        for (seq, r) in self.stats_reviews(after_seq, cards)? {
            f(seq, &r);
        }
        Ok(())
    }
    /// [`StatsRead::for_each_stats_review`] with only the [`StatsReviewRow`] columns (no ids
    /// or texts decoded that statistics never look at).
    fn for_each_stats_review_row(
        &self,
        after_seq: i64,
        cards: Option<&[CardId]>,
        f: &mut dyn FnMut(i64, &StatsReviewRow),
    ) -> StorageResult<()> {
        self.for_each_stats_review(after_seq, cards, &mut |seq, r| {
            f(seq, &StatsReviewRow::of(r));
        })
    }
    /// Current [`ReviewLogMark`].
    fn review_log_mark(&self) -> StorageResult<ReviewLogMark>;
    /// Review rows (any mode) with sequence ≤ `seq`.
    fn review_count_upto(&self, seq: i64) -> StorageResult<i64>;
    /// v1.4 Z2: the insertion mark of the card table (a card inserted later has a higher
    /// one); `None` where the store has no such order (the breakdown cache then fingerprints
    /// every card).
    fn card_seq_mark(&self) -> StorageResult<Option<i64>> {
        Ok(None)
    }
    /// Cards inserted after `mark` (see [`Self::card_seq_mark`]).
    fn cards_after_seq(&self, mark: i64) -> StorageResult<Vec<CardId>> {
        let _ = mark;
        Ok(Vec::new())
    }
    /// Review rows (any mode) of `cards` with sequence ≤ `seq`.
    fn review_count_upto_for(&self, seq: i64, cards: &[CardId]) -> StorageResult<i64> {
        let _ = (seq, cards);
        Ok(0)
    }
    /// The cache of `calendar`.
    fn stats_cache(&self, calendar: &str) -> StorageResult<Option<StatsCacheRecord>>;
    /// Due times in `[from, to)` of live, unsuspended, scheduled (non-new) cards of live notes.
    fn due_times(&self, from: Timestamp, to: Timestamp) -> StorageResult<Vec<Timestamp>>;
    /// [`StatsRead::due_times`] counted per day, sorted by day: `(day, cards)` where `day` is
    /// `(due + shift_ms).div_euclid(MILLIS_PER_DAY)` (a study calendar's day index). v1.0: the
    /// load balancer asks this on every answer, so the count stays in SQL.
    fn due_counts_by_day(
        &self,
        from: Timestamp,
        to: Timestamp,
        shift_ms: i64,
    ) -> StorageResult<Vec<(i64, u32)>> {
        let mut out: Vec<(i64, u32)> = Vec::new();
        for t in self.due_times(from, to)? {
            let day = t.0.saturating_add(shift_ms).div_euclid(fc_core::time::MILLIS_PER_DAY);
            match out.iter_mut().find(|(d, _)| *d == day) {
                Some((_, n)) => *n += 1,
                None => out.push((day, 1)),
            }
        }
        out.sort_unstable();
        Ok(out)
    }
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
