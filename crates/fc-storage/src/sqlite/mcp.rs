//! `mcp_servers` (v0.8).

use super::{all, exec, id, one, ts, Mode, SqliteTx, WriteMode};
use crate::error::StorageResult;
use crate::repo::{McpRead, McpServerRecord, McpWrite};
use fc_core::McpServerId;
use rusqlite::params;

const COLS: &str =
    "id, name, transport, command, args, url, enabled, created_at, updated_at, approved";

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<McpServerRecord> {
    let args: String = r.get(4)?;
    Ok(McpServerRecord {
        id: id(r, 0)?,
        name: r.get(1)?,
        transport: r.get(2)?,
        command: r.get(3)?,
        args: serde_json::from_str(&args).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(4, rusqlite::types::Type::Text, Box::new(e))
        })?,
        url: r.get(5)?,
        enabled: r.get::<_, i64>(6)? != 0,
        created_at: ts(r, 7)?,
        updated_at: ts(r, 8)?,
        approved: r.get(9)?,
    })
}

impl<M: Mode> McpRead for SqliteTx<'_, M> {
    fn list_mcp_servers(&self) -> StorageResult<Vec<McpServerRecord>> {
        all(
            self.conn(),
            "mcp_servers",
            &format!("SELECT {COLS} FROM mcp_servers ORDER BY lower(name), id"),
            [],
            row,
        )
    }

    fn get_mcp_server(&self, server: McpServerId) -> StorageResult<Option<McpServerRecord>> {
        one(
            self.conn(),
            "mcp_servers",
            &format!("SELECT {COLS} FROM mcp_servers WHERE id = ?1"),
            [server.to_string()],
            row,
        )
    }
}

impl McpWrite for SqliteTx<'_, WriteMode> {
    fn upsert_mcp_server(&mut self, s: &McpServerRecord) -> StorageResult<()> {
        let args = serde_json::Value::from(s.args.clone()).to_string();
        exec(
            self.conn(),
            "mcp_servers",
            "INSERT INTO mcp_servers (id, name, transport, command, args, url, enabled,
                 created_at, updated_at, approved)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(id) DO UPDATE SET name = ?2, transport = ?3, command = ?4, args = ?5,
                 url = ?6, enabled = ?7, created_at = ?8, updated_at = ?9,
                 approved = ?10",
            params![
                s.id.to_string(),
                s.name,
                s.transport,
                s.command,
                args,
                s.url,
                i64::from(s.enabled),
                s.created_at.0,
                s.updated_at.0,
                s.approved
            ],
        )?;
        Ok(())
    }

    fn delete_mcp_server(&mut self, server: McpServerId) -> StorageResult<bool> {
        Ok(exec(
            self.conn(),
            "mcp_servers",
            "DELETE FROM mcp_servers WHERE id = ?1",
            [server.to_string()],
        )? > 0)
    }
}
