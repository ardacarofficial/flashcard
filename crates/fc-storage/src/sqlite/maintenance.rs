//! `drafts` and the integrity check (v1.0).

use super::{all, exec, one, ts, Mode, SqliteTx, WriteMode};
use crate::error::StorageResult;
use crate::repo::{DraftRecord, IntegrityRecord, MaintenanceRead, MaintenanceWrite};
use rusqlite::params;

const COLS: &str = "key, payload, created_at, updated_at";

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<DraftRecord> {
    let payload: String = r.get(1)?;
    Ok(DraftRecord {
        key: r.get(0)?,
        payload: serde_json::from_str(&payload).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, Box::new(e))
        })?,
        created_at: ts(r, 2)?,
        updated_at: ts(r, 3)?,
    })
}

impl<M: Mode> MaintenanceRead for SqliteTx<'_, M> {
    fn list_drafts(&self) -> StorageResult<Vec<DraftRecord>> {
        all(
            self.conn(),
            "drafts",
            &format!("SELECT {COLS} FROM drafts ORDER BY updated_at DESC, key"),
            [],
            row,
        )
    }

    fn get_draft(&self, key: &str) -> StorageResult<Option<DraftRecord>> {
        one(
            self.conn(),
            "drafts",
            &format!("SELECT {COLS} FROM drafts WHERE key = ?1"),
            [key],
            row,
        )
    }

    fn integrity_check(&self, quick: bool, max_rows: u32) -> StorageResult<IntegrityRecord> {
        let pragma = if quick {
            "quick_check"
        } else {
            "integrity_check"
        };
        let rows = all(
            self.conn(),
            "sqlite_master",
            &format!("PRAGMA {pragma}({})", max_rows.max(1)),
            [],
            |r| r.get::<_, String>(0),
        )?;
        let fk = all(
            self.conn(),
            "sqlite_master",
            "PRAGMA foreign_key_check",
            [],
            |r| r.get::<_, Option<String>>(0),
        )?;
        Ok(IntegrityRecord {
            rows,
            foreign_key_violations: u64::try_from(fk.len()).unwrap_or(u64::MAX),
        })
    }
}

impl MaintenanceWrite for SqliteTx<'_, WriteMode> {
    fn upsert_draft(&mut self, d: &DraftRecord) -> StorageResult<()> {
        exec(
            self.conn(),
            "drafts",
            "INSERT INTO drafts (key, payload, created_at, updated_at) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(key) DO UPDATE SET payload = excluded.payload,
                 updated_at = excluded.updated_at",
            params![d.key, d.payload.to_string(), d.created_at.0, d.updated_at.0],
        )?;
        Ok(())
    }

    fn delete_draft(&mut self, key: &str) -> StorageResult<bool> {
        let n = exec(
            self.conn(),
            "drafts",
            "DELETE FROM drafts WHERE key = ?1",
            [key],
        )?;
        Ok(n > 0)
    }
}
