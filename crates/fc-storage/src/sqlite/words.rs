//! The word index (v1.4, migration 1410). The dirty queues are filled by triggers.

use super::{all, enum_col, exec, id, json, json_list, opt_ms, opt_ts, ts};
use super::{Mode, SqliteTx, WriteMode};
use crate::error::StorageResult;
use crate::repo::{
    WordCardRow, WordIndexNoteRecord, WordIndexQuery, WordIndexRead, WordIndexRecord,
    WordIndexWrite, WordMarkRecord, WordNoteSource,
};
use fc_core::NoteId;
use rusqlite::{params, Row};

const NOTES: &str = "word_index_notes";
const WORDS: &str = "word_index";

/// The status of a `word_index` row (`?1` = threshold), as `fc_core::words` derives it.
const STATUS: &str = "CASE WHEN mark IS NOT NULL THEN mark
    WHEN cards > 0 AND max_interval_days >= ?1 THEN 'known'
    WHEN cards > 0 THEN 'learning' ELSE 'new' END";

const WORD_COLS: &str =
    "language, key, mark, mark_at, notes, cards, max_interval_days, last_review, first_seen";
const NOTE_COLS: &str = "note_id, language, key, cards, max_interval_days, last_review, created_at";

fn u32_col(r: &Row<'_>, i: usize) -> rusqlite::Result<u32> {
    Ok(u32::try_from(r.get::<_, i64>(i)?.max(0)).unwrap_or(u32::MAX))
}

fn opt_u32(r: &Row<'_>, i: usize) -> rusqlite::Result<Option<u32>> {
    Ok(r.get::<_, Option<i64>>(i)?
        .map(|v| u32::try_from(v.max(0)).unwrap_or(u32::MAX)))
}

fn word_row(r: &Row<'_>) -> rusqlite::Result<WordIndexRecord> {
    Ok(WordIndexRecord {
        language: r.get(0)?,
        key: r.get(1)?,
        mark: r.get(2)?,
        mark_at: opt_ts(r, 3)?,
        notes: u32_col(r, 4)?,
        cards: u32_col(r, 5)?,
        max_interval_days: opt_u32(r, 6)?,
        last_review: opt_ts(r, 7)?,
        first_seen: opt_ts(r, 8)?,
    })
}

fn note_row(r: &Row<'_>) -> rusqlite::Result<WordIndexNoteRecord> {
    Ok(WordIndexNoteRecord {
        note_id: id(r, 0)?,
        language: r.get(1)?,
        key: r.get(2)?,
        cards: u32_col(r, 3)?,
        max_interval_days: opt_u32(r, 4)?,
        last_review: opt_ts(r, 5)?,
        created_at: ts(r, 6)?,
    })
}

fn keys_json(keys: &[String]) -> String {
    serde_json::to_string(keys).unwrap_or_else(|_| "[]".into())
}

impl<M: Mode> WordIndexRead for SqliteTx<'_, M> {
    fn word_index_get(
        &self,
        language: &str,
        keys: &[String],
    ) -> StorageResult<Vec<WordIndexRecord>> {
        all(
            self.conn(),
            WORDS,
            &format!(
                "SELECT {WORD_COLS} FROM word_index
                 WHERE language = ?1 AND key IN (SELECT value FROM json_each(?2))"
            ),
            params![language, keys_json(keys)],
            word_row,
        )
    }

    fn word_index_page(&self, q: &WordIndexQuery) -> StorageResult<Vec<WordIndexRecord>> {
        let (after_lang, after_key) = q
            .after
            .as_ref()
            .map_or(("", ""), |(l, k)| (l.as_str(), k.as_str()));
        all(
            self.conn(),
            WORDS,
            &format!(
                "SELECT {WORD_COLS} FROM word_index
                 WHERE (?2 IS NULL OR language = ?2)
                   AND (?3 = 0 OR language > ?4 OR (language = ?4 AND key > ?5))
                   AND (?6 = '[]' OR ({STATUS}) IN (SELECT value FROM json_each(?6)))
                 ORDER BY language, key LIMIT ?7"
            ),
            params![
                i64::from(q.threshold_days),
                q.language,
                i64::from(q.after.is_some()),
                after_lang,
                after_key,
                keys_json(&q.statuses),
                i64::from(q.limit),
            ],
            word_row,
        )
    }

    fn word_index_counts(&self, threshold_days: u32) -> StorageResult<Vec<(String, String, u32)>> {
        all(
            self.conn(),
            WORDS,
            &format!(
                "SELECT language, {STATUS} AS status, count(*) FROM word_index
                 GROUP BY language, status ORDER BY language, status"
            ),
            [i64::from(threshold_days)],
            |r| Ok((r.get(0)?, r.get(1)?, u32_col(r, 2)?)),
        )
    }

    fn word_index_unassigned(&self) -> StorageResult<u32> {
        let n: i64 = self.conn().query_row(
            "SELECT count(*) FROM word_index_notes WHERE language = ''",
            [],
            |r| r.get(0),
        )?;
        Ok(u32::try_from(n).unwrap_or(u32::MAX))
    }

    fn word_index_notes_of(
        &self,
        language: &str,
        keys: &[String],
    ) -> StorageResult<Vec<WordIndexNoteRecord>> {
        all(
            self.conn(),
            NOTES,
            &format!(
                "SELECT {NOTE_COLS} FROM word_index_notes
                 WHERE language = ?1 AND key IN (SELECT value FROM json_each(?2))
                 ORDER BY key, created_at, note_id"
            ),
            params![language, keys_json(keys)],
            note_row,
        )
    }

    fn word_index_notes_by_id(&self, notes: &[NoteId]) -> StorageResult<Vec<WordIndexNoteRecord>> {
        all(
            self.conn(),
            NOTES,
            &format!(
                "SELECT {NOTE_COLS} FROM word_index_notes
                 WHERE note_id IN (SELECT value FROM json_each(?1))"
            ),
            [json_list(notes)],
            note_row,
        )
    }

    fn word_index_dump(&self) -> StorageResult<crate::repo::WordIndexDump> {
        let words = all(
            self.conn(),
            WORDS,
            &format!("SELECT {WORD_COLS} FROM word_index ORDER BY language, key"),
            [],
            word_row,
        )?;
        let notes = all(
            self.conn(),
            NOTES,
            &format!("SELECT {NOTE_COLS} FROM word_index_notes ORDER BY note_id"),
            [],
            note_row,
        )?;
        Ok((words, notes))
    }

    fn word_note_sources(&self, notes: &[NoteId]) -> StorageResult<Vec<WordNoteSource>> {
        all(
            self.conn(),
            "notes",
            "SELECT n.id, n.notetype_id, n.home_deck_id, n.fields, n.created_at,
                    (SELECT c.lang FROM subtitle_captures c WHERE c.note_id = n.id
                     ORDER BY c.created_at DESC LIMIT 1),
                    (SELECT l.language FROM word_note_language l WHERE l.note_id = n.id)
             FROM notes n
             WHERE n.id IN (SELECT value FROM json_each(?1)) AND n.deleted_at IS NULL",
            [json_list(notes)],
            |r| {
                Ok(WordNoteSource {
                    note_id: id(r, 0)?,
                    notetype_id: id(r, 1)?,
                    home_deck_id: id(r, 2)?,
                    fields: json(r, 3)?,
                    created_at: ts(r, 4)?,
                    capture_lang: r.get(5)?,
                    known_lang: r.get(6)?,
                })
            },
        )
    }

    fn word_card_rows(&self, notes: &[NoteId]) -> StorageResult<Vec<WordCardRow>> {
        all(
            self.conn(),
            "cards",
            "SELECT note_id, id, state, due, last_review FROM cards
             WHERE note_id IN (SELECT value FROM json_each(?1)) AND deleted_at IS NULL
             ORDER BY note_id, created_at, id",
            [json_list(notes)],
            |r| {
                Ok(WordCardRow {
                    note_id: id(r, 0)?,
                    card_id: id(r, 1)?,
                    state: enum_col(r, 2)?,
                    due: opt_ts(r, 3)?,
                    last_review: opt_ts(r, 4)?,
                })
            },
        )
    }

    fn word_marks_of(&self, language: &str, keys: &[String]) -> StorageResult<Vec<WordMarkRecord>> {
        all(
            self.conn(),
            "word_marks",
            "SELECT language, lemma, status, updated_at FROM word_marks
             WHERE language = ?1 AND lemma IN (SELECT value FROM json_each(?2))",
            params![language, keys_json(keys)],
            |r| {
                Ok(WordMarkRecord {
                    language: r.get(0)?,
                    lemma: r.get(1)?,
                    status: r.get(2)?,
                    updated_at: ts(r, 3)?,
                })
            },
        )
    }

    fn word_index_dirty_count(&self) -> StorageResult<u32> {
        let n: i64 = self.conn().query_row(
            "SELECT (SELECT count(*) FROM word_index_dirty)
                  + (SELECT count(*) FROM word_index_dirty_keys)",
            [],
            |r| r.get(0),
        )?;
        Ok(u32::try_from(n).unwrap_or(u32::MAX))
    }
}

impl WordIndexWrite for SqliteTx<'_, WriteMode> {
    fn take_word_index_dirty(&mut self) -> StorageResult<crate::repo::WordIndexDirty> {
        let notes = all(
            self.conn(),
            "word_index_dirty",
            "SELECT DISTINCT note_id FROM word_index_dirty ORDER BY note_id",
            [],
            |r| id(r, 0),
        )?;
        let keys = all(
            self.conn(),
            "word_index_dirty_keys",
            "SELECT DISTINCT language, key FROM word_index_dirty_keys ORDER BY language, key",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        if !notes.is_empty() {
            exec(
                self.conn(),
                "word_index_dirty",
                "DELETE FROM word_index_dirty",
                [],
            )?;
        }
        if !keys.is_empty() {
            exec(
                self.conn(),
                "word_index_dirty_keys",
                "DELETE FROM word_index_dirty_keys",
                [],
            )?;
        }
        Ok((notes, keys))
    }

    fn reset_word_index(&mut self) -> StorageResult<()> {
        self.conn().execute_batch(
            "DELETE FROM word_index;
             DELETE FROM word_index_notes;
             DELETE FROM word_index_dirty;
             DELETE FROM word_index_dirty_keys;
             INSERT INTO word_index_dirty (note_id) SELECT id FROM notes;
             INSERT INTO word_index_dirty_keys (language, key)
                 SELECT language, lemma FROM word_marks;",
        )?;
        Ok(())
    }

    fn put_word_index_note(&mut self, r: &WordIndexNoteRecord) -> StorageResult<()> {
        exec(
            self.conn(),
            NOTES,
            "INSERT OR REPLACE INTO word_index_notes
                 (note_id, language, key, cards, max_interval_days, last_review, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                r.note_id.to_string(),
                r.language,
                r.key,
                i64::from(r.cards),
                r.max_interval_days.map(i64::from),
                opt_ms(r.last_review),
                r.created_at.0,
            ],
        )?;
        Ok(())
    }

    fn delete_word_index_note(&mut self, note: NoteId) -> StorageResult<()> {
        exec(
            self.conn(),
            NOTES,
            "DELETE FROM word_index_notes WHERE note_id = ?1",
            [note.to_string()],
        )?;
        Ok(())
    }

    fn put_word_index_entry(&mut self, r: &WordIndexRecord) -> StorageResult<()> {
        exec(
            self.conn(),
            WORDS,
            "INSERT OR REPLACE INTO word_index
                 (language, key, mark, mark_at, notes, cards, max_interval_days, last_review,
                  first_seen)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                r.language,
                r.key,
                r.mark,
                opt_ms(r.mark_at),
                i64::from(r.notes),
                i64::from(r.cards),
                r.max_interval_days.map(i64::from),
                opt_ms(r.last_review),
                opt_ms(r.first_seen),
            ],
        )?;
        Ok(())
    }

    fn delete_word_index_entry(&mut self, language: &str, key: &str) -> StorageResult<()> {
        exec(
            self.conn(),
            WORDS,
            "DELETE FROM word_index WHERE language = ?1 AND key = ?2",
            params![language, key],
        )?;
        Ok(())
    }

    fn set_word_note_language(&mut self, note: NoteId, language: &str) -> StorageResult<()> {
        exec(
            self.conn(),
            "word_note_language",
            "INSERT INTO word_note_language (note_id, language) VALUES (?1, ?2)
             ON CONFLICT(note_id) DO UPDATE SET language = ?2 WHERE language <> ?2",
            params![note.to_string(), language],
        )?;
        Ok(())
    }
}
