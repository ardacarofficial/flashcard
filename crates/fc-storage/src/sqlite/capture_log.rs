//! `subtitle_capture_log` (v1.4 A5).

use super::{all, exec, opt_id, ts, Mode, SqliteTx, WriteMode};
use crate::error::StorageResult;
use crate::repo::{CaptureLogRead, CaptureLogRecord, CaptureLogWrite};
use rusqlite::params;

const TABLE: &str = "subtitle_capture_log";

impl<M: Mode> CaptureLogRead for SqliteTx<'_, M> {
    fn recent_capture_log(&self, limit: u32) -> StorageResult<Vec<CaptureLogRecord>> {
        all(
            self.conn(),
            TABLE,
            "SELECT seq, capture_id, at, result, reason, card_id, duration_ms
             FROM subtitle_capture_log ORDER BY seq DESC LIMIT ?1",
            [i64::from(limit)],
            |r| {
                Ok(CaptureLogRecord {
                    seq: r.get(0)?,
                    capture_id: r.get(1)?,
                    at: ts(r, 2)?,
                    result: r.get(3)?,
                    reason: r.get(4)?,
                    card_id: opt_id(r, 5)?,
                    duration_ms: r.get(6)?,
                })
            },
        )
    }
}

impl CaptureLogWrite for SqliteTx<'_, WriteMode> {
    fn append_capture_log(&mut self, rec: &CaptureLogRecord, keep: u32) -> StorageResult<()> {
        exec(
            self.conn(),
            TABLE,
            "INSERT INTO subtitle_capture_log
                 (capture_id, at, result, reason, card_id, duration_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                rec.capture_id,
                rec.at.0,
                rec.result,
                rec.reason,
                rec.card_id.map(|c| c.to_string()),
                rec.duration_ms
            ],
        )?;
        exec(
            self.conn(),
            TABLE,
            "DELETE FROM subtitle_capture_log
             WHERE seq <= (SELECT max(seq) FROM subtitle_capture_log) - ?1",
            [i64::from(keep)],
        )?;
        Ok(())
    }
}
