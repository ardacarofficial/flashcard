//! [`MemTx`] repository traits: card content (media, note references, flags).

use super::*;
use fc_core::note::fnv1a64;
use fc_core::notetype::FieldKey;

/// Content tables.
#[derive(Debug, Clone)]
pub struct ContentState {
    media: BTreeMap<String, MediaRecord>,
    files: BTreeMap<String, Vec<u8>>,
    refs: BTreeMap<NoteId, NoteRefs>,
    flags: Vec<FlagDef>,
}

impl Default for ContentState {
    fn default() -> Self {
        let colors = ["red", "orange", "green", "blue", "pink", "turquoise", "purple"];
        Self {
            media: BTreeMap::new(),
            files: BTreeMap::new(),
            refs: BTreeMap::new(),
            flags: (1u8..)
                .zip(colors)
                .map(|(flag, c)| FlagDef {
                    flag,
                    name: None,
                    color: c.to_owned(),
                    updated_at: Timestamp(0),
                })
                .collect(),
        }
    }
}

impl MemTx<'_> {
    fn c(&self) -> &ContentState {
        &self.s().domains.content
    }

    /// `m` with `ref_count` counted from the references of existing notes.
    fn counted(&self, m: &MediaRecord) -> MediaRecord {
        let n = self
            .c()
            .refs
            .iter()
            .filter(|(id, r)| self.s().notes.contains_key(id) && r.media.contains(&m.hash))
            .count();
        MediaRecord {
            ref_count: u32::try_from(n).unwrap_or(u32::MAX),
            ..m.clone()
        }
    }

    fn live_note(&self, id: NoteId) -> bool {
        self.s()
            .notes
            .get(&id)
            .is_some_and(|n| n.deleted_at.is_none())
    }
}

impl ContentRead for MemTx<'_> {
    fn get_media(&self, hash: &str) -> StorageResult<Option<MediaRecord>> {
        Ok(self.c().media.get(hash).map(|m| self.counted(m)))
    }

    fn list_media(&self) -> StorageResult<Vec<MediaRecord>> {
        let mut v: Vec<MediaRecord> = self.c().media.values().map(|m| self.counted(m)).collect();
        v.sort_by(|a, b| b.created_at.cmp(&a.created_at).then(a.hash.cmp(&b.hash)));
        Ok(v)
    }

    fn read_media_file(&self, hash: &str) -> StorageResult<Option<Vec<u8>>> {
        Ok(self.c().files.get(hash).cloned())
    }

    fn note_media(&self, note: NoteId) -> StorageResult<Vec<String>> {
        let mut v = self
            .c()
            .refs
            .get(&note)
            .map(|r| r.media.clone())
            .unwrap_or_default();
        v.sort();
        Ok(v)
    }

    fn notes_by_link_key(&self, keys: &[String]) -> StorageResult<Vec<(String, NoteId)>> {
        let mut v: Vec<(String, Timestamp, NoteId)> = self
            .c()
            .refs
            .iter()
            .filter(|(id, r)| keys.contains(&r.key) && self.live_note(**id))
            .filter_map(|(id, r)| {
                let n = self.s().notes.get(id)?;
                Some((r.key.clone(), n.created_at, *id))
            })
            .collect();
        v.sort();
        Ok(v.into_iter().map(|(k, _, id)| (k, id)).collect())
    }

    fn notes_linking_to(&self, key: &str) -> StorageResult<Vec<NoteId>> {
        Ok(self
            .c()
            .refs
            .iter()
            .filter(|(id, r)| self.live_note(**id) && r.links.iter().any(|(k, _)| k == key))
            .map(|(id, _)| *id)
            .collect())
    }

    fn notes_with_key_prefix(
        &self,
        prefix: &str,
        limit: u32,
    ) -> StorageResult<Vec<(NoteId, String)>> {
        let mut v: Vec<(String, NoteId)> = self
            .c()
            .refs
            .iter()
            .filter(|(id, r)| self.live_note(**id) && r.key.starts_with(prefix))
            .map(|(id, r)| (r.key.clone(), *id))
            .collect();
        v.sort();
        Ok(v.into_iter()
            .take(limit as usize)
            .filter_map(|(_, id)| Some((id, self.s().notes.get(&id)?.sort_field.clone())))
            .collect())
    }

    fn note_ids_of_notetype(
        &self,
        notetype: NoteTypeId,
        include_deleted: bool,
    ) -> StorageResult<Vec<NoteId>> {
        Ok(self
            .s()
            .notes
            .values()
            .filter(|n| n.notetype_id == notetype && (include_deleted || n.deleted_at.is_none()))
            .map(|n| n.id)
            .collect())
    }

    fn list_flag_defs(&self) -> StorageResult<Vec<FlagDef>> {
        Ok(self.c().flags.clone())
    }

    fn sample_field_values(&self, q: &FieldSample<'_>) -> StorageResult<Vec<String>> {
        let s = self.s();
        let mut v: Vec<(u64, NoteId, String)> = s
            .notes
            .values()
            .filter(|n| n.id != q.exclude && n.notetype_id == q.notetype && n.deleted_at.is_none())
            .filter(|n| {
                s.cards.values().any(|c| {
                    c.note_id == n.id && c.deck_id == q.deck && c.deleted_at.is_none()
                })
            })
            .filter_map(|n| {
                let value = n.fields.get(&FieldKey::new(q.field))?;
                let order = fnv1a64(format!("{}{}", q.seed, n.id).as_bytes());
                (!value.is_empty()).then(|| (order, n.id, value.clone()))
            })
            .collect();
        v.sort();
        Ok(v.into_iter()
            .take(q.limit as usize)
            .map(|(_, _, value)| value)
            .collect())
    }
}

impl ContentWrite for MemTx<'_> {
    fn insert_media(&mut self, media: &MediaRecord) -> StorageResult<()> {
        let mut m = media.clone();
        m.ref_count = 0;
        self.m()
            .domains
            .content
            .media
            .entry(m.hash.clone())
            .or_insert(m);
        Ok(())
    }

    fn write_media_file(&mut self, hash: &str, bytes: &[u8]) -> StorageResult<()> {
        self.m()
            .domains
            .content
            .files
            .entry(hash.to_owned())
            .or_insert_with(|| bytes.to_vec());
        Ok(())
    }

    fn delete_unreferenced_media(&mut self, hash: &str) -> StorageResult<bool> {
        let used = self.c().refs.values().any(|r| r.media.iter().any(|h| h == hash));
        Ok(!used && self.m().domains.content.media.remove(hash).is_some())
    }

    fn delete_orphan_media_files(&mut self) -> StorageResult<u64> {
        let content = &mut self.m().domains.content;
        let before = content.files.len();
        let media = &content.media;
        content.files.retain(|h, _| media.contains_key(h));
        Ok((before - content.files.len()) as u64)
    }

    fn set_note_refs(&mut self, note: NoteId, refs: &NoteRefs) -> StorageResult<()> {
        if !self.s().notes.contains_key(&note) {
            return Err(not_found("note", note));
        }
        if let Some(missing) = refs.media.iter().find(|h| !self.c().media.contains_key(*h)) {
            return Err(StorageError::Constraint {
                constraint: format!("foreign_key: media {missing}"),
            });
        }
        self.m().domains.content.refs.insert(note, refs.clone());
        Ok(())
    }

    fn set_flag_def(&mut self, def: &FlagDef) -> StorageResult<()> {
        let slot = self
            .m()
            .domains
            .content
            .flags
            .iter_mut()
            .find(|f| f.flag == def.flag)
            .ok_or_else(|| not_found("flag", def.flag))?;
        *slot = def.clone();
        Ok(())
    }
}
