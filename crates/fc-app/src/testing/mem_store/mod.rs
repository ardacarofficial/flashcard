//! In-memory [`Store`] implementing every `fc-storage` repository trait faithfully enough for
//! service tests (no business logic, like the real store). A write transaction works on a copy
//! of the state and swaps it in on commit; dropping it discards the copy (= rollback).
//!
//! One file per domain with that domain's `impl <Domain>Read/Write for MemTx`. A new domain
//! adds `mem_store/<domain>.rs` and **one entry** in the [`domains!`] list at the bottom
//! (`study => StudyState` if it keeps its own tables: the file then defines
//! `#[derive(Debug, Clone, Default)] pub struct StudyState`, reachable as
//! `self.s().domains.study` / `self.m().domains.study`).

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
    unfinished_batches: BTreeMap<BatchId, UnfinishedBatchRecord>,
    unfinished_batch_parts: BTreeMap<(BatchId, u32), Value>,
    /// State of domains that keep their own tables (see [`domains!`]).
    domains: DomainState,
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

/// Declares the domain files; `domain => State` also adds `State` to [`DomainState`].
macro_rules! domains {
    ($($domain:ident $(=> $state:ident)?),* $(,)?) => {
        $( mod $domain; )*

        /// Per-domain state of domains with their own tables.
        #[derive(Debug, Clone, Default)]
        pub(crate) struct DomainState {
            $($( $domain: self::$domain::$state, )?)*
        }
    };
}

// One entry per domain file. Phase-2 tracks append theirs here.
domains! {
    decks,
    notes,
    tags,
    attributes,
    oplog,
    settings,
    jobs,
    study => StudyState,
    content => ContentState,
    interop,
    search => SearchState,
    mcp => McpState,
    stats => StatsState,
    ai => AiState,
    plugins => PluginState,
}
