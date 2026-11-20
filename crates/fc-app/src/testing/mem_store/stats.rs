//! [`MemTx`] repository traits: statistics & planning (v0.6).
//!
//! The in-memory review log has no insertion order, so sequence numbers are positions in
//! `(created_at, id)` order and the generation is a hash of **all** rows: any change (appends
//! included) invalidates a cache, which is always correct (the app then recomputes).

use super::*;
use fc_core::{ExamId, GoalId};

/// Statistics cache, exams and goals.
#[derive(Debug, Clone, Default)]
pub struct StatsState {
    cache: BTreeMap<String, StatsCacheRecord>,
    exams: BTreeMap<ExamId, ExamRecord>,
    goals: BTreeMap<GoalId, GoalRecord>,
}

impl MemTx<'_> {
    fn ordered_reviews(&self) -> Vec<&ReviewLogRecord> {
        let mut all: Vec<&ReviewLogRecord> = self.s().domains.study.reviews.values().collect();
        all.sort_by_key(|r| (r.created_at, r.id));
        all
    }
}

impl StatsRead for MemTx<'_> {
    fn stats_reviews(
        &self,
        after_seq: i64,
        cards: Option<&[CardId]>,
    ) -> StorageResult<Vec<(i64, ReviewLogRecord)>> {
        Ok(self
            .ordered_reviews()
            .into_iter()
            .enumerate()
            .map(|(i, r)| (i64::try_from(i).unwrap_or(i64::MAX).saturating_add(1), r))
            .filter(|(seq, r)| {
                *seq > after_seq
                    && r.mode != "trial"
                    && cards.is_none_or(|c| c.contains(&r.card_id))
            })
            .map(|(seq, r)| (seq, r.clone()))
            .collect())
    }

    fn review_log_mark(&self) -> StorageResult<ReviewLogMark> {
        let all = self.ordered_reviews();
        let text: String = all.iter().map(|r| format!("{r:?}\n")).collect();
        Ok(ReviewLogMark {
            max_seq: i64::try_from(all.len()).unwrap_or(i64::MAX),
            // Reinterpreting the 64 hash bits is the point.
            generation: fc_core::note::fnv1a64(text.as_bytes()) as i64,
        })
    }

    fn review_count_upto(&self, seq: i64) -> StorageResult<i64> {
        let n = i64::try_from(self.s().domains.study.reviews.len()).unwrap_or(i64::MAX);
        Ok(n.min(seq.max(0)))
    }

    fn stats_cache(&self, calendar: &str) -> StorageResult<Option<StatsCacheRecord>> {
        Ok(self.s().domains.stats.cache.get(calendar).cloned())
    }

    fn due_times(&self, from: Timestamp, to: Timestamp) -> StorageResult<Vec<Timestamp>> {
        let s = self.s();
        let mut out: Vec<Timestamp> = s
            .cards
            .values()
            .filter(|c| c.deleted_at.is_none() && !c.suspended)
            .filter(|c| s.notes.get(&c.note_id).is_some_and(|n| n.deleted_at.is_none()))
            .filter(|c| c.scheduling.state != fc_core::card::CardState::New)
            .filter_map(|c| c.scheduling.due)
            .filter(|d| *d >= from && *d < to)
            .collect();
        out.sort();
        Ok(out)
    }

    fn list_exams(&self) -> StorageResult<Vec<ExamRecord>> {
        let mut out: Vec<ExamRecord> = self.s().domains.stats.exams.values().cloned().collect();
        out.sort_by(|a, b| (&a.date, &a.name, a.id).cmp(&(&b.date, &b.name, b.id)));
        Ok(out)
    }

    fn get_exam(&self, id: ExamId) -> StorageResult<Option<ExamRecord>> {
        Ok(self.s().domains.stats.exams.get(&id).cloned())
    }

    fn list_goals(&self) -> StorageResult<Vec<GoalRecord>> {
        let mut out: Vec<GoalRecord> = self.s().domains.stats.goals.values().cloned().collect();
        out.sort_by_key(|g| (g.created_at, g.id));
        Ok(out)
    }

    fn get_goal(&self, id: GoalId) -> StorageResult<Option<GoalRecord>> {
        Ok(self.s().domains.stats.goals.get(&id).cloned())
    }
}

impl StatsWrite for MemTx<'_> {
    fn put_stats_cache(&mut self, record: &StatsCacheRecord) -> StorageResult<()> {
        self.m()
            .domains
            .stats
            .cache
            .insert(record.calendar.clone(), record.clone());
        Ok(())
    }

    fn upsert_exam(&mut self, exam: &ExamRecord) -> StorageResult<()> {
        if let Some(d) = exam.deck_id {
            if !self.s().decks.contains_key(&d) {
                return Err(StorageError::Constraint {
                    constraint: "foreign_key".into(),
                });
            }
        }
        self.m().domains.stats.exams.insert(exam.id, exam.clone());
        Ok(())
    }

    fn delete_exam(&mut self, id: ExamId) -> StorageResult<bool> {
        Ok(self.m().domains.stats.exams.remove(&id).is_some())
    }

    fn upsert_goal(&mut self, goal: &GoalRecord) -> StorageResult<()> {
        self.m().domains.stats.goals.insert(goal.id, goal.clone());
        Ok(())
    }

    fn delete_goal(&mut self, id: GoalId) -> StorageResult<bool> {
        Ok(self.m().domains.stats.goals.remove(&id).is_some())
    }
}
