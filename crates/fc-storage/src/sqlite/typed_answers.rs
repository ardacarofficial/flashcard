//! `typed_answers` (v1.4 Z1).

use super::{all, exec, id, json_list, one, opt_id, ts, Mode, SqliteTx, WriteMode};
use crate::error::StorageResult;
use crate::repo::{TypedAnswerRead, TypedAnswerRecord, TypedAnswerWrite};
use fc_core::{CardId, ReviewLogId};
use rusqlite::{params, Row};

const TABLE: &str = "typed_answers";

const COLS: &str = "id, card_id, review_log_id, mode, typed, correct, created_at";

fn row(r: &Row<'_>) -> rusqlite::Result<TypedAnswerRecord> {
    Ok(TypedAnswerRecord {
        id: r.get(0)?,
        card_id: id(r, 1)?,
        review_log_id: opt_id(r, 2)?,
        mode: r.get(3)?,
        typed: r.get(4)?,
        correct: r.get::<_, Option<i64>>(5)?.map(|c| c != 0),
        created_at: ts(r, 6)?,
    })
}

impl<M: Mode> TypedAnswerRead for SqliteTx<'_, M> {
    fn typed_answers_of_card(&self, card_id: CardId) -> StorageResult<Vec<TypedAnswerRecord>> {
        all(
            self.conn(),
            TABLE,
            &format!(
                "SELECT {COLS} FROM typed_answers WHERE card_id = ?1
                 ORDER BY created_at DESC, id DESC"
            ),
            [card_id.to_string()],
            row,
        )
    }

    fn typed_answers_of_reviews(
        &self,
        review_ids: &[ReviewLogId],
    ) -> StorageResult<Vec<TypedAnswerRecord>> {
        if review_ids.is_empty() {
            return Ok(Vec::new());
        }
        all(
            self.conn(),
            TABLE,
            &format!(
                "SELECT {COLS} FROM typed_answers
                 WHERE review_log_id IN (SELECT value FROM json_each(?1))
                 ORDER BY created_at, id"
            ),
            [json_list(review_ids)],
            row,
        )
    }

    fn get_typed_answer(&self, id: i64) -> StorageResult<Option<TypedAnswerRecord>> {
        one(
            self.conn(),
            TABLE,
            &format!("SELECT {COLS} FROM typed_answers WHERE id = ?1"),
            [id],
            row,
        )
    }
}

impl TypedAnswerWrite for SqliteTx<'_, WriteMode> {
    fn insert_typed_answer(&mut self, rec: &TypedAnswerRecord) -> StorageResult<i64> {
        exec(
            self.conn(),
            TABLE,
            "INSERT INTO typed_answers (id, card_id, review_log_id, mode, typed, correct, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                (rec.id > 0).then_some(rec.id),
                rec.card_id.to_string(),
                rec.review_log_id.map(|r| r.to_string()),
                rec.mode,
                rec.typed,
                rec.correct.map(i64::from),
                rec.created_at.0
            ],
        )?;
        Ok(self.conn().last_insert_rowid())
    }

    fn delete_typed_answer(&mut self, id: i64) -> StorageResult<()> {
        exec(
            self.conn(),
            TABLE,
            "DELETE FROM typed_answers WHERE id = ?1",
            [id],
        )?;
        Ok(())
    }
}
