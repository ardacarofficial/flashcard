//! Reading mode (v0.10, migration 0900): reading items, word marks, vocabulary rows.

use super::{all, count, enum_col, exec, id, json_list, one, opt_id, opt_ms, opt_ts, ts};
use super::{Mode, SqliteTx, WriteMode};
use crate::error::StorageResult;
use crate::repo::{ReadingItemRecord, ReadingRead, ReadingWrite, VocabularyRow, WordMarkRecord};
use fc_core::{ReadingItemId, Timestamp};
use rusqlite::{params, Row};

const ITEMS: &str = "reading_items";
const MARKS: &str = "word_marks";

/// Columns of a full row; listings select `''`/`NULL` for `body`/`cues`.
const COLS: &str = "id, kind, parent_id, title, language, source, body, cues, media_path,
    extract_start, extract_end, position, queued, priority, due, interval_days, read_count,
    last_read, created_at, updated_at, deleted_at, length,
    coalesce(json_array_length(cues), 0)";
const LIST_COLS: &str = "id, kind, parent_id, title, language, source, '', NULL, media_path,
    extract_start, extract_end, position, queued, priority, due, interval_days, read_count,
    last_read, created_at, updated_at, deleted_at, length,
    coalesce(json_array_length(cues), 0)";

fn item_row(r: &Row<'_>) -> rusqlite::Result<ReadingItemRecord> {
    let range = match (r.get::<_, Option<i64>>(9)?, r.get::<_, Option<i64>>(10)?) {
        (Some(s), Some(e)) => Some((s, e)),
        _ => None,
    };
    Ok(ReadingItemRecord {
        id: id(r, 0)?,
        kind: r.get(1)?,
        parent_id: opt_id(r, 2)?,
        title: r.get(3)?,
        language: r.get(4)?,
        source: r.get(5)?,
        body: r.get(6)?,
        cues: r.get(7)?,
        media_path: r.get(8)?,
        extract_range: range,
        position: r.get(11)?,
        queued: r.get::<_, i64>(12)? != 0,
        priority: r.get(13)?,
        due: opt_ts(r, 14)?,
        interval_days: r.get(15)?,
        read_count: r.get(16)?,
        last_read: opt_ts(r, 17)?,
        created_at: ts(r, 18)?,
        updated_at: ts(r, 19)?,
        deleted_at: opt_ts(r, 20)?,
        length: r.get(21)?,
        cue_count: r.get(22)?,
    })
}

impl<M: Mode> ReadingRead for SqliteTx<'_, M> {
    fn list_reading_items(&self, include_deleted: bool) -> StorageResult<Vec<ReadingItemRecord>> {
        all(
            self.conn(),
            ITEMS,
            &format!(
                "SELECT {LIST_COLS} FROM reading_items
                 WHERE ?1 OR deleted_at IS NULL ORDER BY created_at DESC, id DESC"
            ),
            [include_deleted],
            item_row,
        )
    }

    fn get_reading_item(&self, item: ReadingItemId) -> StorageResult<Option<ReadingItemRecord>> {
        one(
            self.conn(),
            ITEMS,
            &format!("SELECT {COLS} FROM reading_items WHERE id = ?1"),
            [item.to_string()],
            item_row,
        )
    }

    fn list_word_marks(&self, language: &str) -> StorageResult<Vec<WordMarkRecord>> {
        all(
            self.conn(),
            MARKS,
            "SELECT language, lemma, status, updated_at FROM word_marks
             WHERE language = ?1 ORDER BY lemma",
            [language],
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

    fn vocabulary_rows(&self) -> StorageResult<Vec<VocabularyRow>> {
        all(
            self.conn(),
            "cards",
            "SELECT c.id, c.note_id,
                    coalesce(nullif(json_extract(n.fields, '$.\"word\"'), ''), n.sort_field),
                    c.state, c.due, c.last_review
             FROM cards c JOIN notes n ON n.id = c.note_id
             WHERE c.deleted_at IS NULL AND n.deleted_at IS NULL
             ORDER BY c.created_at, c.id",
            [],
            |r| {
                Ok(VocabularyRow {
                    card_id: id(r, 0)?,
                    note_id: id(r, 1)?,
                    word: r.get::<_, Option<String>>(2)?.unwrap_or_default(),
                    state: enum_col(r, 3)?,
                    due: opt_ts(r, 4)?,
                    last_review: opt_ts(r, 5)?,
                })
            },
        )
    }
}

impl ReadingWrite for SqliteTx<'_, WriteMode> {
    fn upsert_reading_item(&mut self, i: &ReadingItemRecord) -> StorageResult<()> {
        exec(
            self.conn(),
            ITEMS,
            "INSERT INTO reading_items (id, kind, parent_id, title, language, source, body, cues,
                 media_path, extract_start, extract_end, position, queued, priority, due,
                 interval_days, read_count, last_read, created_at, updated_at, deleted_at, length)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17,
                 ?18, ?19, ?20, ?21, ?22)
             ON CONFLICT(id) DO UPDATE SET kind = ?2, parent_id = ?3, title = ?4,
                 language = ?5, source = ?6, body = ?7, cues = ?8, media_path = ?9,
                 extract_start = ?10, extract_end = ?11, position = ?12, queued = ?13,
                 priority = ?14, due = ?15, interval_days = ?16, read_count = ?17,
                 last_read = ?18, created_at = ?19, updated_at = ?20, deleted_at = ?21,
                 length = ?22",
            params![
                i.id.to_string(),
                i.kind,
                i.parent_id.map(|p| p.to_string()),
                i.title,
                i.language,
                i.source,
                i.body,
                i.cues,
                i.media_path,
                i.extract_range.map(|r| r.0),
                i.extract_range.map(|r| r.1),
                i.position,
                i64::from(i.queued),
                i.priority,
                opt_ms(i.due),
                i.interval_days,
                i.read_count,
                opt_ms(i.last_read),
                i.created_at.0,
                i.updated_at.0,
                opt_ms(i.deleted_at),
                i.length,
            ],
        )?;
        Ok(())
    }

    fn set_reading_items_deleted(
        &mut self,
        ids: &[ReadingItemId],
        deleted_at: Option<Timestamp>,
    ) -> StorageResult<u32> {
        Ok(count(exec(
            self.conn(),
            ITEMS,
            "UPDATE reading_items SET deleted_at = ?2
             WHERE id IN (SELECT value FROM json_each(?1))",
            params![json_list(ids), opt_ms(deleted_at)],
        )?))
    }

    fn purge_reading_items(&mut self, before: Timestamp) -> StorageResult<u32> {
        Ok(count(exec(
            self.conn(),
            ITEMS,
            "DELETE FROM reading_items WHERE deleted_at IS NOT NULL AND deleted_at < ?1",
            [before.0],
        )?))
    }

    fn set_word_mark(
        &mut self,
        language: &str,
        lemma: &str,
        status: Option<&str>,
        now: Timestamp,
    ) -> StorageResult<()> {
        match status {
            Some(s) => exec(
                self.conn(),
                MARKS,
                "INSERT INTO word_marks (language, lemma, status, updated_at)
                 VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(language, lemma) DO UPDATE SET status = ?3, updated_at = ?4",
                params![language, lemma, s, now.0],
            )?,
            None => exec(
                self.conn(),
                MARKS,
                "DELETE FROM word_marks WHERE language = ?1 AND lemma = ?2",
                params![language, lemma],
            )?,
        };
        Ok(())
    }
}
