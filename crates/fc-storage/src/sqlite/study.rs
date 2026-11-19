//! Study: sessions, review log, study-queue candidates.

use super::cards::{card_row, COLS as CARD_COLS};
use super::{all, enum_col, enum_str, exec, id, json, one, opt_id, opt_ms, opt_ts, to_json, ts};
use super::{Mode, SqliteTx, WriteMode};
use crate::error::StorageResult;
use crate::repo::{ReviewLogRecord, SessionRecord, StudyCardFilter, StudyRead, StudyWrite};
use fc_core::card::Card;
use fc_core::{CardId, ReviewLogId, SessionId, Timestamp};
use rusqlite::{params, Row};

const SESSIONS: &str = "sessions";
const REVIEWS: &str = "review_log";
const SESSION_COLS: &str = "id, kind, query, config, state, started_at, ended_at";
const REVIEW_COLS: &str = "id, card_id, session_id, rating, state_before, state_after, \
                           scheduled_days, elapsed_days, duration_ms, answer_duration_ms, \
                           hints_used, mode, stability_after, difficulty_after, created_at";

fn session_row(r: &Row<'_>) -> rusqlite::Result<SessionRecord> {
    Ok(SessionRecord {
        id: id(r, 0)?,
        kind: r.get(1)?,
        query: r.get(2)?,
        config: json(r, 3)?,
        state: json(r, 4)?,
        started_at: ts(r, 5)?,
        ended_at: opt_ts(r, 6)?,
    })
}

fn review_row(r: &Row<'_>) -> rusqlite::Result<ReviewLogRecord> {
    Ok(ReviewLogRecord {
        id: id(r, 0)?,
        card_id: id(r, 1)?,
        session_id: opt_id(r, 2)?,
        rating: enum_col(r, 3)?,
        state_before: enum_col(r, 4)?,
        state_after: enum_col(r, 5)?,
        scheduled_days: r.get(6)?,
        elapsed_days: r.get(7)?,
        duration_ms: r.get(8)?,
        answer_duration_ms: r.get(9)?,
        hints_used: r.get(10)?,
        mode: r.get(11)?,
        stability_after: r.get(12)?,
        difficulty_after: r.get(13)?,
        created_at: ts(r, 14)?,
    })
}

impl<M: Mode> StudyRead for SqliteTx<'_, M> {
    fn get_session(&self, session: SessionId) -> StorageResult<Option<SessionRecord>> {
        one(
            self.conn(),
            SESSIONS,
            &format!("SELECT {SESSION_COLS} FROM sessions WHERE id = ?1"),
            [session.to_string()],
            session_row,
        )
    }

    fn latest_open_session(&self) -> StorageResult<Option<SessionRecord>> {
        one(
            self.conn(),
            SESSIONS,
            &format!(
                "SELECT {SESSION_COLS} FROM sessions WHERE ended_at IS NULL
                 ORDER BY started_at DESC, id DESC LIMIT 1"
            ),
            [],
            session_row,
        )
    }

    fn study_cards(&self, filter: &StudyCardFilter) -> StorageResult<Vec<Card>> {
        let states = filter
            .states
            .as_ref()
            .map(|s| {
                s.iter()
                    .map(|st| enum_str("cards", st))
                    .collect::<StorageResult<Vec<String>>>()
            })
            .transpose()?
            .map(|s| super::json_list(&s));
        // Conditions as one JSON param: [{"a": attr_id, "v": [values]}]; every condition must
        // be met by a card-level or note-level value.
        let conditions: Vec<serde_json::Value> = filter
            .attributes
            .iter()
            .map(|(attr, values)| serde_json::json!({ "a": attr.to_string(), "v": values }))
            .collect();
        let conditions = to_json("cards", &conditions)?;
        all(
            self.conn(),
            "cards",
            &format!(
                "SELECT {CARD_COLS} FROM cards c
                 WHERE c.deleted_at IS NULL
                   AND EXISTS (SELECT 1 FROM notes n WHERE n.id = c.note_id
                               AND n.deleted_at IS NULL)
                   AND (?1 IS NULL OR c.deck_id IN (SELECT value FROM json_each(?1)))
                   AND (?2 IS NULL OR c.id IN (SELECT value FROM json_each(?2)))
                   AND (?3 IS NULL OR c.state IN (SELECT value FROM json_each(?3)))
                   AND (?4 OR c.suspended = 0)
                   AND NOT EXISTS (
                       SELECT 1 FROM json_each(?5) cond
                       WHERE NOT EXISTS (
                           SELECT 1 FROM attribute_values v
                           WHERE v.attr_id = json_extract(cond.value, '$.a')
                             AND v.value IN (SELECT value FROM json_each(
                                                 json_extract(cond.value, '$.v')))
                             AND ((v.entity_kind = 'card' AND v.entity_id = c.id)
                                  OR (v.entity_kind = 'note' AND v.entity_id = c.note_id))))
                 ORDER BY c.created_at, c.id"
            ),
            params![
                filter.deck_ids.as_deref().map(super::json_list),
                filter.card_ids.as_deref().map(super::json_list),
                states,
                filter.include_suspended,
                conditions,
            ],
            card_row,
        )
    }

    fn get_review_log(&self, review: ReviewLogId) -> StorageResult<Option<ReviewLogRecord>> {
        one(
            self.conn(),
            REVIEWS,
            &format!("SELECT {REVIEW_COLS} FROM review_log WHERE id = ?1"),
            [review.to_string()],
            review_row,
        )
    }

    fn review_logs_for_card(&self, card: CardId) -> StorageResult<Vec<ReviewLogRecord>> {
        all(
            self.conn(),
            REVIEWS,
            &format!(
                "SELECT {REVIEW_COLS} FROM review_log WHERE card_id = ?1
                 ORDER BY created_at, id"
            ),
            [card.to_string()],
            review_row,
        )
    }

    fn review_logs_since(
        &self,
        since: Timestamp,
        mode: &str,
    ) -> StorageResult<Vec<ReviewLogRecord>> {
        all(
            self.conn(),
            REVIEWS,
            &format!(
                "SELECT {REVIEW_COLS} FROM review_log WHERE created_at >= ?1 AND mode = ?2
                 ORDER BY created_at, id"
            ),
            params![since.0, mode],
            review_row,
        )
    }
}

impl StudyWrite for SqliteTx<'_, WriteMode> {
    fn upsert_session(&mut self, s: &SessionRecord) -> StorageResult<()> {
        exec(
            self.conn(),
            SESSIONS,
            &format!(
                "INSERT INTO sessions ({SESSION_COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(id) DO UPDATE SET kind = excluded.kind, query = excluded.query,
                     config = excluded.config, state = excluded.state,
                     started_at = excluded.started_at, ended_at = excluded.ended_at"
            ),
            params![
                s.id.to_string(),
                s.kind,
                s.query,
                to_json(SESSIONS, &s.config)?,
                to_json(SESSIONS, &s.state)?,
                s.started_at.0,
                opt_ms(s.ended_at),
            ],
        )?;
        Ok(())
    }

    fn insert_review_log(&mut self, r: &ReviewLogRecord) -> StorageResult<()> {
        exec(
            self.conn(),
            REVIEWS,
            &format!(
                "INSERT INTO review_log ({REVIEW_COLS})
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)"
            ),
            params![
                r.id.to_string(),
                r.card_id.to_string(),
                r.session_id.map(|s| s.to_string()),
                enum_str(REVIEWS, &r.rating)?,
                enum_str(REVIEWS, &r.state_before)?,
                enum_str(REVIEWS, &r.state_after)?,
                r.scheduled_days,
                r.elapsed_days,
                r.duration_ms,
                r.answer_duration_ms,
                r.hints_used,
                r.mode,
                r.stability_after,
                r.difficulty_after,
                r.created_at.0,
            ],
        )?;
        Ok(())
    }

    fn delete_review_log(&mut self, review: ReviewLogId) -> StorageResult<bool> {
        Ok(exec(
            self.conn(),
            REVIEWS,
            "DELETE FROM review_log WHERE id = ?1",
            [review.to_string()],
        )? > 0)
    }
}
