//! [`MemTx`] repository traits: search (via the reference evaluator
//! `fc_core::query::eval`), saved queries and maintenance reports.

use super::*;
use fc_core::query::eval::{matches, AttrFact, CardFacts};
use fc_core::query::{Filter, SavedQuery};
use fc_core::SavedQueryId;
use std::cmp::Ordering;

/// Saved queries.
#[derive(Debug, Clone, Default)]
pub struct SearchState {
    saved: BTreeMap<SavedQueryId, SavedQuery>,
}

impl MemTx<'_> {
    /// Live (card, note) pairs matching `filter`, in creation order.
    fn matching(&self, filter: &Filter, now: Timestamp) -> Vec<(&Card, &Note)> {
        let s = self.s();
        let mut out: Vec<(&Card, &Note)> = Vec::new();
        for card in s.cards.values().filter(|c| c.deleted_at.is_none()) {
            let Some(note) = s.notes.get(&card.note_id).filter(|n| n.deleted_at.is_none()) else {
                continue;
            };
            let tag_ids: Vec<TagId> = note
                .tags
                .iter()
                .filter_map(|t| s.tags.get(&t.key()).map(|t| t.id))
                .collect();
            let attrs: Vec<AttrFact<'_>> = s
                .attr_values
                .iter()
                .filter(|(_, e, _)| *e == EntityRef::Note(note.id) || *e == EntityRef::Card(card.id))
                .flat_map(|(attr_id, e, rows)| {
                    rows.iter().map(move |r| AttrFact {
                        attr_id: *attr_id,
                        target: e.target(),
                        value: &r.value,
                        value_num: r.value_num,
                    })
                })
                .collect();
            let text = s.search_text.get(&note.id).map_or("", String::as_str);
            let facts = CardFacts {
                card,
                note,
                tag_ids: &tag_ids,
                attrs: &attrs,
                search_text: text,
            };
            if matches(filter, &facts, now) {
                out.push((card, note));
            }
        }
        out.sort_by_key(|(c, _)| (c.created_at, c.id));
        out
    }
}

fn cmp_opt(a: Option<f64>, b: Option<f64>) -> Ordering {
    match (a, b) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(Ordering::Equal),
    }
}

impl SearchRead for MemTx<'_> {
    fn search_cards(&self, q: &CardSearch) -> StorageResult<SearchPage> {
        let mut rows = self.matching(&q.filter, q.now);
        if q.notes_only {
            let mut seen: Vec<NoteId> = Vec::new();
            let mut by_id = rows.clone();
            by_id.sort_by_key(|(c, _)| c.id);
            let firsts: Vec<CardId> = by_id
                .iter()
                .filter(|(c, _)| {
                    if seen.contains(&c.note_id) {
                        false
                    } else {
                        seen.push(c.note_id);
                        true
                    }
                })
                .map(|(c, _)| c.id)
                .collect();
            rows.retain(|(c, _)| firsts.contains(&c.id));
        }
        rows.sort_by(|(a, an), (b, bn)| {
            let s = (&a.scheduling, &b.scheduling);
            let o = match q.sort {
                CardSort::SortField => an.sort_field.to_ascii_lowercase().cmp(&bn.sort_field.to_ascii_lowercase()),
                CardSort::Created => a.created_at.cmp(&b.created_at),
                CardSort::Modified => an.updated_at.cmp(&bn.updated_at),
                CardSort::Due => s.0.due.cmp(&s.1.due),
                CardSort::Stability => cmp_opt(s.0.stability, s.1.stability),
                CardSort::Difficulty => cmp_opt(s.0.difficulty, s.1.difficulty),
                CardSort::Lapses => s.0.lapses.cmp(&s.1.lapses),
                CardSort::Reps => s.0.reps.cmp(&s.1.reps),
            }
            .then(a.id.cmp(&b.id));
            if q.descending {
                o.reverse()
            } else {
                o
            }
        });
        let total = u32::try_from(rows.len()).unwrap_or(u32::MAX);
        let hits = rows
            .into_iter()
            .skip(q.offset as usize)
            .take(q.limit as usize)
            .map(|(card, note)| CardHit {
                card: card.clone(),
                notetype_id: note.notetype_id,
                sort_field: note.sort_field.clone(),
                tags: note.tags.clone(),
                note_updated_at: note.updated_at,
            })
            .collect();
        Ok(SearchPage { hits, total })
    }

    fn search_card_ids(&self, filter: &Filter, now: Timestamp) -> StorageResult<Vec<CardId>> {
        Ok(self.matching(filter, now).into_iter().map(|(c, _)| c.id).collect())
    }

    fn list_saved_queries(&self) -> StorageResult<Vec<SavedQuery>> {
        let mut v: Vec<SavedQuery> = self.s().domains.search.saved.values().cloned().collect();
        v.sort_by(|a, b| {
            a.position
                .cmp(&b.position)
                .then_with(|| a.name.cmp(&b.name))
                .then_with(|| a.id.cmp(&b.id))
        });
        Ok(v)
    }

    fn get_saved_query(&self, id: SavedQueryId) -> StorageResult<Option<SavedQuery>> {
        Ok(self.s().domains.search.saved.get(&id).cloned())
    }

    fn duplicate_note_groups(&self) -> StorageResult<Vec<Vec<NoteId>>> {
        let mut groups: BTreeMap<(String, String), Vec<NoteId>> = BTreeMap::new();
        for n in self.s().notes.values().filter(|n| n.deleted_at.is_none()) {
            groups
                .entry((n.notetype_id.to_string(), n.checksum.clone()))
                .or_default()
                .push(n.id);
        }
        Ok(groups
            .into_values()
            .filter(|g| g.len() > 1)
            .map(|mut g| {
                g.sort();
                g
            })
            .collect())
    }

    fn list_trashed_notes(&self, limit: u32) -> StorageResult<Vec<Note>> {
        let mut v: Vec<Note> = self
            .s()
            .notes
            .values()
            .filter(|n| n.deleted_at.is_some())
            .cloned()
            .collect();
        v.sort_by(|a, b| b.deleted_at.cmp(&a.deleted_at).then(a.id.cmp(&b.id)));
        v.truncate(limit as usize);
        Ok(v)
    }

    fn card_reviews(&self, _card: CardId) -> StorageResult<Vec<ReviewEntry>> {
        // The in-memory store keeps no review log (v0.2 adds one).
        Ok(Vec::new())
    }

    fn integrity_report(&self) -> StorageResult<IntegrityReport> {
        let s = self.s();
        let live_cards = || s.cards.values().filter(|c| c.deleted_at.is_none());
        let live_notes = || s.notes.values().filter(|n| n.deleted_at.is_none());
        Ok(IntegrityReport {
            orphan_cards: live_cards()
                .filter(|c| s.notes.get(&c.note_id).is_none_or(|n| n.deleted_at.is_some()))
                .map(|c| c.id)
                .collect(),
            cards_in_trashed_decks: live_cards()
                .filter(|c| s.decks.get(&c.deck_id).is_some_and(|d| d.deleted_at.is_some()))
                .map(|c| c.id)
                .collect(),
            unscheduled_cards: live_cards()
                .filter(|c| c.scheduling.state != CardState::New && c.scheduling.due.is_none())
                .map(|c| c.id)
                .collect(),
            missing_search_index: live_notes()
                .filter(|n| !s.search_text.contains_key(&n.id))
                .map(|n| n.id)
                .collect(),
            cardless_notes: live_notes()
                .filter(|n| !live_cards().any(|c| c.note_id == n.id))
                .map(|n| n.id)
                .collect(),
            dangling_values: u32::try_from(
                s.attr_values
                    .iter()
                    .filter(|(_, e, _)| match e {
                        EntityRef::Note(n) => !s.notes.contains_key(n),
                        EntityRef::Card(c) => !s.cards.contains_key(c),
                    })
                    .map(|(_, _, rows)| rows.len())
                    .sum::<usize>(),
            )
            .unwrap_or(u32::MAX),
        })
    }
}

impl SearchWrite for MemTx<'_> {
    fn upsert_saved_query(&mut self, q: &SavedQuery) -> StorageResult<()> {
        self.m().domains.search.saved.insert(q.id, q.clone());
        Ok(())
    }

    fn delete_saved_query(&mut self, id: SavedQueryId) -> StorageResult<bool> {
        Ok(self.m().domains.search.saved.remove(&id).is_some())
    }
}
