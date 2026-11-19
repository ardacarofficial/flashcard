//! Note types.

use super::{all, enum_col, enum_str, exec, id, json, one, opt_ms, opt_ts, to_json, ts};
use super::{Mode, SqliteTx, WriteMode};
use crate::error::StorageResult;
use crate::repo::{NoteTypeRead, NoteTypeWrite};
use fc_core::notetype::{FieldKey, NoteType};
use fc_core::NoteTypeId;
use rusqlite::{params, Row};

const T: &str = "notetypes";
const COLS: &str = "id, name, kind, fields, templates, css, sort_field, version, builtin, \
                    created_at, updated_at, deleted_at";

fn notetype_row(r: &Row<'_>) -> rusqlite::Result<NoteType> {
    Ok(NoteType {
        id: id(r, 0)?,
        name: r.get(1)?,
        kind: enum_col(r, 2)?,
        fields: json(r, 3)?,
        templates: json(r, 4)?,
        css: r.get(5)?,
        sort_field: FieldKey::new(r.get::<_, String>(6)?),
        version: r.get(7)?,
        builtin: r.get(8)?,
        created_at: ts(r, 9)?,
        updated_at: ts(r, 10)?,
        deleted_at: opt_ts(r, 11)?,
    })
}

impl<M: Mode> NoteTypeRead for SqliteTx<'_, M> {
    fn get_notetype(&self, nt: NoteTypeId) -> StorageResult<Option<NoteType>> {
        one(
            self.conn(),
            T,
            &format!("SELECT {COLS} FROM notetypes WHERE id = ?1"),
            [nt.to_string()],
            notetype_row,
        )
    }

    fn list_notetypes(&self, include_deleted: bool) -> StorageResult<Vec<NoteType>> {
        all(
            self.conn(),
            T,
            &format!(
                "SELECT {COLS} FROM notetypes WHERE ?1 OR deleted_at IS NULL \
                 ORDER BY name COLLATE NOCASE, id"
            ),
            [include_deleted],
            notetype_row,
        )
    }
}

impl NoteTypeWrite for SqliteTx<'_, WriteMode> {
    fn upsert_notetype(&mut self, nt: &NoteType) -> StorageResult<()> {
        exec(
            self.conn(),
            T,
            &format!(
                "INSERT INTO notetypes ({COLS})
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
                 ON CONFLICT(id) DO UPDATE SET name = excluded.name, kind = excluded.kind,
                     fields = excluded.fields, templates = excluded.templates,
                     css = excluded.css, sort_field = excluded.sort_field,
                     version = excluded.version, builtin = excluded.builtin,
                     created_at = excluded.created_at, updated_at = excluded.updated_at,
                     deleted_at = excluded.deleted_at"
            ),
            params![
                nt.id.to_string(),
                nt.name,
                enum_str(T, &nt.kind)?,
                to_json(T, &nt.fields)?,
                to_json(T, &nt.templates)?,
                nt.css,
                nt.sort_field.as_str(),
                nt.version,
                nt.builtin,
                nt.created_at.0,
                nt.updated_at.0,
                opt_ms(nt.deleted_at),
            ],
        )?;
        Ok(())
    }
}
