//! `ux_state` (v1.0).

use super::{exec, one, Mode, SqliteTx, WriteMode};
use crate::error::StorageResult;
use crate::repo::{UxStateRead, UxStateWrite};
use fc_core::Timestamp;
use rusqlite::params;
use serde_json::Value;

impl<M: Mode> UxStateRead for SqliteTx<'_, M> {
    fn ux_state(&self, key: &str) -> StorageResult<Option<Value>> {
        one(
            self.conn(),
            "ux_state",
            "SELECT value FROM ux_state WHERE key = ?1",
            [key],
            |r| super::json(r, 0),
        )
    }
}

impl UxStateWrite for SqliteTx<'_, WriteMode> {
    fn set_ux_state(&mut self, key: &str, value: &Value, now: Timestamp) -> StorageResult<()> {
        exec(
            self.conn(),
            "ux_state",
            "INSERT INTO ux_state (key, value, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = ?2, updated_at = ?3",
            params![key, value.to_string(), now.0],
        )?;
        Ok(())
    }
}
