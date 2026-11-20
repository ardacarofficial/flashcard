//! `api_clients` (v0.9).

use super::{all, exec, id, one, opt_ms, opt_ts, ts, Mode, SqliteTx, WriteMode};
use crate::error::{StorageError, StorageResult};
use crate::repo::{ApiClientRecord, LocalApiRead, LocalApiWrite};
use fc_core::ApiClientId;
use rusqlite::params;

const T: &str = "api_clients";
const COLS: &str = "id, name, kind, origin, requested_scopes, scopes, status, token_hash,
    created_at, updated_at, last_used_at";

fn list_col(r: &rusqlite::Row<'_>, i: usize) -> rusqlite::Result<Vec<String>> {
    let s: String = r.get(i)?;
    serde_json::from_str(&s).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(i, rusqlite::types::Type::Text, Box::new(e))
    })
}

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<ApiClientRecord> {
    Ok(ApiClientRecord {
        id: id(r, 0)?,
        name: r.get(1)?,
        kind: r.get(2)?,
        origin: r.get(3)?,
        requested_scopes: list_col(r, 4)?,
        scopes: list_col(r, 5)?,
        status: r.get(6)?,
        token_hash: r.get(7)?,
        created_at: ts(r, 8)?,
        updated_at: ts(r, 9)?,
        last_used_at: opt_ts(r, 10)?,
    })
}

impl<M: Mode> LocalApiRead for SqliteTx<'_, M> {
    fn list_api_clients(&self) -> StorageResult<Vec<ApiClientRecord>> {
        all(
            self.conn(),
            T,
            &format!("SELECT {COLS} FROM api_clients ORDER BY created_at DESC, id DESC"),
            [],
            row,
        )
    }

    fn get_api_client(&self, client: ApiClientId) -> StorageResult<Option<ApiClientRecord>> {
        one(
            self.conn(),
            T,
            &format!("SELECT {COLS} FROM api_clients WHERE id = ?1"),
            [client.to_string()],
            row,
        )
    }

    fn api_client_by_token_hash(&self, token_hash: &str) -> StorageResult<Option<ApiClientRecord>> {
        one(
            self.conn(),
            T,
            &format!("SELECT {COLS} FROM api_clients WHERE token_hash = ?1"),
            [token_hash],
            row,
        )
    }
}

impl LocalApiWrite for SqliteTx<'_, WriteMode> {
    fn upsert_api_client(&mut self, c: &ApiClientRecord) -> StorageResult<()> {
        exec(
            self.conn(),
            T,
            "INSERT INTO api_clients (id, name, kind, origin, requested_scopes, scopes, status,
                 token_hash, created_at, updated_at, last_used_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(id) DO UPDATE SET name = ?2, kind = ?3, origin = ?4,
                 requested_scopes = ?5, scopes = ?6, status = ?7, token_hash = ?8,
                 created_at = ?9, updated_at = ?10, last_used_at = ?11",
            params![
                c.id.to_string(),
                c.name,
                c.kind,
                c.origin,
                serde_json::Value::from(c.requested_scopes.clone()).to_string(),
                serde_json::Value::from(c.scopes.clone()).to_string(),
                c.status,
                c.token_hash,
                c.created_at.0,
                c.updated_at.0,
                opt_ms(c.last_used_at),
            ],
        )
        .map_err(|e| match e {
            StorageError::Constraint { .. } => StorageError::Constraint {
                constraint: "api_clients_token".into(),
            },
            other => other,
        })?;
        Ok(())
    }

    fn delete_api_client(&mut self, client: ApiClientId) -> StorageResult<bool> {
        Ok(exec(
            self.conn(),
            T,
            "DELETE FROM api_clients WHERE id = ?1",
            [client.to_string()],
        )? > 0)
    }
}
