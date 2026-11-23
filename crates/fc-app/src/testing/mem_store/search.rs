//! [`MemTx`] repository traits: search (via the reference evaluator
//! `fc_core::query::eval`), saved queries and maintenance reports.

use super::*;
use fc_core::query::eval::{matches, AttrFact, CardFacts};
use fc_core::query::{Filter, LangFacts, NoteOrigin, SavedQuery};
use fc_core::SavedQueryId;
use std::cmp::Ordering;

/// Saved queries.
#[derive(Debug, Clone, Default)]
pub struct SearchState {
    saved: BTreeMap<SavedQueryId, SavedQuery>,
    /// v1.4 `note_origins`.
    origins: BTreeMap<NoteId, NoteOriginRecord>,
}

impl SearchState {
    /// A note deleted for good takes its origin with it (SQLite: trigger
    /// `note_origins_note_del`).
    pub(super) fn forget_origin(&mut self, note: &NoteId) {
        self.origins.remove(note);
    }
}

impl MemTx<'_> {
    /// What the language predicates see of a note (word index, origin, the word-data pack's
    /// rank and band of its key; media files are not modelled here: no media).
    fn lang_facts(&self, note: &Note) -> LangFacts {
        let s = self.s();
        let mut f = LangFacts::default();
        if let Some(n) = s.domains.words.notes.get(&note.id) {
            f.language = n.language.clone();
            f.key = Some(n.key.clone());
            if let Some(d) = s.domains.word_data.row(&n.language, &n.key) {
                f.rank = d.rank;
                f.band = d
                    .band
                    .as_deref()
                    .and_then(fc_core::query::lang::cefr_ordinal);
            }
            if !n.language.is_empty() {
                // Encounters beyond one per note, over the word's notes (the SQL view).
                let caps = &s.domains.subtitle.captures;
                f.extra_encounters = s
                    .domains
                    .words
                    .notes
                    .iter()
                    .filter(|(_, x)| x.language == n.language && x.key == n.key)
                    .map(|(id, _)| {
                        let c = caps.values().filter(|c| c.note_id == *id).count();
                        u32::try_from(c.saturating_sub(1)).unwrap_or(u32::MAX)
                    })
                    .sum();
                f.word = s
                    .domains
                    .words
                    .words
                    .get(&(n.language.clone(), n.key.clone()))
                    .map(|w| fc_core::words::WordEvidence {
                        mark: w
                            .mark
                            .as_deref()
                            .and_then(fc_core::reading::WordMark::parse)
                            .map(|m| (m, w.mark_at.unwrap_or(Timestamp(0)))),
                        notes: w.notes,
                        cards: w.cards,
                        max_interval_days: w.max_interval_days,
                        last_review: w.last_review,
                        first_seen: w.first_seen,
                    });
            }
        }
        if let Some(o) = s.domains.search.origins.get(&note.id) {
            f.origin = o.origin;
            f.source = [&o.source, &o.site, &o.video_id]
                .into_iter()
                .filter(|x| !x.is_empty())
                .cloned()
                .collect();
        }
        // v1.4 W4: the stored media files the note references, with their origin.
        let mut seen = std::collections::BTreeSet::new();
        for v in note.fields.values() {
            for h in fc_core::content::refs::extract_media(v) {
                if !seen.insert(h.clone()) {
                    continue;
                }
                let Ok(Some(m)) = self.get_media(&h) else {
                    continue;
                };
                let kind = match fc_core::content::media::media_kind(&m.mime) {
                    Some(fc_core::content::media::MediaKind::Audio) => {
                        fc_core::query::lang::MediaKind::Audio
                    }
                    Some(fc_core::content::media::MediaKind::Image) => {
                        fc_core::query::lang::MediaKind::Image
                    }
                    _ => continue,
                };
                let origin = self
                    .media_origins_of(std::slice::from_ref(&h))
                    .ok()
                    .and_then(|r| r.first().map(|r| r.origin))
                    .unwrap_or(
                        if fc_core::query::lang::is_video_media(m.original_name.as_deref()) {
                            fc_core::content::media::MediaOrigin::Original
                        } else {
                            fc_core::content::media::MediaOrigin::User
                        },
                    );
                f.media.push((kind, origin));
            }
        }
        f
    }

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
            let lang = self.lang_facts(note);
            let facts = CardFacts {
                card,
                note,
                tag_ids: &tag_ids,
                attrs: &attrs,
                search_text: text,
                lang: &lang,
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

    fn search_card_ids_among(
        &self,
        filter: &Filter,
        now: Timestamp,
        among: &[CardId],
    ) -> StorageResult<Vec<CardId>> {
        let among: std::collections::HashSet<CardId> = among.iter().copied().collect();
        Ok(self
            .matching(filter, now)
            .into_iter()
            .map(|(c, _)| c.id)
            .filter(|id| among.contains(id))
            .collect())
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

    fn note_origins_of(&self, notes: &[NoteId]) -> StorageResult<Vec<NoteOriginRecord>> {
        let o = &self.s().domains.search.origins;
        let mut ids = notes.to_vec();
        ids.sort();
        ids.dedup();
        Ok(ids.iter().filter_map(|n| o.get(n).cloned()).collect())
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
            broken_fields: live_notes()
                .filter(|n| {
                    s.notetypes.get(&n.notetype_id).is_some_and(|nt| {
                        nt.fields.iter().any(|f| !n.fields.contains_key(&f.key))
                    })
                })
                .map(|n| n.id)
                .collect(),
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

    fn set_note_origin(&mut self, o: &NoteOriginRecord) -> StorageResult<()> {
        let origins = &mut self.m().domains.search.origins;
        if o.origin == NoteOrigin::Manual {
            origins.remove(&o.note_id);
        } else {
            origins.insert(o.note_id, o.clone());
        }
        Ok(())
    }

    fn purge_trash(&mut self, before: Timestamp) -> StorageResult<PurgeCounts> {
        let old = |d: Option<Timestamp>| d.is_some_and(|d| d < before);
        let s = self.m();
        let gone: Vec<NoteId> = s
            .notes
            .values()
            .filter(|n| old(n.deleted_at))
            .map(|n| n.id)
            .collect();
        for id in &gone {
            s.notes.remove(id);
            s.search_text.remove(id);
            s.domains.search.forget_origin(id);
        }
        // Cards of purged notes cascade (not counted), then cards trashed on their own.
        s.cards.retain(|_, c| !gone.contains(&c.note_id));
        let before_cards = s.cards.len();
        s.cards.retain(|_, c| !old(c.deleted_at));
        let card_count = before_cards - s.cards.len();
        let mut decks = 0u32;
        loop {
            let free: Vec<DeckId> = s
                .decks
                .values()
                .filter(|d| {
                    old(d.deleted_at)
                        && !s.decks.values().any(|c| c.parent_id == Some(d.id))
                        && !s.cards.values().any(|c| c.deck_id == d.id)
                        && !s.notes.values().any(|n| n.home_deck_id == d.id)
                        && !s.attr_defs.values().any(|a| a.scope_deck_id == Some(d.id))
                })
                .map(|d| d.id)
                .collect();
            if free.is_empty() {
                break;
            }
            for id in &free {
                s.decks.remove(id);
            }
            decks += u32::try_from(free.len()).unwrap_or(u32::MAX);
        }
        self.delete_dangling_values()?;
        Ok(PurgeCounts {
            notes: u32::try_from(gone.len()).unwrap_or(u32::MAX),
            cards: u32::try_from(card_count).unwrap_or(u32::MAX),
            decks,
        })
    }

    fn delete_dangling_values(&mut self) -> StorageResult<u32> {
        let s = self.m();
        let (notes, cards) = (&s.notes, &s.cards);
        let mut removed = 0usize;
        s.attr_values.retain(|(_, e, rows)| {
            let live = match e {
                EntityRef::Note(n) => notes.contains_key(n),
                EntityRef::Card(c) => cards.contains_key(c),
            };
            if !live {
                removed += rows.len();
            }
            live
        });
        Ok(u32::try_from(removed).unwrap_or(u32::MAX))
    }
}
