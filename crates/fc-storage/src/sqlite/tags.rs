//! Tags. Names are unique case-insensitively: the `tags_name` index is ASCII `NOCASE`, and
//! lookups additionally compare Unicode-lowercase keys ([`TagName::key`]) so `Çay`/`çay` are
//! one tag.

use super::{all, exec, exec_one, id, one, ts};
use super::{Mode, SqliteTx, WriteMode};
use crate::error::{StorageError, StorageResult};
use crate::repo::{EnsuredTag, TagRead, TagWrite};
use fc_core::tag::{Tag, TagName};
use fc_core::{NoteId, TagId, Timestamp};
use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ValueRef};
use rusqlite::{params, Connection, Row};

const T: &str = "tags";
const COLS: &str = "id, name, color, created_at, updated_at";

/// Column holding a stored (already normalized) tag name.
pub(super) struct TagCol(pub TagName);

impl FromSql for TagCol {
    fn column_result(v: ValueRef<'_>) -> FromSqlResult<Self> {
        TagName::parse(v.as_str()?)
            .map(TagCol)
            .map_err(|e| FromSqlError::Other(e.to_string().into()))
    }
}

fn tag_row(r: &Row<'_>) -> rusqlite::Result<Tag> {
    Ok(Tag {
        id: id(r, 0)?,
        name: r.get::<_, TagCol>(1)?.0,
        color: r.get(2)?,
        created_at: ts(r, 3)?,
        updated_at: ts(r, 4)?,
    })
}

/// Tag by name, Unicode case-insensitive.
pub(super) fn find_tag(conn: &Connection, name: &TagName) -> StorageResult<Option<Tag>> {
    let key = name.key();
    // Fast path: ASCII-case-insensitive index lookup.
    let hits = all(
        conn,
        T,
        &format!("SELECT {COLS} FROM tags WHERE name = ?1 COLLATE NOCASE"),
        [name.as_str()],
        tag_row,
    )?;
    if let Some(t) = hits.into_iter().find(|t| t.name.key() == key) {
        return Ok(Some(t));
    }
    // Names differing only in non-ASCII case (`Çay`/`çay`) miss NOCASE: compare keys over
    // the (few) names that contain non-ASCII characters.
    if name.as_str().is_ascii() {
        return Ok(None);
    }
    let candidates = all(
        conn,
        T,
        &format!("SELECT {COLS} FROM tags WHERE name GLOB '*[^ -~]*'"),
        [],
        tag_row,
    )?;
    Ok(candidates.into_iter().find(|t| t.name.key() == key))
}

impl<M: Mode> TagRead for SqliteTx<'_, M> {
    fn list_tags(&self) -> StorageResult<Vec<(Tag, u32)>> {
        all(
            self.conn(),
            T,
            &format!(
                "SELECT {COLS},
                     (SELECT count(*) FROM note_tags nt JOIN notes n ON n.id = nt.note_id
                      WHERE nt.tag_id = tags.id AND n.deleted_at IS NULL)
                 FROM tags ORDER BY name COLLATE NOCASE, name, id"
            ),
            [],
            |r| Ok((tag_row(r)?, r.get(5)?)),
        )
    }

    fn list_tags_uncounted(&self) -> StorageResult<Vec<Tag>> {
        all(
            self.conn(),
            T,
            &format!("SELECT {COLS} FROM tags ORDER BY name COLLATE NOCASE, name, id"),
            [],
            tag_row,
        )
    }

    fn get_tag_by_name(&self, name: &TagName) -> StorageResult<Option<Tag>> {
        find_tag(self.conn(), name)
    }
}

impl TagWrite for SqliteTx<'_, WriteMode> {
    fn upsert_tag(&mut self, t: &Tag) -> StorageResult<()> {
        exec(
            self.conn(),
            T,
            &format!(
                "INSERT INTO tags ({COLS}) VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(id) DO UPDATE SET name = excluded.name, color = excluded.color,
                     created_at = excluded.created_at, updated_at = excluded.updated_at"
            ),
            params![
                t.id.to_string(),
                t.name.as_str(),
                t.color,
                t.created_at.0,
                t.updated_at.0
            ],
        )?;
        Ok(())
    }

    fn ensure_tags(
        &mut self,
        tags: &[(TagName, TagId)],
        now: Timestamp,
    ) -> StorageResult<Vec<EnsuredTag>> {
        let conn = self.conn();
        let mut out: Vec<EnsuredTag> = Vec::with_capacity(tags.len());
        for (name, new_id) in tags {
            if let Some(t) = find_tag(conn, name)? {
                // A name repeated in the input finds the row inserted for its first occurrence.
                let created = out.iter().any(|e| e.id == t.id && e.created);
                out.push(EnsuredTag { id: t.id, created });
                continue;
            }
            exec(
                conn,
                T,
                &format!("INSERT INTO tags ({COLS}) VALUES (?1, ?2, NULL, ?3, ?3)"),
                params![new_id.to_string(), name.as_str(), now.0],
            )?;
            out.push(EnsuredTag {
                id: *new_id,
                created: true,
            });
        }
        Ok(out)
    }

    fn delete_unused_tags(&mut self) -> StorageResult<u32> {
        let n = exec(
            self.conn(),
            T,
            "DELETE FROM tags WHERE NOT EXISTS (SELECT 1 FROM note_tags WHERE tag_id = tags.id)",
            [],
        )?;
        Ok(super::count(n))
    }

    fn merge_tag(
        &mut self,
        from: TagId,
        into: TagId,
        now: Timestamp,
    ) -> StorageResult<Vec<NoteId>> {
        let conn = self.conn();
        for t in [from, into] {
            if one(
                conn,
                T,
                "SELECT 1 FROM tags WHERE id = ?1",
                [t.to_string()],
                |r| r.get::<_, i64>(0),
            )?
            .is_none()
            {
                return Err(StorageError::NotFound {
                    entity: "tag",
                    id: t.to_string(),
                });
            }
        }
        if from == into {
            return Ok(Vec::new());
        }
        let (f, i) = (from.to_string(), into.to_string());
        let affected: Vec<NoteId> = all(
            conn,
            "note_tags",
            "SELECT note_id FROM note_tags WHERE tag_id = ?1 ORDER BY note_id",
            [&f],
            |r| id(r, 0),
        )?;
        exec(
            conn,
            "notes",
            "UPDATE notes SET updated_at = ?2
             WHERE id IN (SELECT note_id FROM note_tags WHERE tag_id = ?1)",
            params![f, now.0],
        )?;
        exec(
            conn,
            "note_tags",
            "DELETE FROM note_tags WHERE tag_id = ?1
               AND note_id IN (SELECT note_id FROM note_tags WHERE tag_id = ?2)",
            [&f, &i],
        )?;
        exec(
            conn,
            "note_tags",
            "UPDATE note_tags SET tag_id = ?2 WHERE tag_id = ?1",
            [&f, &i],
        )?;
        exec_one(
            conn,
            T,
            "tag",
            &from,
            "DELETE FROM tags WHERE id = ?1",
            [&f],
        )?;
        Ok(affected)
    }
}
