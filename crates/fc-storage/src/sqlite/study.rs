//! Study: sessions, review log, study-queue candidates.

use super::cards::{card_row, COLS as CARD_COLS};
use super::{all, enum_col, enum_str, exec, id, json, one, opt_id, opt_ms, opt_ts, to_json, ts};
use super::{Mode, SqliteTx, WriteMode};
use crate::error::StorageResult;
use crate::repo::{
    ReviewLogRecord, SessionQueueRow, SessionRecord, StudyCardFilter, StudyRead, StudyWrite,
};
use fc_core::card::Card;
use fc_core::{CardId, ReviewLogId, SessionId, Timestamp};
use rusqlite::{params, Row};

const SESSIONS: &str = "sessions";
const QUEUE: &str = "session_queue";
const REVIEWS: &str = "review_log";
const SESSION_COLS: &str = "id, kind, query, config, state, started_at, ended_at";
pub(super) const REVIEW_COLS: &str = "id, card_id, session_id, rating, state_before, state_after, \
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

pub(super) fn review_row(r: &Row<'_>) -> rusqlite::Result<ReviewLogRecord> {
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
        // Only the conditions that are set (v1.0): `?n IS NULL OR …` keeps SQLite off the
        // indexes, and an empty classification list still ran `json_each` for every card.
        let mut conds = vec![
            "c.deleted_at IS NULL".to_owned(),
            "EXISTS (SELECT 1 FROM notes n WHERE n.id = c.note_id AND n.deleted_at IS NULL)"
                .to_owned(),
        ];
        let mut args: Vec<String> = Vec::new();
        let arg = |v: String, args: &mut Vec<String>| {
            args.push(v);
            args.len()
        };
        if let Some(decks) = filter.deck_ids.as_deref() {
            let n = arg(super::json_list(decks), &mut args);
            conds.push(format!("c.deck_id IN (SELECT value FROM json_each(?{n}))"));
        }
        if let Some(cards) = filter.card_ids.as_deref() {
            let n = arg(super::json_list(cards), &mut args);
            conds.push(format!("c.id IN (SELECT value FROM json_each(?{n}))"));
        }
        if let Some(states) = states {
            let n = arg(states, &mut args);
            conds.push(format!("c.state IN (SELECT value FROM json_each(?{n}))"));
        }
        if !filter.include_suspended {
            conds.push("c.suspended = 0".to_owned());
        }
        if !conditions.is_empty() {
            let n = arg(to_json("cards", &conditions)?, &mut args);
            conds.push(format!(
                "NOT EXISTS (
                     SELECT 1 FROM json_each(?{n}) cond
                     WHERE NOT EXISTS (
                         SELECT 1 FROM attribute_values v
                         WHERE v.attr_id = json_extract(cond.value, '$.a')
                           AND v.value IN (SELECT value FROM json_each(
                                               json_extract(cond.value, '$.v')))
                           AND ((v.entity_kind = 'card' AND v.entity_id = c.id)
                                OR (v.entity_kind = 'note' AND v.entity_id = c.note_id))))"
            ));
        }
        let base = conds.join(" AND ");
        if filter.due_before.is_none() && filter.new_first.is_none() {
            return all(
                self.conn(),
                "cards",
                &format!(
                    "SELECT {CARD_COLS} FROM cards c WHERE {base} ORDER BY c.created_at, c.id"
                ),
                rusqlite::params_from_iter(args.iter()),
                card_row,
            );
        }
        // v1.0: cards in progress through the due indexes, new cards in creation order up
        // to the limit (a normal session's queue no longer reads every card).
        let started = "c.state IN ('learning', 'review', 'relearning')";
        let mut due_args = args.clone();
        let sql = match filter.due_before {
            // Two branches so each walks `cards_due_active` (an `OR` would scan).
            Some(t) => {
                due_args.push(t.0.to_string());
                let n = due_args.len();
                format!(
                    "SELECT {CARD_COLS} FROM cards c
                     WHERE {base} AND {started} AND c.due < CAST(?{n} AS INTEGER)
                     UNION ALL
                     SELECT {CARD_COLS} FROM cards c WHERE {base} AND {started} AND c.due IS NULL"
                )
            }
            None => format!("SELECT {CARD_COLS} FROM cards c WHERE {base} AND {started}"),
        };
        let mut out = all(
            self.conn(),
            "cards",
            &sql,
            rusqlite::params_from_iter(due_args.iter()),
            card_row,
        )?;
        let mut new_args = args;
        let (order, limit) = match filter.new_first {
            Some(f) => {
                new_args.push(f.n.to_string());
                let dir = if f.newest { "DESC" } else { "ASC" };
                (
                    format!("c.created_at {dir}, c.id {dir}"),
                    format!(" LIMIT CAST(?{} AS INTEGER)", new_args.len()),
                )
            }
            None => ("c.created_at, c.id".to_owned(), String::new()),
        };
        out.extend(all(
            self.conn(),
            "cards",
            &format!(
                "SELECT {CARD_COLS} FROM cards c WHERE {base} AND c.state = 'new'
                 ORDER BY {order}{limit}"
            ),
            rusqlite::params_from_iter(new_args.iter()),
            card_row,
        )?);
        out.sort_by_key(|c| (c.created_at, c.id));
        Ok(out)
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

    fn session_queue(&self, session: SessionId) -> StorageResult<Vec<SessionQueueRow>> {
        all(
            self.conn(),
            QUEUE,
            "SELECT seq, card_id, note_id, kind, due FROM session_queue
             WHERE session_id = ?1 ORDER BY seq",
            [session.to_string()],
            |r| {
                Ok(SessionQueueRow {
                    seq: r.get(0)?,
                    card_id: id(r, 1)?,
                    note_id: id(r, 2)?,
                    kind: r.get(3)?,
                    due: opt_ts(r, 4)?,
                })
            },
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

    fn edit_session_queue(
        &mut self,
        session: SessionId,
        clear: bool,
        remove: &[i64],
        add: &[SessionQueueRow],
    ) -> StorageResult<()> {
        let sid = session.to_string();
        if clear {
            exec(
                self.conn(),
                QUEUE,
                "DELETE FROM session_queue WHERE session_id = ?1",
                [&sid],
            )?;
        }
        for seq in remove {
            exec(
                self.conn(),
                QUEUE,
                "DELETE FROM session_queue WHERE session_id = ?1 AND seq = ?2",
                params![sid, seq],
            )?;
        }
        for r in add {
            exec(
                self.conn(),
                QUEUE,
                "INSERT INTO session_queue (session_id, seq, card_id, note_id, kind, due)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    sid,
                    r.seq,
                    r.card_id.to_string(),
                    r.note_id.to_string(),
                    r.kind,
                    opt_ms(r.due),
                ],
            )?;
        }
        Ok(())
    }
}
