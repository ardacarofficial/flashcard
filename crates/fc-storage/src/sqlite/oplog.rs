//! Oplog (append-only). Order is insertion order (`rowid`): one command = one write
//! transaction = one contiguous batch, because there is a single writer.

use super::{all, exec, id, json, one, opt_id, to_json, ts};
use super::{Mode, SqliteTx, WriteMode};
use crate::error::{StorageError, StorageResult};
use crate::repo::{OplogBatch, OplogRead, OplogRecord, OplogRecordKind, OplogWrite};
use fc_core::paging::{Page, PageRequest};
use fc_core::BatchId;
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ValueRef};
use rusqlite::{params, Row};

const T: &str = "oplog";
const COLS: &str = "id, batch_id, seq, command, payload, inverse, actor, kind, \
                    reverts_batch_id, summary, created_at";

fn kind_str(k: OplogRecordKind) -> &'static str {
    match k {
        OplogRecordKind::Do => "do",
        OplogRecordKind::Undo => "undo",
        OplogRecordKind::Redo => "redo",
    }
}

struct KindCol(OplogRecordKind);

impl FromSql for KindCol {
    fn column_result(v: ValueRef<'_>) -> FromSqlResult<Self> {
        Ok(KindCol(match v.as_str()? {
            "do" => OplogRecordKind::Do,
            "undo" => OplogRecordKind::Undo,
            "redo" => OplogRecordKind::Redo,
            other => {
                return Err(FromSqlError::Other(
                    format!("unknown oplog kind {other}").into(),
                ))
            }
        }))
    }
}

fn record_row(r: &Row<'_>) -> rusqlite::Result<OplogRecord> {
    Ok(OplogRecord {
        id: id(r, 0)?,
        batch_id: id(r, 1)?,
        seq: r.get(2)?,
        command: r.get(3)?,
        payload: json(r, 4)?,
        inverse: json(r, 5)?,
        actor: json(r, 6)?,
        kind: r.get::<_, KindCol>(7)?.0,
        reverts_batch_id: opt_id(r, 8)?,
        summary: json(r, 9)?,
        created_at: ts(r, 10)?,
    })
}

impl<M: Mode> OplogRead for SqliteTx<'_, M> {
    fn list_oplog(&self, page: &PageRequest) -> StorageResult<Page<OplogRecord>> {
        let limit = page.effective_limit();
        let before: Option<i64> = page
            .cursor
            .as_deref()
            .map(|c| {
                c.parse::<i64>()
                    .map_err(|_| StorageError::InvalidCursor(c.to_owned()))
            })
            .transpose()?;
        let rows = all(
            self.conn(),
            T,
            &format!(
                "SELECT {COLS}, rowid FROM oplog WHERE ?1 IS NULL OR rowid < ?1
                 ORDER BY rowid DESC LIMIT ?2"
            ),
            params![before, i64::from(limit) + 1],
            |r| Ok((record_row(r)?, r.get::<_, i64>(11)?)),
        )?;
        let has_more = rows.len() > limit as usize;
        let rows: Vec<_> = rows.into_iter().take(limit as usize).collect();
        let next_cursor = if has_more {
            rows.last().map(|(_, rowid)| rowid.to_string())
        } else {
            None
        };
        Ok(Page {
            items: rows.into_iter().map(|(rec, _)| rec).collect(),
            next_cursor,
        })
    }

    fn list_oplog_batches(
        &self,
        before: Option<BatchId>,
        limit: u32,
    ) -> StorageResult<Vec<OplogBatch>> {
        let bound: Option<i64> = match before {
            Some(b) => {
                let first = one(
                    self.conn(),
                    T,
                    "SELECT min(rowid) FROM oplog WHERE batch_id = ?1",
                    [b.to_string()],
                    |r| r.get::<_, Option<i64>>(0),
                )?
                .flatten();
                match first {
                    Some(r) => Some(r),
                    None => {
                        return Err(StorageError::NotFound {
                            entity: "oplog_batch",
                            id: b.to_string(),
                        })
                    }
                }
            }
            None => None,
        };
        let mut batches: Vec<BatchId> = Vec::new();
        let mut cursor = bound;
        // Walk entries backwards by rowid; batches are contiguous.
        'outer: while batches.len() < limit as usize {
            let chunk: Vec<(BatchId, i64)> = all(
                self.conn(),
                T,
                "SELECT batch_id, rowid FROM oplog WHERE ?1 IS NULL OR rowid < ?1
                 ORDER BY rowid DESC LIMIT 256",
                [cursor],
                |r| Ok((id(r, 0)?, r.get(1)?)),
            )?;
            if chunk.is_empty() {
                break;
            }
            for (b, rowid) in chunk {
                cursor = Some(rowid);
                if batches.last() != Some(&b) && !batches.contains(&b) {
                    if batches.len() == limit as usize {
                        break 'outer;
                    }
                    batches.push(b);
                }
            }
        }
        batches
            .into_iter()
            .map(|b| {
                one(
                    self.conn(),
                    T,
                    "SELECT batch_id, kind, reverts_batch_id, summary, created_at FROM oplog
                     WHERE batch_id = ?1 ORDER BY seq LIMIT 1",
                    [b.to_string()],
                    |r| {
                        Ok(OplogBatch {
                            batch_id: id(r, 0)?,
                            kind: r.get::<_, KindCol>(1)?.0,
                            reverts_batch_id: opt_id(r, 2)?,
                            summary: json(r, 3)?,
                            created_at: ts(r, 4)?,
                        })
                    },
                )?
                .ok_or_else(|| StorageError::NotFound {
                    entity: "oplog_batch",
                    id: b.to_string(),
                })
            })
            .collect()
    }

    fn oplog_watermark(&self) -> StorageResult<i64> {
        Ok(
            one(self.conn(), T, "SELECT max(rowid) FROM oplog", [], |r| {
                r.get::<_, Option<i64>>(0)
            })?
            .flatten()
            .unwrap_or(0),
        )
    }

    fn oplog_actors_after(&self, after: i64) -> StorageResult<Vec<serde_json::Value>> {
        all(
            self.conn(),
            T,
            "SELECT DISTINCT actor FROM oplog WHERE rowid > ?1",
            [after],
            |r| json(r, 0),
        )
    }

    fn oplog_batch_entries(&self, batch: BatchId) -> StorageResult<Vec<OplogRecord>> {
        all(
            self.conn(),
            T,
            &format!("SELECT {COLS} FROM oplog WHERE batch_id = ?1 ORDER BY seq"),
            [batch.to_string()],
            record_row,
        )
    }
}

impl OplogWrite for SqliteTx<'_, WriteMode> {
    fn append_oplog(&mut self, rec: &OplogRecord) -> StorageResult<()> {
        exec(
            self.conn(),
            T,
            &format!(
                "INSERT INTO oplog ({COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)"
            ),
            params![
                rec.id.to_string(),
                rec.batch_id.to_string(),
                rec.seq,
                rec.command,
                to_json(T, &rec.payload)?,
                to_json(T, &rec.inverse)?,
                to_json(T, &rec.actor)?,
                kind_str(rec.kind),
                rec.reverts_batch_id.map(|b| b.to_string()),
                to_json(T, &rec.summary)?,
                rec.created_at.0,
            ],
        )?;
        Ok(())
    }
}
