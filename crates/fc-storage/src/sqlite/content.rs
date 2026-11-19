//! Card content: media rows and files, note references, flag names.
//!
//! Media files live next to the database file at `media/<ab>/<sha256>` ([`media_file_path`]).
//! An in-memory database keeps them in a per-connection `temp.media_blob` table instead (all
//! transactions of an in-memory store share the writer connection).

use super::{all, exec, id, json_list, one, ts};
use super::{Mode, SqliteTx, WriteMode};
use crate::error::{StorageError, StorageResult};
use crate::media::media_file_path;
use crate::repo::{ContentRead, ContentWrite, FlagDef, MediaRecord, NoteRefs};
use fc_core::content::refs::is_media_hash;
use fc_core::{NoteId, NoteTypeId};
use rusqlite::{params, Connection, Row};
use std::path::{Path, PathBuf};

const M: &str = "media";
const MCOLS: &str = "hash, mime, size, original_name, ref_count, created_at";

fn db_file(conn: &Connection) -> Option<PathBuf> {
    conn.path().filter(|p| !p.is_empty()).map(PathBuf::from)
}

fn file_err(path: &Path, e: &std::io::Error) -> StorageError {
    StorageError::File {
        path: path.display().to_string(),
        message: e.to_string(),
    }
}

fn media_row(r: &Row<'_>) -> rusqlite::Result<MediaRecord> {
    Ok(MediaRecord {
        hash: r.get(0)?,
        mime: r.get(1)?,
        size: u64::try_from(r.get::<_, i64>(2)?).unwrap_or(0),
        original_name: r.get(3)?,
        ref_count: r.get(4)?,
        created_at: ts(r, 5)?,
    })
}

fn invalid_hash(hash: &str) -> StorageError {
    StorageError::NotFound {
        entity: "media",
        id: hash.to_owned(),
    }
}

fn ensure_blob_table(conn: &Connection) -> StorageResult<()> {
    conn.execute_batch(
        "CREATE TEMP TABLE IF NOT EXISTS media_blob (hash TEXT PRIMARY KEY, data BLOB NOT NULL)",
    )?;
    Ok(())
}

impl<M: Mode> ContentRead for SqliteTx<'_, M> {
    fn get_media(&self, hash: &str) -> StorageResult<Option<MediaRecord>> {
        one(
            self.conn(),
            M,
            &format!("SELECT {MCOLS} FROM media WHERE hash = ?1"),
            [hash],
            media_row,
        )
    }

    fn list_media(&self) -> StorageResult<Vec<MediaRecord>> {
        all(
            self.conn(),
            M,
            &format!("SELECT {MCOLS} FROM media ORDER BY created_at DESC, hash"),
            [],
            media_row,
        )
    }

    fn read_media_file(&self, hash: &str) -> StorageResult<Option<Vec<u8>>> {
        if !is_media_hash(hash) {
            return Err(invalid_hash(hash));
        }
        match db_file(self.conn()) {
            Some(db) => {
                let Some(path) = media_file_path(&db, hash) else {
                    return Err(invalid_hash(hash));
                };
                match std::fs::read(&path) {
                    Ok(b) => Ok(Some(b)),
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
                    Err(e) => Err(file_err(&path, &e)),
                }
            }
            None => {
                let exists: bool = self
                    .conn()
                    .query_row(
                        "SELECT count(*) FROM temp.sqlite_master WHERE name = 'media_blob'",
                        [],
                        |r| r.get::<_, i64>(0),
                    )
                    .map(|n| n > 0)?;
                if !exists {
                    return Ok(None);
                }
                one(
                    self.conn(),
                    M,
                    "SELECT data FROM temp.media_blob WHERE hash = ?1",
                    [hash],
                    |r| r.get(0),
                )
            }
        }
    }

    fn note_media(&self, note: NoteId) -> StorageResult<Vec<String>> {
        all(
            self.conn(),
            "note_media",
            "SELECT hash FROM note_media WHERE note_id = ?1 ORDER BY hash",
            [note.to_string()],
            |r| r.get(0),
        )
    }

    fn notes_by_link_key(&self, keys: &[String]) -> StorageResult<Vec<(String, NoteId)>> {
        if keys.is_empty() {
            return Ok(Vec::new());
        }
        all(
            self.conn(),
            "note_keys",
            "SELECT k.key, k.note_id FROM note_keys k JOIN notes n ON n.id = k.note_id
             WHERE k.key IN (SELECT value FROM json_each(?1)) AND n.deleted_at IS NULL
             ORDER BY k.key, n.created_at, n.id",
            [json_list(keys)],
            |r| Ok((r.get(0)?, id(r, 1)?)),
        )
    }

    fn notes_linking_to(&self, key: &str) -> StorageResult<Vec<NoteId>> {
        all(
            self.conn(),
            "note_link_refs",
            "SELECT l.from_note FROM note_link_refs l JOIN notes n ON n.id = l.from_note
             WHERE l.target_key = ?1 AND n.deleted_at IS NULL ORDER BY l.from_note",
            [key],
            |r| id(r, 0),
        )
    }

    fn notes_with_key_prefix(
        &self,
        prefix: &str,
        limit: u32,
    ) -> StorageResult<Vec<(NoteId, String)>> {
        // `substr` comparison (not LIKE): keys are Unicode-lowercased in Rust, LIKE is ASCII.
        all(
            self.conn(),
            "note_keys",
            "SELECT n.id, n.sort_field FROM note_keys k JOIN notes n ON n.id = k.note_id
             WHERE substr(k.key, 1, length(?1)) = ?1 AND n.deleted_at IS NULL
             ORDER BY k.key, n.id LIMIT ?2",
            params![prefix, limit],
            |r| Ok((id(r, 0)?, r.get(1)?)),
        )
    }

    fn note_ids_of_notetype(
        &self,
        notetype: NoteTypeId,
        include_deleted: bool,
    ) -> StorageResult<Vec<NoteId>> {
        all(
            self.conn(),
            "notes",
            "SELECT id FROM notes WHERE notetype_id = ?1 AND (?2 OR deleted_at IS NULL)
             ORDER BY id",
            params![notetype.to_string(), include_deleted],
            |r| id(r, 0),
        )
    }

    fn list_flag_defs(&self) -> StorageResult<Vec<FlagDef>> {
        all(
            self.conn(),
            "card_flag_defs",
            "SELECT flag, name, color, updated_at FROM card_flag_defs ORDER BY flag",
            [],
            |r| {
                Ok(FlagDef {
                    flag: r.get(0)?,
                    name: r.get(1)?,
                    color: r.get(2)?,
                    updated_at: ts(r, 3)?,
                })
            },
        )
    }
}

impl ContentWrite for SqliteTx<'_, WriteMode> {
    fn insert_media(&mut self, m: &MediaRecord) -> StorageResult<()> {
        exec(
            self.conn(),
            M,
            "INSERT INTO media (hash, mime, size, original_name, ref_count, created_at)
             VALUES (?1, ?2, ?3, ?4, 0, ?5) ON CONFLICT (hash) DO NOTHING",
            params![
                m.hash,
                m.mime,
                i64::try_from(m.size).unwrap_or(i64::MAX),
                m.original_name,
                m.created_at.0
            ],
        )?;
        Ok(())
    }

    fn write_media_file(&mut self, hash: &str, bytes: &[u8]) -> StorageResult<()> {
        if !is_media_hash(hash) {
            return Err(invalid_hash(hash));
        }
        match db_file(self.conn()) {
            Some(db) => {
                let Some(path) = media_file_path(&db, hash) else {
                    return Err(invalid_hash(hash));
                };
                if path.exists() {
                    return Ok(());
                }
                if let Some(dir) = path.parent() {
                    std::fs::create_dir_all(dir).map_err(|e| file_err(dir, &e))?;
                }
                // Write then rename: a crash never leaves a truncated file under the hash.
                let part = path.with_extension("part");
                std::fs::write(&part, bytes).map_err(|e| file_err(&part, &e))?;
                std::fs::rename(&part, &path).map_err(|e| file_err(&path, &e))
            }
            None => {
                ensure_blob_table(self.conn())?;
                exec(
                    self.conn(),
                    M,
                    "INSERT INTO temp.media_blob (hash, data) VALUES (?1, ?2)
                     ON CONFLICT (hash) DO NOTHING",
                    params![hash, bytes],
                )?;
                Ok(())
            }
        }
    }

    fn delete_unreferenced_media(&mut self, hash: &str) -> StorageResult<bool> {
        let n = exec(
            self.conn(),
            M,
            "DELETE FROM media WHERE hash = ?1
               AND NOT EXISTS (SELECT 1 FROM note_media WHERE hash = ?1)",
            [hash],
        )?;
        Ok(n > 0)
    }

    fn set_note_refs(&mut self, note: NoteId, refs: &NoteRefs) -> StorageResult<()> {
        let conn = self.conn();
        let nid = note.to_string();
        let mut affected = all(
            conn,
            "note_media",
            "SELECT hash FROM note_media WHERE note_id = ?1",
            [&nid],
            |r| r.get::<_, String>(0),
        )?;
        exec(
            conn,
            "note_media",
            "DELETE FROM note_media WHERE note_id = ?1",
            [&nid],
        )?;
        for h in &refs.media {
            exec(
                conn,
                "note_media",
                "INSERT INTO note_media (note_id, hash) VALUES (?1, ?2) ON CONFLICT DO NOTHING",
                params![nid, h],
            )?;
            affected.push(h.clone());
        }
        affected.sort();
        affected.dedup();
        exec(
            conn,
            M,
            "UPDATE media SET ref_count = (SELECT count(*) FROM note_media WHERE hash = media.hash)
             WHERE hash IN (SELECT value FROM json_each(?1))",
            [json_list(&affected)],
        )?;
        exec(
            conn,
            "note_link_refs",
            "DELETE FROM note_link_refs WHERE from_note = ?1",
            [&nid],
        )?;
        for (key, label) in &refs.links {
            exec(
                conn,
                "note_link_refs",
                "INSERT INTO note_link_refs (from_note, target_key, label) VALUES (?1, ?2, ?3)
                 ON CONFLICT DO NOTHING",
                params![nid, key, label],
            )?;
        }
        exec(
            conn,
            "note_keys",
            "INSERT INTO note_keys (note_id, key) VALUES (?1, ?2)
             ON CONFLICT (note_id) DO UPDATE SET key = excluded.key",
            params![nid, refs.key],
        )?;
        Ok(())
    }

    fn set_flag_def(&mut self, def: &FlagDef) -> StorageResult<()> {
        super::exec_one(
            self.conn(),
            "card_flag_defs",
            "flag",
            &def.flag,
            "UPDATE card_flag_defs SET name = ?2, color = ?3, updated_at = ?4 WHERE flag = ?1",
            params![def.flag, def.name, def.color, def.updated_at.0],
        )
    }
}
