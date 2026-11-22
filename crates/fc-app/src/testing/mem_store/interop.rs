//! [`MemTx`] repository traits: import/export support. Review log rows live in the study
//! domain's state, media in the content domain's; whole-collection dump/load is SQLite-only
//! (`StorageError::NotImplemented`).

use super::*;

impl InteropRead for MemTx<'_> {
    fn review_logs_for_cards(&self, cards: &[CardId]) -> StorageResult<Vec<ReviewLogRecord>> {
        let mut out: Vec<ReviewLogRecord> = self
            .s()
            .domains
            .study
            .reviews
            .values()
            .filter(|r| cards.contains(&r.card_id))
            .cloned()
            .collect();
        out.sort_by(|a, b| a.created_at.cmp(&b.created_at).then(a.id.cmp(&b.id)));
        Ok(out)
    }

    fn dump_collection(&self) -> StorageResult<CollectionDump> {
        Err(StorageError::NotImplemented("MemStore::dump_collection"))
    }
}

impl InteropWrite for MemTx<'_> {
    fn insert_review_logs(&mut self, rows: &[ReviewLogRecord]) -> StorageResult<u32> {
        let mut n = 0;
        for r in rows {
            if self.s().cards.contains_key(&r.card_id)
                && !self.s().domains.study.reviews.contains_key(&r.id)
            {
                self.m().domains.study.reviews.insert(r.id, r.clone());
                n += 1;
            }
        }
        Ok(n)
    }

    fn load_collection(&mut self, _dump: &CollectionDump) -> StorageResult<()> {
        Err(StorageError::NotImplemented("MemStore::load_collection"))
    }

    fn purge_rows(&mut self, rows: &PurgeRows) -> StorageResult<()> {
        let s = self.m();
        let mut cards: Vec<CardId> = rows.cards.clone();
        cards.extend(
            s.cards
                .values()
                .filter(|c| rows.notes.contains(&c.note_id))
                .map(|c| c.id),
        );
        let mut tag_names: Vec<String> = Vec::new();
        for id in &rows.notes {
            if let Some(n) = s.notes.remove(id) {
                tag_names.extend(n.tags.iter().map(|t| t.as_str().to_lowercase()));
            }
            s.search_text.remove(id);
            s.domains.search.forget_origin(id);
        }
        for id in &cards {
            s.cards.remove(id);
        }
        s.domains.study.reviews.retain(|_, r| !cards.contains(&r.card_id));
        s.attr_values.retain(|(attr, e, _)| {
            !rows.attribute_defs.contains(attr)
                && match e {
                    EntityRef::Note(n) => !rows.notes.contains(n),
                    EntityRef::Card(c) => !cards.contains(c),
                }
        });
        for id in &rows.attribute_defs {
            s.attr_defs.remove(id);
        }
        for id in &rows.decks {
            s.decks.remove(id);
        }
        for id in &rows.notetypes {
            s.notetypes.remove(id);
        }
        if let Some(since) = rows.tags_created_since {
            let used: std::collections::HashSet<String> = s
                .notes
                .values()
                .flat_map(|n| n.tags.iter().map(|t| t.as_str().to_lowercase()))
                .collect();
            s.tags.retain(|_, t| {
                let name = t.name.as_str().to_lowercase();
                !(tag_names.contains(&name) && t.created_at >= since && !used.contains(&name))
            });
        }
        Ok(())
    }
}
