//! Classifications: definitions (`attribute_defs`, options as JSON with stable ids) and
//! values (`attribute_values`, one row per value; multi-select = several rows).

use super::{all, enum_col, enum_str, exec, exec_one, id, json, one, opt_ms, opt_ts, to_json, ts};
use super::{Mode, SqliteTx, WriteMode};
use crate::error::{StorageError, StorageResult};
use crate::repo::{AttributeRead, AttributeWrite, DeckRead};
use fc_core::attribute::{
    effective_attribute_defs, AttributeDef, AttributeKind, AttributeTarget, AttributeValue,
    EntityRef, StoredValue,
};
use fc_core::{AttributeDefId, AttributeOptionId, CardId, DeckId, NoteId, Timestamp};
use rusqlite::{params, Row};
use std::collections::HashMap;

const T: &str = "attribute_defs";
const V: &str = "attribute_values";
const COLS: &str = "id, scope_deck_id, key, name, kind, options, target, required, \
                    default_value, position, template_id, created_at, updated_at, deleted_at";

fn def_row(r: &Row<'_>) -> rusqlite::Result<AttributeDef> {
    Ok(AttributeDef {
        id: id(r, 0)?,
        scope_deck_id: super::opt_id(r, 1)?,
        key: r.get(2)?,
        name: r.get(3)?,
        kind: enum_col(r, 4)?,
        options: json(r, 5)?,
        target: enum_col(r, 6)?,
        required: r.get(7)?,
        default_value: r
            .get::<_, Option<super::Json<AttributeValue>>>(8)?
            .map(|j| j.0),
        position: r.get(9)?,
        template_id: r.get(10)?,
        created_at: ts(r, 11)?,
        updated_at: ts(r, 12)?,
        deleted_at: opt_ts(r, 13)?,
    })
}

fn def_params(d: &AttributeDef) -> StorageResult<Vec<Box<dyn rusqlite::ToSql + '_>>> {
    Ok(vec![
        Box::new(d.id.to_string()),
        Box::new(d.scope_deck_id.map(|s| s.to_string())),
        Box::new(&d.key),
        Box::new(&d.name),
        Box::new(enum_str(T, &d.kind)?),
        Box::new(to_json(T, &d.options)?),
        Box::new(enum_str(T, &d.target)?),
        Box::new(d.required),
        Box::new(
            d.default_value
                .as_ref()
                .map(|v| to_json(T, v))
                .transpose()?,
        ),
        Box::new(d.position),
        Box::new(&d.template_id),
        Box::new(d.created_at.0),
        Box::new(d.updated_at.0),
        Box::new(opt_ms(d.deleted_at)),
    ])
}

fn entity_parts(e: &EntityRef) -> (&'static str, String) {
    (e.target().as_str(), e.uuid().to_string())
}

/// Decodes one attribute's rows. Multi-select options come back in the definition's option
/// order (unknown ids last, by id).
fn decode(def: &AttributeDef, mut rows: Vec<StoredValue>) -> StorageResult<AttributeValue> {
    if def.kind == AttributeKind::MultiEnum {
        let order: HashMap<String, i32> = def
            .options
            .iter()
            .map(|o| (o.id.to_string(), o.order))
            .collect();
        rows.sort_by(|a, b| {
            let oa = order.get(&a.value).copied().unwrap_or(i32::MAX);
            let ob = order.get(&b.value).copied().unwrap_or(i32::MAX);
            oa.cmp(&ob).then_with(|| a.value.cmp(&b.value))
        });
    }
    AttributeValue::from_stored(def.kind, &rows).ok_or_else(|| StorageError::Corrupt {
        table: V,
        message: format!("cannot decode value of attribute {} ({})", def.key, def.id),
    })
}

impl<M: Mode> AttributeRead for SqliteTx<'_, M> {
    fn get_attribute_def(&self, def: AttributeDefId) -> StorageResult<Option<AttributeDef>> {
        one(
            self.conn(),
            T,
            &format!("SELECT {COLS} FROM attribute_defs WHERE id = ?1"),
            [def.to_string()],
            def_row,
        )
    }

    fn list_attribute_defs(&self, include_deleted: bool) -> StorageResult<Vec<AttributeDef>> {
        all(
            self.conn(),
            T,
            &format!(
                "SELECT {COLS} FROM attribute_defs WHERE ?1 OR deleted_at IS NULL
                 ORDER BY scope_deck_id IS NOT NULL, scope_deck_id, position, key, id"
            ),
            [include_deleted],
            def_row,
        )
    }

    fn get_attribute_values(
        &self,
        entity: EntityRef,
    ) -> StorageResult<Vec<(AttributeDefId, AttributeValue)>> {
        let (kind, eid) = entity_parts(&entity);
        let rows = all(
            self.conn(),
            V,
            "SELECT attr_id, value, value_num FROM attribute_values
             WHERE entity_kind = ?1 AND entity_id = ?2 ORDER BY attr_id, value",
            params![kind, eid],
            |r| {
                Ok((
                    id::<AttributeDefId>(r, 0)?,
                    StoredValue {
                        value: r.get(1)?,
                        value_num: r.get(2)?,
                    },
                ))
            },
        )?;
        if rows.is_empty() {
            return Ok(Vec::new());
        }
        let mut grouped: HashMap<AttributeDefId, Vec<StoredValue>> = HashMap::new();
        for (a, v) in rows {
            grouped.entry(a).or_default().push(v);
        }
        // Decode in definition order (live definitions only).
        let defs = self.list_attribute_defs(false)?;
        let mut out = Vec::new();
        for def in &defs {
            if let Some(rows) = grouped.remove(&def.id) {
                out.push((def.id, decode(def, rows)?));
            }
        }
        Ok(out)
    }

    fn entities_with_option(
        &self,
        attr: AttributeDefId,
        option: AttributeOptionId,
    ) -> StorageResult<Vec<EntityRef>> {
        all(
            self.conn(),
            V,
            "SELECT entity_kind, entity_id FROM attribute_values
             WHERE attr_id = ?1 AND value = ?2 ORDER BY entity_kind, entity_id",
            params![attr.to_string(), option.to_string()],
            |r| {
                let kind: AttributeTarget = enum_col(r, 0)?;
                Ok(match kind {
                    AttributeTarget::Note => EntityRef::Note(id::<NoteId>(r, 1)?),
                    AttributeTarget::Card => EntityRef::Card(id::<CardId>(r, 1)?),
                })
            },
        )
    }

    fn effective_attribute_defs_for_deck(
        &self,
        deck: Option<DeckId>,
    ) -> StorageResult<Vec<(AttributeDef, usize)>> {
        let ancestry = match deck {
            Some(d) => self.deck_ancestry(d)?,
            None => Vec::new(),
        };
        let defs = self.list_attribute_defs(false)?;
        Ok(effective_attribute_defs(&ancestry, &defs)
            .into_iter()
            .map(|e| (e.def.clone(), e.depth))
            .collect())
    }
}

impl AttributeWrite for SqliteTx<'_, WriteMode> {
    fn insert_attribute_def(&mut self, d: &AttributeDef) -> StorageResult<()> {
        let p = def_params(d)?;
        exec(
            self.conn(),
            T,
            &format!(
                "INSERT INTO attribute_defs ({COLS})
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)"
            ),
            rusqlite::params_from_iter(p.iter()),
        )?;
        Ok(())
    }

    fn update_attribute_def(&mut self, d: &AttributeDef) -> StorageResult<()> {
        let p = def_params(d)?;
        exec_one(
            self.conn(),
            T,
            "attribute",
            &d.id,
            "UPDATE attribute_defs SET scope_deck_id = ?2, key = ?3, name = ?4, kind = ?5,
                 options = ?6, target = ?7, required = ?8, default_value = ?9, position = ?10,
                 template_id = ?11, created_at = ?12, updated_at = ?13, deleted_at = ?14
             WHERE id = ?1",
            rusqlite::params_from_iter(p.iter()),
        )
    }

    fn set_attribute_def_deleted(
        &mut self,
        def: AttributeDefId,
        deleted_at: Option<Timestamp>,
    ) -> StorageResult<()> {
        exec_one(
            self.conn(),
            T,
            "attribute",
            &def,
            "UPDATE attribute_defs SET deleted_at = ?2 WHERE id = ?1",
            params![def.to_string(), opt_ms(deleted_at)],
        )
    }

    fn set_attribute_values(
        &mut self,
        entity: EntityRef,
        values: &[(AttributeDefId, Option<AttributeValue>)],
    ) -> StorageResult<()> {
        let (kind, eid) = entity_parts(&entity);
        let conn = self.conn();
        for (attr, value) in values {
            let aid = attr.to_string();
            exec(
                conn,
                V,
                "DELETE FROM attribute_values
                 WHERE attr_id = ?1 AND entity_kind = ?2 AND entity_id = ?3",
                params![aid, kind, eid],
            )?;
            let Some(value) = value else { continue };
            for row in value.to_stored() {
                exec(
                    conn,
                    V,
                    "INSERT INTO attribute_values (attr_id, entity_kind, entity_id, value, value_num)
                     VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![aid, kind, eid, row.value, row.value_num],
                )?;
            }
        }
        Ok(())
    }
}
