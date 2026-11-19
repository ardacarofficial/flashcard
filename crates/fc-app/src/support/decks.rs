//! Deck index: every deck (trash included) by id, with paths, ancestry and subtrees.

use super::{deleted, err_id};
use crate::error::AppResult;
use fc_api::v1::decks::DeckDto;
use fc_api::ErrorCode;
use fc_core::deck::{deck_ancestry, Deck, PATH_SEPARATOR};
use fc_core::DeckId;
use fc_storage::Reads;
use std::collections::HashMap;

/// All decks, indexed.
pub(crate) struct DeckIndex {
    decks: HashMap<DeckId, Deck>,
}

impl DeckIndex {
    /// Loads every deck (live and trashed).
    pub fn load(reads: &dyn Reads) -> AppResult<Self> {
        Ok(Self {
            decks: reads
                .list_decks(true)?
                .into_iter()
                .map(|d| (d.id, d))
                .collect(),
        })
    }

    /// Deck by id.
    pub fn get(&self, id: DeckId) -> Option<&Deck> {
        self.decks.get(&id)
    }

    /// Deck by id or `NOT_FOUND_DECK`.
    pub fn require(&self, id: DeckId) -> AppResult<&Deck> {
        self.get(id)
            .ok_or_else(|| err_id(ErrorCode::NotFoundDeck, format!("deck {id} not found"), id))
    }

    /// Live deck by id (`NOT_FOUND_DECK` / `CONFLICT_DELETED`).
    pub fn require_live(&self, id: DeckId) -> AppResult<&Deck> {
        let d = self.require(id)?;
        if d.is_deleted() {
            return Err(deleted("deck", id));
        }
        Ok(d)
    }

    /// Root-first ancestry, the deck itself last.
    pub fn ancestry(&self, id: DeckId) -> AppResult<Vec<DeckId>> {
        Ok(deck_ancestry(id, |d| {
            self.decks.get(&d).map(|d| d.parent_id)
        })?)
    }

    /// Full path `A::B::C`.
    pub fn path(&self, id: DeckId) -> String {
        self.ancestry(id)
            .unwrap_or_else(|_| vec![id])
            .iter()
            .filter_map(|d| self.decks.get(d).map(|d| d.name.as_str()))
            .collect::<Vec<_>>()
            .join(PATH_SEPARATOR)
    }

    /// Every deck below `id` (any depth, trash included), parents before children.
    pub fn descendants(&self, id: DeckId) -> Vec<DeckId> {
        let mut out = Vec::new();
        let mut frontier = vec![id];
        while let Some(cur) = frontier.pop() {
            let mut kids: Vec<DeckId> = self
                .decks
                .values()
                .filter(|d| d.parent_id == Some(cur) && d.id != id)
                .map(|d| d.id)
                .collect();
            kids.sort();
            for k in kids {
                if !out.contains(&k) {
                    out.push(k);
                    frontier.push(k);
                }
            }
        }
        out
    }

    /// `id` plus its live descendants.
    pub fn live_subtree(&self, id: DeckId) -> Vec<DeckId> {
        let mut v = vec![id];
        v.extend(
            self.descendants(id)
                .into_iter()
                .filter(|d| self.get(*d).is_some_and(|d| !d.is_deleted())),
        );
        v
    }

    /// Live children of `parent`, sorted by position then name.
    pub fn children(&self, parent: Option<DeckId>) -> Vec<&Deck> {
        let mut v: Vec<&Deck> = self
            .decks
            .values()
            .filter(|d| d.parent_id == parent && !d.is_deleted())
            .collect();
        v.sort_by(|a, b| {
            a.position
                .cmp(&b.position)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                .then(a.id.cmp(&b.id))
        });
        v
    }

    /// Next free position among live children of `parent` (ignoring `except`).
    pub fn next_position(&self, parent: Option<DeckId>, except: Option<DeckId>) -> i64 {
        self.children(parent)
            .iter()
            .filter(|d| Some(d.id) != except)
            .map(|d| d.position + 1)
            .max()
            .unwrap_or(0)
    }

    /// DTO of a deck.
    pub fn dto(&self, d: &Deck) -> DeckDto {
        DeckDto {
            id: d.id,
            parent_id: d.parent_id,
            name: d.name.clone(),
            path: self.path(d.id),
            description: d.description.clone(),
            icon: d.icon.clone(),
            color: d.color.clone(),
            preset_id: d.preset_id,
            position: d.position,
            created_at: d.created_at,
            updated_at: d.updated_at,
        }
    }

    /// Live decks.
    pub fn live(&self) -> impl Iterator<Item = &Deck> {
        self.decks.values().filter(|d| !d.is_deleted())
    }
}
