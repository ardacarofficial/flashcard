//! Statistics & smart planning (v0.6, migration 0500): review rows by rowid, the statistics
//! cache, due times, exam dates, goals.

use super::study::{review_row, REVIEW_COLS};
use super::{all, exec, id, json, one, opt_id, opt_ms, opt_ts, to_json, ts};
use super::{Mode, SqliteTx, WriteMode};
use crate::error::StorageResult;
use crate::repo::PRACTICE_MODE_SQL;
use crate::repo::{
    ExamRecord, GoalRecord, ReviewLogMark, ReviewLogRecord, StatsCacheRecord, StatsRead, StatsWrite,
};
use fc_core::{CardId, ExamId, GoalId, Timestamp};
use rusqlite::{params, Row};

const REVIEWS: &str = "review_log";
const CACHE: &str = "stats_day_cache";
const META: &str = "stats_cache_meta";
const EXAMS: &str = "exam_dates";
const GOALS: &str = "goals";
const EXAM_COLS: &str = "id, name, date, deck_id, query, created_at, updated_at";
const GOAL_COLS: &str = "id, kind, target, deadline, query, created_at, updated_at";

fn exam_row(r: &Row<'_>) -> rusqlite::Result<ExamRecord> {
    Ok(ExamRecord {
        id: id(r, 0)?,
        name: r.get(1)?,
        date: r.get(2)?,
        deck_id: opt_id(r, 3)?,
        query: r.get(4)?,
        created_at: ts(r, 5)?,
        updated_at: ts(r, 6)?,
    })
}

fn goal_row(r: &Row<'_>) -> rusqlite::Result<GoalRecord> {
    Ok(GoalRecord {
        id: id(r, 0)?,
        kind: r.get(1)?,
        target: r.get(2)?,
        deadline: opt_ts(r, 3)?,
        query: r.get(4)?,
        created_at: ts(r, 5)?,
        updated_at: ts(r, 6)?,
    })
}

impl<M: Mode> StatsRead for SqliteTx<'_, M> {
    fn stats_reviews(
        &self,
        after_seq: i64,
        cards: Option<&[CardId]>,
    ) -> StorageResult<Vec<(i64, ReviewLogRecord)>> {
        all(
            self.conn(),
            REVIEWS,
            &format!(
                "SELECT {REVIEW_COLS}, rowid FROM review_log
                 WHERE rowid > ?1 AND NOT {PRACTICE_MODE_SQL}
                   AND (?2 IS NULL OR card_id IN (SELECT value FROM json_each(?2)))
                 ORDER BY rowid"
            ),
            params![after_seq, cards.map(super::json_list)],
            |r| Ok((r.get::<_, i64>(15)?, review_row(r)?)),
        )
    }

    fn review_log_mark(&self) -> StorageResult<ReviewLogMark> {
        let max_seq = one(
            self.conn(),
            REVIEWS,
            "SELECT coalesce(max(rowid), 0) FROM review_log",
            [],
            |r| r.get::<_, i64>(0),
        )?
        .unwrap_or(0);
        let generation = one(
            self.conn(),
            "review_log_generation",
            "SELECT n FROM review_log_generation WHERE id = 1",
            [],
            |r| r.get::<_, i64>(0),
        )?
        .unwrap_or(0);
        Ok(ReviewLogMark {
            max_seq,
            generation,
        })
    }

    fn review_count_upto(&self, seq: i64) -> StorageResult<i64> {
        Ok(one(
            self.conn(),
            REVIEWS,
            "SELECT count(*) FROM review_log WHERE rowid <= ?1",
            [seq],
            |r| r.get::<_, i64>(0),
        )?
        .unwrap_or(0))
    }

    fn stats_cache(&self, calendar: &str) -> StorageResult<Option<StatsCacheRecord>> {
        let Some((upto_seq, row_count, generation, updated_at)) = one(
            self.conn(),
            META,
            "SELECT upto_seq, row_count, generation, updated_at FROM stats_cache_meta
             WHERE calendar = ?1",
            [calendar],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, ts(r, 3)?)),
        )?
        else {
            return Ok(None);
        };
        let days = all(
            self.conn(),
            CACHE,
            "SELECT day, data FROM stats_day_cache WHERE calendar = ?1 ORDER BY day",
            [calendar],
            |r| Ok((r.get::<_, i64>(0)?, json(r, 1)?)),
        )?;
        Ok(Some(StatsCacheRecord {
            calendar: calendar.to_owned(),
            upto_seq,
            row_count,
            generation,
            days,
            updated_at,
        }))
    }

    fn due_times(&self, from: Timestamp, to: Timestamp) -> StorageResult<Vec<Timestamp>> {
        all(
            self.conn(),
            "cards",
            "SELECT c.due FROM cards c
             WHERE c.due >= ?1 AND c.due < ?2
               AND c.state IN ('learning', 'review', 'relearning')
               AND c.suspended = 0 AND c.deleted_at IS NULL
               AND EXISTS (SELECT 1 FROM notes n WHERE n.id = c.note_id AND n.deleted_at IS NULL)
             ORDER BY c.due",
            params![from.0, to.0],
            |r| ts(r, 0),
        )
    }

    fn list_exams(&self) -> StorageResult<Vec<ExamRecord>> {
        all(
            self.conn(),
            EXAMS,
            &format!("SELECT {EXAM_COLS} FROM exam_dates ORDER BY date, name, id"),
            [],
            exam_row,
        )
    }

    fn get_exam(&self, exam: ExamId) -> StorageResult<Option<ExamRecord>> {
        one(
            self.conn(),
            EXAMS,
            &format!("SELECT {EXAM_COLS} FROM exam_dates WHERE id = ?1"),
            [exam.to_string()],
            exam_row,
        )
    }

    fn list_goals(&self) -> StorageResult<Vec<GoalRecord>> {
        all(
            self.conn(),
            GOALS,
            &format!("SELECT {GOAL_COLS} FROM goals ORDER BY created_at, id"),
            [],
            goal_row,
        )
    }

    fn get_goal(&self, goal: GoalId) -> StorageResult<Option<GoalRecord>> {
        one(
            self.conn(),
            GOALS,
            &format!("SELECT {GOAL_COLS} FROM goals WHERE id = ?1"),
            [goal.to_string()],
            goal_row,
        )
    }
}

impl StatsWrite for SqliteTx<'_, WriteMode> {
    fn put_stats_cache(&mut self, rec: &StatsCacheRecord) -> StorageResult<()> {
        exec(
            self.conn(),
            CACHE,
            "DELETE FROM stats_day_cache WHERE calendar = ?1",
            [&rec.calendar],
        )?;
        for (day, data) in &rec.days {
            exec(
                self.conn(),
                CACHE,
                "INSERT INTO stats_day_cache (calendar, day, data) VALUES (?1, ?2, ?3)",
                params![rec.calendar, day, to_json(CACHE, data)?],
            )?;
        }
        exec(
            self.conn(),
            META,
            "INSERT INTO stats_cache_meta (calendar, upto_seq, row_count, generation, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(calendar) DO UPDATE SET upto_seq = excluded.upto_seq,
                 row_count = excluded.row_count, generation = excluded.generation,
                 updated_at = excluded.updated_at",
            params![
                rec.calendar,
                rec.upto_seq,
                rec.row_count,
                rec.generation,
                rec.updated_at.0
            ],
        )?;
        Ok(())
    }

    fn upsert_exam(&mut self, e: &ExamRecord) -> StorageResult<()> {
        exec(
            self.conn(),
            EXAMS,
            &format!(
                "INSERT INTO exam_dates ({EXAM_COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(id) DO UPDATE SET name = excluded.name, date = excluded.date,
                     deck_id = excluded.deck_id, query = excluded.query,
                     created_at = excluded.created_at, updated_at = excluded.updated_at"
            ),
            params![
                e.id.to_string(),
                e.name,
                e.date,
                e.deck_id.map(|d| d.to_string()),
                e.query,
                e.created_at.0,
                e.updated_at.0
            ],
        )?;
        Ok(())
    }

    fn delete_exam(&mut self, exam: ExamId) -> StorageResult<bool> {
        Ok(exec(
            self.conn(),
            EXAMS,
            "DELETE FROM exam_dates WHERE id = ?1",
            [exam.to_string()],
        )? > 0)
    }

    fn upsert_goal(&mut self, g: &GoalRecord) -> StorageResult<()> {
        exec(
            self.conn(),
            GOALS,
            &format!(
                "INSERT INTO goals ({GOAL_COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(id) DO UPDATE SET kind = excluded.kind, target = excluded.target,
                     deadline = excluded.deadline, query = excluded.query,
                     created_at = excluded.created_at, updated_at = excluded.updated_at"
            ),
            params![
                g.id.to_string(),
                g.kind,
                g.target,
                opt_ms(g.deadline),
                g.query,
                g.created_at.0,
                g.updated_at.0
            ],
        )?;
        Ok(())
    }

    fn delete_goal(&mut self, goal: GoalId) -> StorageResult<bool> {
        Ok(exec(
            self.conn(),
            GOALS,
            "DELETE FROM goals WHERE id = ?1",
            [goal.to_string()],
        )? > 0)
    }
}
