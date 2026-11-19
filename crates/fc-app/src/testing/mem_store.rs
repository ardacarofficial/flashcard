//! In-memory [`Store`] implementing every `fc-storage` repository trait faithfully enough for
//! service tests (no business logic, like the real store). A write transaction works on a copy
//! of the state and swaps it in on commit; dropping it discards the copy (= rollback).

use fc_core::attribute::{
    effective_attribute_defs, AttributeDef, AttributeOption, AttributeTarget, AttributeValue,
    EntityRef, StoredValue,
};
use fc_core::card::{Card, CardState};
use fc_core::deck::{name_eq, Deck};
use fc_core::note::Note;
use fc_core::notetype::NoteType;
use fc_core::paging::{Page, PageRequest};
use fc_core::preset::Preset;
use fc_core::settings::SettingScope;
use fc_core::tag::{Tag, TagName};
use fc_core::{
    AttributeDefId, AttributeOptionId, BatchId, CardId, DeckId, JobId, NoteId, NoteTypeId,
    PresetId, TagId, Timestamp,
};
use fc_storage::repo::*;
use fc_storage::{ReadTx, StorageError, StorageResult, Store, WriteTx};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::{Mutex, MutexGuard};

/// Whole database content.
#[derive(Debug, Clone, Default)]
pub struct MemState {
    decks: BTreeMap<DeckId, Deck>,
    presets: BTreeMap<PresetId, Preset>,
    notetypes: BTreeMap<NoteTypeId, NoteType>,
    notes: BTreeMap<NoteId, Note>,
    search_text: BTreeMap<NoteId, String>,
    cards: BTreeMap<CardId, Card>,
    tags: BTreeMap<String, Tag>,
    attr_defs: BTreeMap<AttributeDefId, AttributeDef>,
    attr_values: Vec<(AttributeDefId, EntityRef, Vec<StoredValue>)>,
    oplog: Vec<OplogRecord>,
    settings: BTreeMap<(String, String), Value>,
    modules: BTreeMap<String, bool>,
    jobs: BTreeMap<JobId, JobRecord>,
}

/// In-memory store.
#[derive(Debug, Default)]
pub struct MemStore {
    state: Mutex<MemState>,
}

impl MemStore {
    /// Empty database.
    pub fn new() -> Self {
        Self::default()
    }

    fn lock(&self) -> StorageResult<MutexGuard<'_, MemState>> {
        self.state.lock().map_err(|_| StorageError::Busy)
    }

    /// Indexed text of a note (what the real store writes to `notes_fts`).
    pub fn search_text(&self, id: NoteId) -> Option<String> {
        self.lock().ok()?.search_text.get(&id).cloned()
    }
}

/// A transaction: a working copy of the state, plus the writer lock for write transactions.
/// (Read snapshots also accept writes, which are simply discarded.)
pub struct MemTx<'a> {
    guard: Option<MutexGuard<'a, MemState>>,
    work: MemState,
}

impl MemTx<'_> {
    fn s(&self) -> &MemState {
        &self.work
    }

    fn m(&mut self) -> &mut MemState {
        &mut self.work
    }
}

impl Store for MemStore {
    fn read(&self) -> StorageResult<Box<dyn ReadTx + '_>> {
        let work = self.lock()?.clone();
        Ok(Box::new(MemTx { guard: None, work }))
    }

    fn write(&self) -> StorageResult<Box<dyn WriteTx + '_>> {
        let guard = self.lock()?;
        let work = guard.clone();
        Ok(Box::new(MemTx {
            guard: Some(guard),
            work,
        }))
    }

    fn schema_version(&self) -> StorageResult<u32> {
        Ok(fc_storage::migrate::latest_version())
    }
}

impl ReadTx for MemTx<'_> {}

impl WriteTx for MemTx<'_> {
    fn commit(self: Box<Self>) -> StorageResult<()> {
        let MemTx { guard, work } = *self;
        if let Some(mut guard) = guard {
            *guard = work;
        }
        Ok(())
    }
}

fn not_found(entity: &'static str, id: impl ToString) -> StorageError {
    StorageError::NotFound {
        entity,
        id: id.to_string(),
    }
}

fn offset_page<T>(items: Vec<T>, page: &PageRequest) -> StorageResult<Page<T>> {
    let start: usize = match &page.cursor {
        None => 0,
        Some(c) => c
            .parse()
            .map_err(|_| StorageError::InvalidCursor(c.clone()))?,
    };
    let limit = page.effective_limit() as usize;
    let total = items.len();
    let items: Vec<T> = items.into_iter().skip(start).take(limit).collect();
    let end = start + items.len();
    Ok(Page {
        items,
        next_cursor: (end < total).then(|| end.to_string()),
    })
}

// ------------------------------------------------------------------ decks & presets

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

// ------------------------------------------------------------------ note types, notes, cards

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

// ------------------------------------------------------------------ tags

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

// ------------------------------------------------------------------ attributes

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

// ------------------------------------------------------------------ oplog

impl OplogRead for MemTx<'_> {
    fn list_oplog(&self, page: &PageRequest) -> StorageResult<Page<OplogRecord>> {
        let items: Vec<OplogRecord> = self.s().oplog.iter().rev().cloned().collect();
        offset_page(items, page)
    }

    fn list_oplog_batches(
        &self,
        before: Option<BatchId>,
        limit: u32,
    ) -> StorageResult<Vec<OplogBatch>> {
        let mut batches: Vec<OplogBatch> = Vec::new();
        for r in &self.s().oplog {
            if !batches.iter().any(|b| b.batch_id == r.batch_id) {
                batches.push(OplogBatch {
                    batch_id: r.batch_id,
                    kind: r.kind,
                    reverts_batch_id: r.reverts_batch_id,
                    summary: r.summary.clone(),
                    created_at: r.created_at,
                });
            }
        }
        batches.reverse();
        let start = match before {
            None => 0,
            Some(b) => batches
                .iter()
                .position(|x| x.batch_id == b)
                .map_or(batches.len(), |p| p + 1),
        };
        Ok(batches
            .into_iter()
            .skip(start)
            .take(limit as usize)
            .collect())
    }

    fn oplog_batch_entries(&self, batch: BatchId) -> StorageResult<Vec<OplogRecord>> {
        let mut v: Vec<OplogRecord> = self
            .s()
            .oplog
            .iter()
            .filter(|r| r.batch_id == batch)
            .cloned()
            .collect();
        v.sort_by_key(|r| r.seq);
        Ok(v)
    }
}

impl OplogWrite for MemTx<'_> {
    fn append_oplog(&mut self, record: &OplogRecord) -> StorageResult<()> {
        self.m().oplog.push(record.clone());
        Ok(())
    }
}

// ------------------------------------------------------------------ settings & modules

impl SettingsRead for MemTx<'_> {
    fn get_setting(&self, scope: &SettingScope, key: &str) -> StorageResult<Option<Value>> {
        Ok(self
            .s()
            .settings
            .get(&(scope.storage_key(), key.to_owned()))
            .cloned())
    }

    fn list_settings(&self, scope: &SettingScope) -> StorageResult<Vec<(String, Value)>> {
        let sk = scope.storage_key();
        Ok(self
            .s()
            .settings
            .iter()
            .filter(|((s, _), _)| *s == sk)
            .map(|((_, k), v)| (k.clone(), v.clone()))
            .collect())
    }
}

impl SettingsWrite for MemTx<'_> {
    fn set_setting(
        &mut self,
        scope: &SettingScope,
        key: &str,
        value: &Value,
        _now: Timestamp,
    ) -> StorageResult<()> {
        self.m()
            .settings
            .insert((scope.storage_key(), key.to_owned()), value.clone());
        Ok(())
    }

    fn delete_setting(&mut self, scope: &SettingScope, key: &str) -> StorageResult<bool> {
        Ok(self
            .m()
            .settings
            .remove(&(scope.storage_key(), key.to_owned()))
            .is_some())
    }
}

impl ModuleStateRead for MemTx<'_> {
    fn list_module_states(&self) -> StorageResult<Vec<(String, bool)>> {
        Ok(self
            .s()
            .modules
            .iter()
            .map(|(k, v)| (k.clone(), *v))
            .collect())
    }
}

impl ModuleStateWrite for MemTx<'_> {
    fn set_module_enabled(
        &mut self,
        id: &str,
        enabled: bool,
        _now: Timestamp,
    ) -> StorageResult<()> {
        self.m().modules.insert(id.to_owned(), enabled);
        Ok(())
    }
}

// ------------------------------------------------------------------ jobs

impl JobRead for MemTx<'_> {
    fn get_job(&self, id: JobId) -> StorageResult<Option<JobRecord>> {
        Ok(self.s().jobs.get(&id).cloned())
    }

    fn list_jobs(&self, limit: u32) -> StorageResult<Vec<JobRecord>> {
        let mut v: Vec<JobRecord> = self.s().jobs.values().cloned().collect();
        v.sort_by(|a, b| b.created_at.cmp(&a.created_at).then(b.id.cmp(&a.id)));
        v.truncate(limit as usize);
        Ok(v)
    }
}

impl JobWrite for MemTx<'_> {
    fn upsert_job(&mut self, job: &JobRecord) -> StorageResult<()> {
        self.m().jobs.insert(job.id, job.clone());
        Ok(())
    }

    fn mark_unfinished_jobs_interrupted(&mut self, now: Timestamp) -> StorageResult<u32> {
        let mut n = 0;
        for j in self.m().jobs.values_mut() {
            if j.status == "running" || j.status == "queued" {
                j.status = "interrupted".into();
                j.updated_at = now;
                n += 1;
            }
        }
        Ok(n)
    }

    fn claim_next_job(&mut self, now: Timestamp) -> StorageResult<Option<JobRecord>> {
        let next = self
            .s()
            .jobs
            .values()
            .filter(|j| j.status == "queued")
            .min_by(|a, b| a.created_at.cmp(&b.created_at).then(a.id.cmp(&b.id)))
            .map(|j| j.id);
        let Some(id) = next else {
            return Ok(None);
        };
        let job = self
            .m()
            .jobs
            .get_mut(&id)
            .ok_or_else(|| not_found("job", id))?;
        job.status = "running".into();
        job.updated_at = now;
        Ok(Some(job.clone()))
    }
}
