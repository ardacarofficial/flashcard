//! `leech_repairs` (v1.4 W3).

use super::{all, exec, id, json_list, one, ts, Mode, SqliteTx, WriteMode};
use crate::error::StorageResult;
use crate::repo::{LeechRepairRead, LeechRepairRecord, LeechRepairWrite};
use fc_core::CardId;
use rusqlite::{params, Row};

const TABLE: &str = "leech_repairs";

const COLS: &str = "id, card_id, note_id, kind, detail, lapses, applied_at";

fn row(r: &Row<'_>) -> rusqlite::Result<LeechRepairRecord> {
    Ok(LeechRepairRecord {
        id: r.get(0)?,
        card_id: id(r, 1)?,
        note_id: id(r, 2)?,
        kind: r.get(3)?,
        detail: r.get(4)?,
        lapses: r.get(5)?,
        applied_at: ts(r, 6)?,
    })
}

impl<M: Mode> LeechRepairRead for SqliteTx<'_, M> {
    fn list_leech_repairs(
        &self,
        card_ids: Option<&[CardId]>,
    ) -> StorageResult<Vec<LeechRepairRecord>> {
        match card_ids {
            None => all(
                self.conn(),
                TABLE,
                &format!("SELECT {COLS} FROM leech_repairs ORDER BY applied_at, id"),
                [],
                row,
            ),
            Some([]) => Ok(Vec::new()),
            Some(ids) => all(
                self.conn(),
                TABLE,
                &format!(
                    "SELECT {COLS} FROM leech_repairs
                     WHERE card_id IN (SELECT value FROM json_each(?1))
                     ORDER BY applied_at, id"
                ),
                [json_list(ids)],
                row,
            ),
        }
    }

    fn get_leech_repair(&self, id: i64) -> StorageResult<Option<LeechRepairRecord>> {
        one(
            self.conn(),
            TABLE,
            &format!("SELECT {COLS} FROM leech_repairs WHERE id = ?1"),
            [id],
            row,
        )
    }
}

impl LeechRepairWrite for SqliteTx<'_, WriteMode> {
    fn insert_leech_repair(&mut self, rec: &LeechRepairRecord) -> StorageResult<i64> {
        exec(
            self.conn(),
            TABLE,
            "INSERT INTO leech_repairs (id, card_id, note_id, kind, detail, lapses, applied_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                (rec.id > 0).then_some(rec.id),
                rec.card_id.to_string(),
                rec.note_id.to_string(),
                rec.kind,
                rec.detail,
                rec.lapses,
                rec.applied_at.0
            ],
        )?;
        Ok(self.conn().last_insert_rowid())
    }

    fn delete_leech_repair(&mut self, id: i64) -> StorageResult<()> {
        exec(
            self.conn(),
            TABLE,
            "DELETE FROM leech_repairs WHERE id = ?1",
            [id],
        )?;
        Ok(())
    }
}
