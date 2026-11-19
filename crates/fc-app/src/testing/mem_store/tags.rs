//! [`MemTx`] repository traits: Tags.

use super::*;

impl TagRead for MemTx<'_> {
    fn list_tags(&self) -> StorageResult<Vec<(Tag, u32)>> {
        let s = self.s();
        Ok(s.tags
            .iter()
            .map(|(key, tag)| {
                let n = s
                    .notes
                    .values()
                    .filter(|n| n.deleted_at.is_none() && n.tags.iter().any(|t| &t.key() == key))
                    .count();
                (tag.clone(), u32::try_from(n).unwrap_or(u32::MAX))
            })
            .collect())
    }

    fn get_tag_by_name(&self, name: &TagName) -> StorageResult<Option<Tag>> {
        Ok(self.s().tags.get(&name.key()).cloned())
    }
}

impl TagWrite for MemTx<'_> {
    fn upsert_tag(&mut self, tag: &Tag) -> StorageResult<()> {
        self.m().tags.insert(tag.name.key(), tag.clone());
        Ok(())
    }

    fn delete_unused_tags(&mut self) -> StorageResult<u32> {
        let s = self.m();
        let before = s.tags.len();
        let used: Vec<String> = s
            .notes
            .values()
            .flat_map(|n| n.tags.iter().map(TagName::key))
            .collect();
        s.tags.retain(|k, _| used.contains(k));
        Ok(u32::try_from(before - s.tags.len()).unwrap_or(u32::MAX))
    }

    fn ensure_tags(
        &mut self,
        tags: &[(TagName, TagId)],
        now: Timestamp,
    ) -> StorageResult<Vec<EnsuredTag>> {
        let mut out = Vec::with_capacity(tags.len());
        for (name, id) in tags {
            if let Some(t) = self.s().tags.get(&name.key()) {
                out.push(EnsuredTag {
                    id: t.id,
                    created: false,
                });
                continue;
            }
            if self.s().tags.values().any(|t| t.id == *id) {
                return Err(StorageError::Constraint {
                    constraint: "tags_pk".into(),
                });
            }
            self.m().tags.insert(
                name.key(),
                Tag {
                    id: *id,
                    name: name.clone(),
                    color: None,
                    created_at: now,
                    updated_at: now,
                },
            );
            out.push(EnsuredTag {
                id: *id,
                created: true,
            });
        }
        Ok(out)
    }

    fn merge_tag(
        &mut self,
        from: TagId,
        into: TagId,
        now: Timestamp,
    ) -> StorageResult<Vec<NoteId>> {
        let find = |s: &MemState, id: TagId| s.tags.values().find(|t| t.id == id).cloned();
        let src = find(self.s(), from).ok_or_else(|| not_found("tag", from))?;
        let dst = find(self.s(), into).ok_or_else(|| not_found("tag", into))?;
        let s = self.m();
        let mut touched = Vec::new();
        for n in s.notes.values_mut() {
            let Some(pos) = n.tags.iter().position(|t| t.key() == src.name.key()) else {
                continue;
            };
            if n.tags.iter().any(|t| t.key() == dst.name.key()) {
                n.tags.remove(pos);
            } else {
                n.tags[pos] = dst.name.clone();
            }
            n.updated_at = now;
            touched.push(n.id);
        }
        s.tags.remove(&src.name.key());
        Ok(touched)
    }
}
