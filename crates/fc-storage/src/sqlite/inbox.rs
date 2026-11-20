//! `inbox` (v0.9).

use super::{all, exec, id, one, opt_id, ts, Mode, SqliteTx, WriteMode};
use crate::error::{StorageError, StorageResult};
use crate::repo::{InboxRead, InboxRecord, InboxWrite};
use fc_core::paging::{Page, PageRequest};
use fc_core::{ApiClientId, InboxItemId, Timestamp};
use rusqlite::params;

const T: &str = "inbox";
const COLS: &str =
    "id, source, payload, status, note_ids, client_id, client_ref, created_at, updated_at";

fn json_col<V: serde::de::DeserializeOwned>(
    r: &rusqlite::Row<'_>,
    i: usize,
) -> rusqlite::Result<V> {
    let s: String = r.get(i)?;
    serde_json::from_str(&s).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(i, rusqlite::types::Type::Text, Box::new(e))
    })
}

fn row(r: &rusqlite::Row<'_>) -> rusqlite::Result<InboxRecord> {
    Ok(InboxRecord {
        id: id(r, 0)?,
        source: json_col(r, 1)?,
        payload: json_col(r, 2)?,
        status: r.get(3)?,
        note_ids: json_col(r, 4)?,
        client_id: opt_id(r, 5)?,
        client_ref: r.get(6)?,
        created_at: ts(r, 7)?,
        updated_at: ts(r, 8)?,
    })
}

fn decode_cursor(c: &str) -> StorageResult<(i64, String)> {
    let bad = || StorageError::InvalidCursor(c.to_owned());
    let (t, i) = c.split_once(':').ok_or_else(bad)?;
    let t: i64 = t.parse().map_err(|_| bad())?;
    let i: InboxItemId = i.parse().map_err(|_| bad())?;
    Ok((t, i.to_string()))
}

impl<M: Mode> InboxRead for SqliteTx<'_, M> {
    fn list_inbox(&self, status: &str, page: &PageRequest) -> StorageResult<Page<InboxRecord>> {
        let limit = page.effective_limit();
        let cursor = page.cursor.as_deref().map(decode_cursor).transpose()?;
        let (c_time, c_id) = match &cursor {
            Some((t, i)) => (Some(*t), Some(i.as_str())),
            None => (None, None),
        };
        let mut items = all(
            self.conn(),
            T,
            &format!(
                "SELECT {COLS} FROM inbox WHERE status = ?1
                   AND (?2 IS NULL OR created_at < ?2 OR (created_at = ?2 AND id < ?3))
                 ORDER BY created_at DESC, id DESC LIMIT ?4"
            ),
            params![status, c_time, c_id, i64::from(limit) + 1],
            row,
        )?;
        let has_more = items.len() > limit as usize;
        items.truncate(limit as usize);
        let next_cursor = if has_more {
            items.last().map(|i| format!("{}:{}", i.created_at.0, i.id))
        } else {
            None
        };
        Ok(Page { items, next_cursor })
    }

    fn get_inbox_item(&self, item: InboxItemId) -> StorageResult<Option<InboxRecord>> {
        one(
            self.conn(),
            T,
            &format!("SELECT {COLS} FROM inbox WHERE id = ?1"),
            [item.to_string()],
            row,
        )
    }

    fn count_inbox(&self, status: &str) -> StorageResult<u32> {
        Ok(one(
            self.conn(),
            T,
            "SELECT count(*) FROM inbox WHERE status = ?1",
            [status],
            |r| r.get::<_, u32>(0),
        )?
        .unwrap_or(0))
    }

    fn inbox_item_by_client_ref(
        &self,
        client: ApiClientId,
        client_ref: &str,
    ) -> StorageResult<Option<InboxRecord>> {
        one(
            self.conn(),
            T,
            &format!("SELECT {COLS} FROM inbox WHERE client_id = ?1 AND client_ref = ?2"),
            params![client.to_string(), client_ref],
            row,
        )
    }

    fn inbox_client_usage(&self, client: ApiClientId, status: &str) -> StorageResult<(u32, u64)> {
        let usage = one(
            self.conn(),
            T,
            "SELECT count(*), COALESCE(SUM(length(CAST(payload AS BLOB))), 0)
             FROM inbox WHERE client_id = ?1 AND status = ?2",
            params![client.to_string(), status],
            |r| Ok((r.get::<_, u32>(0)?, r.get::<_, i64>(1)?)),
        )?
        .unwrap_or((0, 0));
        Ok((usage.0, u64::try_from(usage.1).unwrap_or(0)))
    }

    fn inbox_client_added_since(
        &self,
        client: ApiClientId,
        since: Timestamp,
    ) -> StorageResult<u32> {
        Ok(one(
            self.conn(),
            T,
            "SELECT count(*) FROM inbox WHERE client_id = ?1 AND created_at >= ?2",
            params![client.to_string(), since.0],
            |r| r.get::<_, u32>(0),
        )?
        .unwrap_or(0))
    }
}

impl InboxWrite for SqliteTx<'_, WriteMode> {
    fn upsert_inbox_item(&mut self, i: &InboxRecord) -> StorageResult<()> {
        let note_ids: Vec<String> = i.note_ids.iter().map(ToString::to_string).collect();
        exec(
            self.conn(),
            T,
            "INSERT INTO inbox (id, source, payload, status, note_ids, client_id, client_ref,
                 created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(id) DO UPDATE SET source = ?2, payload = ?3, status = ?4,
                 note_ids = ?5, client_id = ?6, client_ref = ?7, created_at = ?8,
                 updated_at = ?9",
            params![
                i.id.to_string(),
                i.source.to_string(),
                i.payload.to_string(),
                i.status,
                serde_json::Value::from(note_ids).to_string(),
                i.client_id.map(|c| c.to_string()),
                i.client_ref,
                i.created_at.0,
                i.updated_at.0,
            ],
        )
        .map_err(|e| match e {
            StorageError::Constraint { .. } => StorageError::Constraint {
                constraint: "inbox_client_ref".into(),
            },
            other => other,
        })?;
        Ok(())
    }

    fn delete_inbox_item(&mut self, item: InboxItemId) -> StorageResult<bool> {
        Ok(exec(
            self.conn(),
            T,
            "DELETE FROM inbox WHERE id = ?1",
            [item.to_string()],
        )? > 0)
    }
}
