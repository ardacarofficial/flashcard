//! [`MemTx`] repository traits: Note types, notes and cards.

use super::*;

impl NoteTypeRead for MemTx<'_> {
    fn get_notetype(&self, id: NoteTypeId) -> StorageResult<Option<NoteType>> {
        Ok(self.s().notetypes.get(&id).cloned())
    }

    fn list_notetypes(&self, include_deleted: bool) -> StorageResult<Vec<NoteType>> {
        Ok(self
            .s()
            .notetypes
            .values()
            .filter(|n| include_deleted || n.deleted_at.is_none())
            .cloned()
            .collect())
    }
}

impl NoteTypeWrite for MemTx<'_> {
    fn upsert_notetype(&mut self, nt: &NoteType) -> StorageResult<()> {
        self.m().notetypes.insert(nt.id, nt.clone());
        Ok(())
    }
}

impl NoteRead for MemTx<'_> {
    fn get_note(&self, id: NoteId) -> StorageResult<Option<Note>> {
        Ok(self.s().notes.get(&id).cloned())
    }

    fn list_notes(
        &self,
        filter: &NoteFilter,
        page: &PageRequest,
    ) -> StorageResult<Page<NoteListRow>> {
        let s = self.s();
        let mut rows: Vec<NoteListRow> = s
            .notes
            .values()
            .filter(|n| filter.include_deleted || n.deleted_at.is_none())
            .filter_map(|n| {
                let live: Vec<&Card> = s
                    .cards
                    .values()
                    .filter(|c| c.note_id == n.id && c.deleted_at.is_none())
                    .collect();
                if let Some(decks) = &filter.deck_ids {
                    if !live.iter().any(|c| decks.contains(&c.deck_id)) {
                        return None;
                    }
                }
                Some(NoteListRow {
                    note: n.clone(),
                    card_count: u32::try_from(live.len()).unwrap_or(u32::MAX),
                })
            })
            .collect();
        rows.sort_by(|a, b| {
            b.note
                .updated_at
                .cmp(&a.note.updated_at)
                .then(b.note.id.cmp(&a.note.id))
        });
        offset_page(rows, page)
    }

    fn find_notes_by_checksum(
        &self,
        notetype: NoteTypeId,
        checksum: &str,
    ) -> StorageResult<Vec<NoteId>> {
        Ok(self
            .s()
            .notes
            .values()
            .filter(|n| {
                n.deleted_at.is_none() && n.notetype_id == notetype && n.checksum == checksum
            })
            .map(|n| n.id)
            .collect())
    }

    fn note_ids_with_tag(&self, tag: &TagName) -> StorageResult<Vec<NoteId>> {
        Ok(self
            .s()
            .notes
            .values()
            .filter(|n| n.deleted_at.is_none() && n.tags.iter().any(|t| t.is_within(tag)))
            .map(|n| n.id)
            .collect())
    }

    fn note_home_deck(&self, id: NoteId) -> StorageResult<Option<DeckId>> {
        Ok(self.s().notes.get(&id).map(|n| n.home_deck_id))
    }
}

/// Like the real store's foreign key: the home deck must exist.
fn check_home(s: &MemState, note: &Note) -> StorageResult<()> {
    if s.decks.contains_key(&note.home_deck_id) {
        Ok(())
    } else {
        Err(StorageError::Constraint {
            constraint: "foreign_key".into(),
        })
    }
}

/// Tag rows are the app's job (it mints `TagId`s); like the real store, a note may only
/// reference existing tags.
fn check_tags(s: &MemState, note: &Note) -> StorageResult<()> {
    match note.tags.iter().find(|t| !s.tags.contains_key(&t.key())) {
        Some(t) => Err(not_found("tag", t.as_str())),
        None => Ok(()),
    }
}

impl NoteWrite for MemTx<'_> {
    fn insert_note(&mut self, note: &Note, search_text: &str) -> StorageResult<()> {
        if self.s().notes.contains_key(&note.id) {
            return Err(StorageError::Constraint {
                constraint: "notes_pk".into(),
            });
        }
        check_tags(self.s(), note)?;
        check_home(self.s(), note)?;
        let s = self.m();
        s.notes.insert(note.id, note.clone());
        s.search_text.insert(note.id, search_text.to_owned());
        Ok(())
    }

    fn update_note(&mut self, note: &Note, search_text: &str) -> StorageResult<()> {
        if !self.s().notes.contains_key(&note.id) {
            return Err(not_found("note", note.id));
        }
        check_tags(self.s(), note)?;
        check_home(self.s(), note)?;
        let s = self.m();
        s.notes.insert(note.id, note.clone());
        s.search_text.insert(note.id, search_text.to_owned());
        Ok(())
    }

    fn set_notes_deleted(
        &mut self,
        ids: &[NoteId],
        deleted_at: Option<Timestamp>,
    ) -> StorageResult<u32> {
        let mut n = 0;
        for id in ids {
            if let Some(note) = self.m().notes.get_mut(id) {
                if note.deleted_at != deleted_at {
                    note.deleted_at = deleted_at;
                    n += 1;
                }
            }
        }
        Ok(n)
    }

    fn set_note_home_deck(&mut self, id: NoteId, home_deck: DeckId) -> StorageResult<()> {
        let note = self
            .m()
            .notes
            .get_mut(&id)
            .ok_or_else(|| not_found("note", id))?;
        note.home_deck_id = home_deck;
        Ok(())
    }
}

impl CardRead for MemTx<'_> {
    fn get_card(&self, id: CardId) -> StorageResult<Option<Card>> {
        Ok(self.s().cards.get(&id).cloned())
    }

    fn list_cards(&self, filter: &CardFilter) -> StorageResult<Vec<Card>> {
        Ok(self
            .s()
            .cards
            .values()
            .filter(|c| filter.include_deleted || c.deleted_at.is_none())
            .filter(|c| {
                filter
                    .deck_ids
                    .as_ref()
                    .is_none_or(|d| d.contains(&c.deck_id))
            })
            .filter(|c| {
                filter
                    .note_ids
                    .as_ref()
                    .is_none_or(|n| n.contains(&c.note_id))
            })
            .cloned()
            .collect())
    }
}

impl CardWrite for MemTx<'_> {
    fn insert_card(&mut self, card: &Card) -> StorageResult<()> {
        if self.s().cards.contains_key(&card.id) {
            return Err(StorageError::Constraint {
                constraint: "cards_pk".into(),
            });
        }
        self.m().cards.insert(card.id, card.clone());
        Ok(())
    }

    fn update_card(&mut self, card: &Card) -> StorageResult<()> {
        let slot = self
            .m()
            .cards
            .get_mut(&card.id)
            .ok_or_else(|| not_found("card", card.id))?;
        *slot = card.clone();
        Ok(())
    }

    fn set_cards_deleted(
        &mut self,
        ids: &[CardId],
        deleted_at: Option<Timestamp>,
    ) -> StorageResult<u32> {
        let mut n = 0;
        for id in ids {
            if let Some(c) = self.m().cards.get_mut(id) {
                if c.deleted_at != deleted_at {
                    c.deleted_at = deleted_at;
                    n += 1;
                }
            }
        }
        Ok(n)
    }
}
