//! [`MemTx`] repository traits: Attributes and their values.

use super::*;

impl AttributeRead for MemTx<'_> {
    fn get_attribute_def(&self, id: AttributeDefId) -> StorageResult<Option<AttributeDef>> {
        Ok(self.s().attr_defs.get(&id).cloned())
    }

    fn list_attribute_defs(&self, include_deleted: bool) -> StorageResult<Vec<AttributeDef>> {
        Ok(self
            .s()
            .attr_defs
            .values()
            .filter(|d| include_deleted || d.deleted_at.is_none())
            .cloned()
            .collect())
    }

    fn get_attribute_values(
        &self,
        entity: EntityRef,
    ) -> StorageResult<Vec<(AttributeDefId, AttributeValue)>> {
        let s = self.s();
        let mut out = Vec::new();
        for (attr, e, rows) in &s.attr_values {
            if *e != entity {
                continue;
            }
            let Some(def) = s.attr_defs.get(attr).filter(|d| d.deleted_at.is_none()) else {
                continue;
            };
            let value = AttributeValue::from_stored(def, rows).ok_or(StorageError::Corrupt {
                table: "attribute_values",
                message: format!("cannot decode value of {attr}"),
            })?;
            out.push((*attr, value));
        }
        out.sort_by_key(|(id, _)| *id);
        Ok(out)
    }

    fn entities_with_option(
        &self,
        attr: AttributeDefId,
        option: AttributeOptionId,
    ) -> StorageResult<Vec<EntityRef>> {
        let wanted = option.to_string();
        Ok(self
            .s()
            .attr_values
            .iter()
            .filter(|(a, _, rows)| *a == attr && rows.iter().any(|r| r.value == wanted))
            .map(|(_, e, _)| *e)
            .collect())
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

    fn effective_attribute_defs_for_note(
        &self,
        note: NoteId,
    ) -> StorageResult<Vec<(AttributeDef, usize)>> {
        let home = self
            .note_home_deck(note)?
            .ok_or_else(|| not_found("note", note))?;
        effective_for(self, home, AttributeTarget::Note)
    }

    fn effective_attribute_defs_for_card(
        &self,
        card: CardId,
    ) -> StorageResult<Vec<(AttributeDef, usize)>> {
        let deck = self
            .s()
            .cards
            .get(&card)
            .ok_or_else(|| not_found("card", card))?
            .deck_id;
        effective_for(self, deck, AttributeTarget::Card)
    }
}

fn effective_for(
    tx: &MemTx<'_>,
    deck: DeckId,
    target: AttributeTarget,
) -> StorageResult<Vec<(AttributeDef, usize)>> {
    Ok(tx
        .effective_attribute_defs_for_deck(Some(deck))?
        .into_iter()
        .filter(|(d, _)| d.target == target)
        .collect())
}

fn option_not_found(id: AttributeOptionId) -> StorageError {
    not_found("attribute_option", id)
}

/// A stored row pointing at `option`.
fn option_row(option: AttributeOptionId) -> StoredValue {
    StoredValue {
        value: option.to_string(),
        value_num: None,
    }
}

impl MemState {
    fn def_mut(&mut self, attr: AttributeDefId) -> StorageResult<&mut AttributeDef> {
        self.attr_defs
            .get_mut(&attr)
            .ok_or_else(|| not_found("attribute", attr))
    }

    fn values_mut(&mut self, attr: AttributeDefId, entity: EntityRef) -> &mut Vec<StoredValue> {
        let i = match self
            .attr_values
            .iter()
            .position(|(a, e, _)| *a == attr && *e == entity)
        {
            Some(i) => i,
            None => {
                self.attr_values.push((attr, entity, Vec::new()));
                self.attr_values.len() - 1
            }
        };
        &mut self.attr_values[i].2
    }
}

impl AttributeWrite for MemTx<'_> {
    fn insert_attribute_def(&mut self, def: &AttributeDef) -> StorageResult<()> {
        if self.s().attr_defs.contains_key(&def.id) {
            return Err(StorageError::Constraint {
                constraint: "attribute_defs_pk".into(),
            });
        }
        self.m().attr_defs.insert(def.id, def.clone());
        Ok(())
    }

    fn update_attribute_def(&mut self, def: &AttributeDef) -> StorageResult<()> {
        let slot = self
            .m()
            .attr_defs
            .get_mut(&def.id)
            .ok_or_else(|| not_found("attribute", def.id))?;
        *slot = def.clone();
        Ok(())
    }

    fn set_attribute_def_deleted(
        &mut self,
        id: AttributeDefId,
        deleted_at: Option<Timestamp>,
    ) -> StorageResult<()> {
        let def = self
            .m()
            .attr_defs
            .get_mut(&id)
            .ok_or_else(|| not_found("attribute", id))?;
        def.deleted_at = deleted_at;
        Ok(())
    }

    fn set_attribute_values(
        &mut self,
        entity: EntityRef,
        values: &[(AttributeDefId, Option<AttributeValue>)],
    ) -> StorageResult<()> {
        for (attr, value) in values {
            if !self.s().attr_defs.contains_key(attr) {
                return Err(not_found("attribute", attr));
            }
            let s = self.m();
            s.attr_values
                .retain(|(a, e, _)| !(a == attr && *e == entity));
            if let Some(v) = value {
                s.attr_values.push((*attr, entity, v.to_stored()));
            }
        }
        Ok(())
    }

    fn insert_attribute_option(
        &mut self,
        attr: AttributeDefId,
        option: &AttributeOption,
        index: usize,
        now: Timestamp,
    ) -> StorageResult<()> {
        let taken = self
            .s()
            .attr_defs
            .values()
            .any(|d| d.options.iter().any(|o| o.id == option.id));
        if taken {
            return Err(StorageError::Constraint {
                constraint: "attribute_option_id".into(),
            });
        }
        let def = self.m().def_mut(attr)?;
        def.insert_option_at(option.clone(), index)
            .map_err(|e| StorageError::Corrupt {
                table: "attribute_defs",
                message: e.to_string(),
            })?;
        def.updated_at = now;
        Ok(())
    }

    fn remove_attribute_option(
        &mut self,
        attr: AttributeDefId,
        option: AttributeOptionId,
        replace_with: Option<AttributeOptionId>,
        now: Timestamp,
    ) -> StorageResult<OptionRemoval> {
        let s = self.m();
        let def = s.def_mut(attr)?;
        let sorted: Vec<AttributeOption> = def.sorted_options().into_iter().cloned().collect();
        let index = sorted
            .iter()
            .position(|o| o.id == option)
            .ok_or_else(|| option_not_found(option))?;
        if let Some(r) = replace_with {
            if r == option || !sorted.iter().any(|o| o.id == r) {
                return Err(option_not_found(r));
            }
        }
        let removed = sorted[index].clone();
        def.remove_option(option)
            .map_err(|e| StorageError::Corrupt {
                table: "attribute_defs",
                message: e.to_string(),
            })?;
        def.updated_at = now;
        let (old, new) = (option.to_string(), replace_with.map(|r| r.to_string()));
        let (mut moved, mut dropped) = (Vec::new(), Vec::new());
        for (a, entity, rows) in &mut s.attr_values {
            if *a != attr {
                continue;
            }
            let Some(pos) = rows.iter().position(|r| r.value == old) else {
                continue;
            };
            match &new {
                Some(n) if !rows.iter().any(|r| &r.value == n) => {
                    rows[pos].value = n.clone();
                    moved.push(*entity);
                }
                _ => {
                    rows.remove(pos);
                    dropped.push(*entity);
                }
            }
        }
        s.attr_values.retain(|(_, _, rows)| !rows.is_empty());
        Ok(OptionRemoval {
            option: removed,
            index,
            replaced_with: replace_with,
            moved,
            dropped,
        })
    }

    fn restore_attribute_option(
        &mut self,
        attr: AttributeDefId,
        removal: &OptionRemoval,
        now: Timestamp,
    ) -> StorageResult<()> {
        self.insert_attribute_option(attr, &removal.option, removal.index, now)?;
        let s = self.m();
        let id = removal.option.id;
        for entity in &removal.moved {
            let rows = s.values_mut(attr, *entity);
            let from = removal.replaced_with.map(|r| r.to_string());
            match rows.iter_mut().find(|r| Some(&r.value) == from.as_ref()) {
                Some(r) => r.value = id.to_string(),
                None => rows.push(option_row(id)),
            }
        }
        for entity in &removal.dropped {
            s.values_mut(attr, *entity).push(option_row(id));
        }
        Ok(())
    }
}
