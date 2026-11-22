//! `note_meanings`, `note_accepted_answers` (v1.4 W2).

use super::{all, exec, id, json, to_json, ts, Mode, SqliteTx, WriteMode};
use crate::error::StorageResult;
use crate::repo::{AcceptedAnswerRecord, MeaningRead, MeaningWrite, NoteMeaningRecord};
use fc_core::NoteId;
use rusqlite::params;

const MEANINGS: &str = "note_meanings";
const ANSWERS: &str = "note_accepted_answers";

impl<M: Mode> MeaningRead for SqliteTx<'_, M> {
    fn get_note_meaning(&self, note: NoteId) -> StorageResult<Option<NoteMeaningRecord>> {
        let mut rows = all(
            self.conn(),
            MEANINGS,
            "SELECT note_id, field_key, status, meaning, phrase, expression, also_correct,
                    not_this, confidence, edited, original, model, created_at, updated_at
             FROM note_meanings WHERE note_id = ?1",
            [note.to_string()],
            |r| {
                Ok(NoteMeaningRecord {
                    note_id: id(r, 0)?,
                    field_key: r.get(1)?,
                    status: r.get(2)?,
                    meaning: r.get(3)?,
                    phrase: r.get(4)?,
                    expression: r.get::<_, i64>(5)? != 0,
                    also_correct: json(r, 6)?,
                    not_this: json(r, 7)?,
                    confidence: r.get(8)?,
                    edited: r.get::<_, i64>(9)? != 0,
                    original: r.get(10)?,
                    model: r.get(11)?,
                    created_at: ts(r, 12)?,
                    updated_at: ts(r, 13)?,
                })
            },
        )?;
        Ok(rows.pop())
    }

    fn list_accepted_answers(&self, note: NoteId) -> StorageResult<Vec<AcceptedAnswerRecord>> {
        all(
            self.conn(),
            ANSWERS,
            "SELECT note_id, field_key, answer, position FROM note_accepted_answers
             WHERE note_id = ?1 ORDER BY field_key, position, answer",
            [note.to_string()],
            |r| {
                Ok(AcceptedAnswerRecord {
                    note_id: id(r, 0)?,
                    field_key: r.get(1)?,
                    answer: r.get(2)?,
                    position: r.get(3)?,
                })
            },
        )
    }
}

impl MeaningWrite for SqliteTx<'_, WriteMode> {
    fn put_note_meaning(&mut self, rec: &NoteMeaningRecord) -> StorageResult<()> {
        exec(
            self.conn(),
            MEANINGS,
            "INSERT OR REPLACE INTO note_meanings
                 (note_id, field_key, status, meaning, phrase, expression, also_correct, not_this,
                  confidence, edited, original, model, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            params![
                rec.note_id.to_string(),
                rec.field_key,
                rec.status,
                rec.meaning,
                rec.phrase,
                i64::from(rec.expression),
                to_json(MEANINGS, &rec.also_correct)?,
                to_json(MEANINGS, &rec.not_this)?,
                rec.confidence,
                i64::from(rec.edited),
                rec.original,
                rec.model,
                rec.created_at.0,
                rec.updated_at.0
            ],
        )?;
        Ok(())
    }

    fn delete_note_meaning(&mut self, note: NoteId) -> StorageResult<()> {
        exec(
            self.conn(),
            MEANINGS,
            "DELETE FROM note_meanings WHERE note_id = ?1",
            [note.to_string()],
        )?;
        Ok(())
    }

    fn set_accepted_answers(
        &mut self,
        note: NoteId,
        rows: &[AcceptedAnswerRecord],
    ) -> StorageResult<()> {
        exec(
            self.conn(),
            ANSWERS,
            "DELETE FROM note_accepted_answers WHERE note_id = ?1",
            [note.to_string()],
        )?;
        for r in rows {
            exec(
                self.conn(),
                ANSWERS,
                "INSERT OR IGNORE INTO note_accepted_answers (note_id, field_key, answer, position)
                 VALUES (?1, ?2, ?3, ?4)",
                params![note.to_string(), r.field_key, r.answer, r.position],
            )?;
        }
        Ok(())
    }
}
