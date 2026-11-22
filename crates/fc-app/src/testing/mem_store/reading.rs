//! [`MemTx`] repository traits: reading mode (v0.10).

use super::*;
use fc_core::notetype::FieldKey;
use fc_core::ReadingItemId;

/// `reading_items`, `word_marks`.
#[derive(Debug, Clone, Default)]
pub struct ReadingState {
    items: BTreeMap<ReadingItemId, ReadingItemRecord>,
    pub(super) marks: BTreeMap<(String, String), WordMarkRecord>,
}

fn with_count(mut r: ReadingItemRecord) -> ReadingItemRecord {
    r.cue_count = r
        .cues
        .as_deref()
        .and_then(|c| serde_json::from_str::<Vec<Value>>(c).ok())
        .map_or(0, |v| i64::try_from(v.len()).unwrap_or(i64::MAX));
    r
}

impl ReadingRead for MemTx<'_> {
    fn list_reading_items(&self, include_deleted: bool) -> StorageResult<Vec<ReadingItemRecord>> {
        let mut v: Vec<ReadingItemRecord> = self
            .s()
            .domains
            .reading
            .items
            .values()
            .filter(|i| include_deleted || i.deleted_at.is_none())
            .cloned()
            .map(|i| ReadingItemRecord {
                body: String::new(),
                cues: None,
                ..with_count(i)
            })
            .collect();
        v.sort_by_key(|i| std::cmp::Reverse((i.created_at, i.id)));
        Ok(v)
    }

    fn get_reading_item(&self, id: ReadingItemId) -> StorageResult<Option<ReadingItemRecord>> {
        Ok(self.s().domains.reading.items.get(&id).cloned().map(with_count))
    }

    fn list_word_marks(&self, language: &str) -> StorageResult<Vec<WordMarkRecord>> {
        Ok(self
            .s()
            .domains
            .reading
            .marks
            .values()
            .filter(|m| m.language == language)
            .cloned()
            .collect())
    }

    fn vocabulary_rows(&self) -> StorageResult<Vec<VocabularyRow>> {
        let s = self.s();
        let word_key = FieldKey::new("word");
        let mut cards: Vec<&Card> = s.cards.values().filter(|c| c.deleted_at.is_none()).collect();
        cards.sort_by_key(|c| (c.created_at, c.id));
        Ok(cards
            .into_iter()
            .filter_map(|c| {
                let note = s.notes.get(&c.note_id).filter(|n| n.deleted_at.is_none())?;
                let word = note
                    .fields
                    .get(&word_key)
                    .filter(|w| !w.is_empty())
                    .cloned()
                    .unwrap_or_else(|| note.sort_field.clone());
                Some(VocabularyRow {
                    card_id: c.id,
                    note_id: c.note_id,
                    word,
                    state: c.scheduling.state,
                    due: c.scheduling.due,
                    last_review: c.scheduling.last_review,
                })
            })
            .collect())
    }
}

impl ReadingWrite for MemTx<'_> {
    fn upsert_reading_item(&mut self, item: &ReadingItemRecord) -> StorageResult<()> {
        if let Some(p) = item.parent_id {
            if !self.s().domains.reading.items.contains_key(&p) {
                return Err(StorageError::Constraint {
                    constraint: "reading_items.parent_id".into(),
                });
            }
        }
        self.m()
            .domains
            .reading
            .items
            .insert(item.id, item.clone());
        Ok(())
    }

    fn set_reading_items_deleted(
        &mut self,
        ids: &[ReadingItemId],
        deleted_at: Option<Timestamp>,
    ) -> StorageResult<u32> {
        let mut n = 0;
        for id in ids {
            if let Some(i) = self.m().domains.reading.items.get_mut(id) {
                i.deleted_at = deleted_at;
                n += 1;
            }
        }
        Ok(n)
    }

    fn purge_reading_items(&mut self, before: Timestamp) -> StorageResult<u32> {
        let items = &mut self.m().domains.reading.items;
        let gone: Vec<ReadingItemId> = items
            .values()
            .filter(|i| i.deleted_at.is_some_and(|d| d < before))
            .map(|i| i.id)
            .collect();
        for id in &gone {
            items.remove(id);
        }
        for i in items.values_mut() {
            if i.parent_id.is_some_and(|p| gone.contains(&p)) {
                i.parent_id = None;
            }
        }
        Ok(u32::try_from(gone.len()).unwrap_or(u32::MAX))
    }

    fn set_word_mark(
        &mut self,
        language: &str,
        lemma: &str,
        status: Option<&str>,
        now: Timestamp,
    ) -> StorageResult<()> {
        let key = (language.to_owned(), lemma.to_owned());
        let marks = &mut self.m().domains.reading.marks;
        match status {
            Some(s) => {
                marks.insert(
                    key,
                    WordMarkRecord {
                        language: language.to_owned(),
                        lemma: lemma.to_owned(),
                        status: s.to_owned(),
                        updated_at: now,
                    },
                );
            }
            None => {
                marks.remove(&key);
            }
        }
        Ok(())
    }
}
