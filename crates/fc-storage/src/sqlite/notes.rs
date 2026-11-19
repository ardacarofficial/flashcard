//! Notes, their tags (`note_tags`) and their full-text rows (`notes_fts`).
//!
//! FTS is maintained explicitly (no triggers): the app passes the searchable text, and
//! `notes.fts_rowid` points at the note's `notes_fts` row. Trashing a note
//! keeps its FTS row (searches join `notes` and filter `deleted_at`), so restore needs no text.

use super::decks::set_deleted;
use super::tags::{find_tag, TagCol};
use super::{all, exec, exec_one, id, json, json_list, one, opt_ms, opt_ts, to_json, ts};
use super::{Mode, SqliteTx, WriteMode};
use crate::error::{StorageError, StorageResult};
use crate::repo::{NoteFilter, NoteListRow, NoteRead, NoteWrite};
use fc_core::note::Note;
use fc_core::paging::{Page, PageRequest};
use fc_core::tag::TagName;
use fc_core::{DeckId, NoteId, NoteTypeId, TagId, Timestamp};
use rusqlite::{params, Connection, Row};
use std::collections::HashMap;

const T: &str = "notes";
const COLS: &str = "id, notetype_id, fields, sort_field, checksum, created_at, updated_at, \
                    deleted_at, home_deck_id";

fn note_row(r: &Row<'_>) -> rusqlite::Result<Note> {
    Ok(Note {
        id: id(r, 0)?,
        notetype_id: id(r, 1)?,
        fields: json(r, 2)?,
        sort_field: r.get(3)?,
        checksum: r.get(4)?,
        tags: Vec::new(),
        created_at: ts(r, 5)?,
        updated_at: ts(r, 6)?,
        deleted_at: opt_ts(r, 7)?,
        home_deck_id: id(r, 8)?,
    })
}

/// Tags of each note in `ids`, in stored order.
fn tags_of(conn: &Connection, ids: &[NoteId]) -> StorageResult<HashMap<NoteId, Vec<TagName>>> {
    let rows = all(
        conn,
        "note_tags",
        "SELECT nt.note_id, t.name FROM note_tags nt JOIN tags t ON t.id = nt.tag_id
         WHERE nt.note_id IN (SELECT value FROM json_each(?1))
         ORDER BY nt.note_id, nt.position, t.name",
        [json_list(ids)],
        |r| Ok((id::<NoteId>(r, 0)?, r.get::<_, TagCol>(1)?.0)),
    )?;
    let mut out: HashMap<NoteId, Vec<TagName>> = HashMap::new();
    for (n, t) in rows {
        out.entry(n).or_default().push(t);
    }
    Ok(out)
}

fn with_tags(conn: &Connection, mut notes: Vec<Note>) -> StorageResult<Vec<Note>> {
    let ids: Vec<NoteId> = notes.iter().map(|n| n.id).collect();
    let mut tags = tags_of(conn, &ids)?;
    for n in &mut notes {
        n.tags = tags.remove(&n.id).unwrap_or_default();
    }
    Ok(notes)
}

/// Replaces a note's `note_tags` rows. Every tag must exist (`NotFound` otherwise; storage
/// never mints tag ids). Case-insensitive duplicates in the list are stored once (first
/// position wins).
fn write_tags(conn: &Connection, note: &Note) -> StorageResult<()> {
    let nid = note.id.to_string();
    exec(
        conn,
        "note_tags",
        "DELETE FROM note_tags WHERE note_id = ?1",
        [&nid],
    )?;
    let mut seen: Vec<TagId> = Vec::new();
    for tag in &note.tags {
        let tid = find_tag(conn, tag)?
            .ok_or_else(|| StorageError::NotFound {
                entity: "tag",
                id: tag.as_str().to_owned(),
            })?
            .id;
        if seen.contains(&tid) {
            continue;
        }
        exec(
            conn,
            "note_tags",
            "INSERT INTO note_tags (note_id, tag_id, position) VALUES (?1, ?2, ?3)",
            params![
                nid,
                tid.to_string(),
                i64::try_from(seen.len()).unwrap_or(i64::MAX)
            ],
        )?;
        seen.push(tid);
    }
    Ok(())
}

/// Inserts or updates the note's FTS row and links it via `notes.fts_rowid`.
fn write_fts(conn: &Connection, note: &Note, search_text: &str) -> StorageResult<()> {
    let nid = note.id.to_string();
    let rowid: Option<i64> = one(
        conn,
        T,
        "SELECT fts_rowid FROM notes WHERE id = ?1",
        [&nid],
        |r| r.get(0),
    )?
    .flatten();
    if let Some(rowid) = rowid {
        let n = exec(
            conn,
            "notes_fts",
            "UPDATE notes_fts SET note_id = ?2, sort_field = ?3, content = ?4 WHERE rowid = ?1",
            params![rowid, nid, note.sort_field, search_text],
        )?;
        if n == 1 {
            return Ok(());
        }
    }
    exec(
        conn,
        "notes_fts",
        "INSERT INTO notes_fts (note_id, sort_field, content) VALUES (?1, ?2, ?3)",
        params![nid, note.sort_field, search_text],
    )?;
    let rowid = conn.last_insert_rowid();
    exec(
        conn,
        T,
        "UPDATE notes SET fts_rowid = ?2 WHERE id = ?1",
        params![nid, rowid],
    )?;
    Ok(())
}

/// Paging cursor of the note list: `<updated_at>:<id>` of the last row.
fn decode_cursor(c: &str) -> StorageResult<(i64, String)> {
    let bad = || StorageError::InvalidCursor(c.to_owned());
    let (t, i) = c.split_once(':').ok_or_else(bad)?;
    let t: i64 = t.parse().map_err(|_| bad())?;
    let i: NoteId = i.parse().map_err(|_| bad())?;
    Ok((t, i.to_string()))
}

impl<M: Mode> NoteRead for SqliteTx<'_, M> {
    fn get_note(&self, note: NoteId) -> StorageResult<Option<Note>> {
        let found = one(
            self.conn(),
            T,
            &format!("SELECT {COLS} FROM notes WHERE id = ?1"),
            [note.to_string()],
            note_row,
        )?;
        match found {
            Some(n) => Ok(with_tags(self.conn(), vec![n])?.pop()),
            None => Ok(None),
        }
    }

    fn list_notes(
        &self,
        filter: &NoteFilter,
        page: &PageRequest,
    ) -> StorageResult<Page<NoteListRow>> {
        let limit = page.effective_limit();
        let cursor = page.cursor.as_deref().map(decode_cursor).transpose()?;
        let (c_time, c_id) = match &cursor {
            Some((t, i)) => (Some(*t), Some(i.as_str())),
            None => (None, None),
        };
        let rows = all(
            self.conn(),
            T,
            &format!(
                "SELECT {COLS},
                     (SELECT count(*) FROM cards c WHERE c.note_id = notes.id
                        AND c.deleted_at IS NULL)
                 FROM notes
                 WHERE (?1 OR deleted_at IS NULL)
                   AND (?2 IS NULL OR EXISTS (
                         SELECT 1 FROM cards c WHERE c.note_id = notes.id
                           AND c.deleted_at IS NULL
                           AND c.deck_id IN (SELECT value FROM json_each(?2))))
                   AND (?3 IS NULL OR updated_at < ?3 OR (updated_at = ?3 AND id < ?4))
                 ORDER BY updated_at DESC, id DESC
                 LIMIT ?5"
            ),
            params![
                filter.include_deleted,
                filter.deck_ids.as_deref().map(json_list),
                c_time,
                c_id,
                i64::from(limit) + 1,
            ],
            |r| Ok((note_row(r)?, r.get::<_, u32>(9)?)),
        )?;
        let has_more = rows.len() > limit as usize;
        let (notes, counts): (Vec<Note>, Vec<u32>) = rows.into_iter().take(limit as usize).unzip();
        let notes = with_tags(self.conn(), notes)?;
        let next_cursor = if has_more {
            notes.last().map(|n| format!("{}:{}", n.updated_at.0, n.id))
        } else {
            None
        };
        Ok(Page {
            items: notes
                .into_iter()
                .zip(counts)
                .map(|(note, card_count)| NoteListRow { note, card_count })
                .collect(),
            next_cursor,
        })
    }

    fn find_notes_by_checksum(
        &self,
        notetype: NoteTypeId,
        checksum: &str,
    ) -> StorageResult<Vec<NoteId>> {
        all(
            self.conn(),
            T,
            "SELECT id FROM notes WHERE notetype_id = ?1 AND checksum = ?2
               AND deleted_at IS NULL ORDER BY id",
            params![notetype.to_string(), checksum],
            |r| id(r, 0),
        )
    }

    fn note_ids_with_tag(&self, tag: &TagName) -> StorageResult<Vec<NoteId>> {
        let tags = all(self.conn(), "tags", "SELECT id, name FROM tags", [], |r| {
            Ok((id::<TagId>(r, 0)?, r.get::<_, TagCol>(1)?.0))
        })?;
        let within: Vec<TagId> = tags
            .into_iter()
            .filter(|(_, name)| name.is_within(tag))
            .map(|(i, _)| i)
            .collect();
        if within.is_empty() {
            return Ok(Vec::new());
        }
        all(
            self.conn(),
            "note_tags",
            "SELECT DISTINCT nt.note_id FROM note_tags nt JOIN notes n ON n.id = nt.note_id
             WHERE n.deleted_at IS NULL AND nt.tag_id IN (SELECT value FROM json_each(?1))
             ORDER BY nt.note_id",
            [json_list(&within)],
            |r| id(r, 0),
        )
    }

    fn note_home_deck(&self, note: NoteId) -> StorageResult<Option<DeckId>> {
        one(
            self.conn(),
            T,
            "SELECT home_deck_id FROM notes WHERE id = ?1",
            [note.to_string()],
            |r| id(r, 0),
        )
    }
}

impl NoteWrite for SqliteTx<'_, WriteMode> {
    fn insert_note(&mut self, note: &Note, search_text: &str) -> StorageResult<()> {
        let conn = self.conn();
        exec(
            conn,
            T,
            &format!(
                "INSERT INTO notes ({COLS})
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)"
            ),
            params![
                note.id.to_string(),
                note.notetype_id.to_string(),
                to_json(T, &note.fields)?,
                note.sort_field,
                note.checksum,
                note.created_at.0,
                note.updated_at.0,
                opt_ms(note.deleted_at),
                note.home_deck_id.to_string(),
            ],
        )?;
        write_tags(conn, note)?;
        write_fts(conn, note, search_text)
    }

    fn update_note(&mut self, note: &Note, search_text: &str) -> StorageResult<()> {
        let conn = self.conn();
        exec_one(
            conn,
            T,
            "note",
            &note.id,
            "UPDATE notes SET notetype_id = ?2, fields = ?3, sort_field = ?4, checksum = ?5,
                 created_at = ?6, updated_at = ?7, deleted_at = ?8, home_deck_id = ?9
             WHERE id = ?1",
            params![
                note.id.to_string(),
                note.notetype_id.to_string(),
                to_json(T, &note.fields)?,
                note.sort_field,
                note.checksum,
                note.created_at.0,
                note.updated_at.0,
                opt_ms(note.deleted_at),
                note.home_deck_id.to_string(),
            ],
        )?;
        write_tags(conn, note)?;
        write_fts(conn, note, search_text)
    }

    fn set_notes_deleted(
        &mut self,
        ids: &[NoteId],
        deleted_at: Option<Timestamp>,
    ) -> StorageResult<u32> {
        set_deleted(self.conn(), T, ids, deleted_at)
    }

    fn set_note_home_deck(&mut self, note: NoteId, home_deck: DeckId) -> StorageResult<()> {
        exec_one(
            self.conn(),
            T,
            "note",
            &note,
            "UPDATE notes SET home_deck_id = ?2 WHERE id = ?1",
            params![note.to_string(), home_deck.to_string()],
        )
    }
}
