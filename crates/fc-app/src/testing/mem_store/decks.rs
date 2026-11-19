//! [`MemTx`] repository traits: Decks and presets.

use super::*;

impl DeckRead for MemTx<'_> {
    fn get_deck(&self, id: DeckId) -> StorageResult<Option<Deck>> {
        Ok(self.s().decks.get(&id).cloned())
    }

    fn list_decks(&self, include_deleted: bool) -> StorageResult<Vec<Deck>> {
        Ok(self
            .s()
            .decks
            .values()
            .filter(|d| include_deleted || d.deleted_at.is_none())
            .cloned()
            .collect())
    }

    fn find_child_deck(&self, parent: Option<DeckId>, name: &str) -> StorageResult<Option<Deck>> {
        Ok(self
            .s()
            .decks
            .values()
            .find(|d| d.deleted_at.is_none() && d.parent_id == parent && name_eq(&d.name, name))
            .cloned())
    }

    fn card_counts_by_deck(&self, now: Timestamp) -> StorageResult<Vec<(DeckId, CardCounts)>> {
        let mut out: BTreeMap<DeckId, CardCounts> = BTreeMap::new();
        for c in self.s().cards.values().filter(|c| c.deleted_at.is_none()) {
            let e = out.entry(c.deck_id).or_default();
            e.total += 1;
            let available = !c.suspended && c.buried_until.is_none_or(|b| b <= now);
            let due = c.scheduling.due.is_some_and(|d| d <= now);
            match c.scheduling.state {
                CardState::New if available => e.new += 1,
                CardState::Learning | CardState::Relearning if available && due => e.learning += 1,
                CardState::Review if available && due => e.review_due += 1,
                _ => {}
            }
        }
        Ok(out.into_iter().collect())
    }

    fn deck_ancestry(&self, id: DeckId) -> StorageResult<Vec<DeckId>> {
        let decks = &self.s().decks;
        let mut out = vec![id];
        let mut cur = decks.get(&id).ok_or_else(|| not_found("deck", id))?;
        while let Some(p) = cur.parent_id {
            if out.contains(&p) {
                return Err(StorageError::Corrupt {
                    table: "decks",
                    message: format!("parent cycle at {p}"),
                });
            }
            out.push(p);
            cur = decks.get(&p).ok_or_else(|| not_found("deck", p))?;
        }
        out.reverse();
        Ok(out)
    }
}

impl MemState {
    /// `root` and the decks reachable through children matching `keep`.
    fn subtree(&self, root: DeckId, keep: impl Fn(&Deck) -> bool) -> Vec<DeckId> {
        let mut out = vec![root];
        let mut i = 0;
        while i < out.len() {
            let parent = out[i];
            out.extend(
                self.decks
                    .values()
                    .filter(|d| d.parent_id == Some(parent) && keep(d))
                    .map(|d| d.id),
            );
            i += 1;
        }
        out.sort();
        out
    }

    fn apply_trash_set(&mut self, set: &DeckTrashSet, deleted_at: Option<Timestamp>) -> u32 {
        // Trash touches live rows, restore touches trashed rows.
        let applies = |current: Option<Timestamp>| current.is_none() == deleted_at.is_some();
        let mut n = 0;
        for id in &set.decks {
            if let Some(d) = self.decks.get_mut(id).filter(|d| applies(d.deleted_at)) {
                d.deleted_at = deleted_at;
                n += 1;
            }
        }
        for id in &set.cards {
            if let Some(c) = self.cards.get_mut(id).filter(|c| applies(c.deleted_at)) {
                c.deleted_at = deleted_at;
                n += 1;
            }
        }
        for id in &set.notes {
            if let Some(x) = self.notes.get_mut(id).filter(|x| applies(x.deleted_at)) {
                x.deleted_at = deleted_at;
                n += 1;
            }
        }
        n
    }
}

impl DeckWrite for MemTx<'_> {
    fn insert_deck(&mut self, deck: &Deck) -> StorageResult<()> {
        if self.s().decks.contains_key(&deck.id) {
            return Err(StorageError::Constraint {
                constraint: "decks_pk".into(),
            });
        }
        self.m().decks.insert(deck.id, deck.clone());
        Ok(())
    }

    fn update_deck(&mut self, deck: &Deck) -> StorageResult<()> {
        let slot = self
            .m()
            .decks
            .get_mut(&deck.id)
            .ok_or_else(|| not_found("deck", deck.id))?;
        *slot = deck.clone();
        Ok(())
    }

    fn set_decks_deleted(
        &mut self,
        ids: &[DeckId],
        deleted_at: Option<Timestamp>,
    ) -> StorageResult<u32> {
        let mut n = 0;
        for id in ids {
            if let Some(d) = self.m().decks.get_mut(id) {
                if d.deleted_at != deleted_at {
                    d.deleted_at = deleted_at;
                    n += 1;
                }
            }
        }
        Ok(n)
    }

    fn trash_deck_subtree(&mut self, deck: DeckId, at: Timestamp) -> StorageResult<DeckTrashSet> {
        let s = self.s();
        let root = s.decks.get(&deck).ok_or_else(|| not_found("deck", deck))?;
        if root.deleted_at.is_some() {
            return Ok(DeckTrashSet::default());
        }
        let decks = s.subtree(deck, |d| d.deleted_at.is_none());
        let cards: Vec<CardId> = s
            .cards
            .values()
            .filter(|c| c.deleted_at.is_none() && decks.contains(&c.deck_id))
            .map(|c| c.id)
            .collect();
        let notes: Vec<NoteId> = s
            .notes
            .values()
            .filter(|n| n.deleted_at.is_none())
            .filter(|n| {
                let live: Vec<&Card> = s
                    .cards
                    .values()
                    .filter(|c| c.note_id == n.id && c.deleted_at.is_none())
                    .collect();
                if live.is_empty() {
                    decks.contains(&n.home_deck_id)
                } else {
                    live.iter().all(|c| cards.contains(&c.id))
                }
            })
            .map(|n| n.id)
            .collect();
        let set = DeckTrashSet {
            decks,
            cards,
            notes,
        };
        self.m().apply_trash_set(&set, Some(at));
        Ok(set)
    }

    fn restore_deck_subtree(&mut self, deck: DeckId) -> StorageResult<DeckTrashSet> {
        let s = self.s();
        let root = s.decks.get(&deck).ok_or_else(|| not_found("deck", deck))?;
        let Some(at) = root.deleted_at else {
            return Ok(DeckTrashSet::default());
        };
        let decks = s.subtree(deck, |d| d.deleted_at == Some(at));
        let cards: Vec<CardId> = s
            .cards
            .values()
            .filter(|c| c.deleted_at == Some(at) && decks.contains(&c.deck_id))
            .map(|c| c.id)
            .collect();
        let notes: Vec<NoteId> = s
            .notes
            .values()
            .filter(|n| n.deleted_at == Some(at))
            .filter(|n| {
                decks.contains(&n.home_deck_id)
                    || s.cards
                        .values()
                        .any(|c| c.note_id == n.id && cards.contains(&c.id))
            })
            .map(|n| n.id)
            .collect();
        let set = DeckTrashSet {
            decks,
            cards,
            notes,
        };
        self.m().apply_trash_set(&set, None);
        Ok(set)
    }

    fn set_trash_set_deleted(
        &mut self,
        set: &DeckTrashSet,
        deleted_at: Option<Timestamp>,
    ) -> StorageResult<u32> {
        Ok(self.m().apply_trash_set(set, deleted_at))
    }
}

impl PresetRead for MemTx<'_> {
    fn get_preset(&self, id: PresetId) -> StorageResult<Option<Preset>> {
        Ok(self.s().presets.get(&id).cloned())
    }

    fn list_presets(&self) -> StorageResult<Vec<Preset>> {
        Ok(self.s().presets.values().cloned().collect())
    }
}

impl PresetWrite for MemTx<'_> {
    fn upsert_preset(&mut self, preset: &Preset) -> StorageResult<()> {
        self.m().presets.insert(preset.id, preset.clone());
        Ok(())
    }
}
