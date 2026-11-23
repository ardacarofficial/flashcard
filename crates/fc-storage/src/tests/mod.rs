//! Storage tests. Fixtures live here; each area has its own file.
//!
//! Most tests run on a temp-file database (the production configuration: WAL + reader
//! pool); a few use the in-memory store. Raw `rusqlite` connections are used only to
//! inspect the schema and the FTS index.

mod ai;
mod attributes;
mod automation;
mod capture_log;
mod content;
mod decks;
mod encounters;
mod inbox;
mod interop;
mod leech_repairs;
mod maintenance;
mod mcp;
mod meanings;
mod media_origins;
mod migrations;
mod misc;
mod notes;
mod plugins;
mod reading;
mod search;
mod search_fuzz;
mod stats;
mod store;
mod study;
mod subtitle;
mod tags;
mod typed_answers;
mod ux;
mod vocab_roles;
mod word_data;
mod word_index;

use crate::error::StorageResult;
use crate::repo::Writes;
use crate::store::{SqliteStore, Store};
use fc_core::card::{standard_card_specs, Card};
use fc_core::deck::Deck;
use fc_core::note::{fnv1a64, note_checksum, searchable_text, sort_field_text, FieldValues, Note};
use fc_core::notetype::{builtin, FieldKey, NoteType};
use fc_core::tag::TagName;
use fc_core::{CardId, DeckId, NoteId, TagId, Timestamp};
use rusqlite::Connection;
use tempfile::TempDir;
use uuid::Uuid;

/// A file-backed store in a temp dir (kept alive by the struct).
pub(crate) struct Fx {
    pub store: SqliteStore,
    pub dir: TempDir,
}

impl Fx {
    pub fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let store = SqliteStore::open(&dir.path().join("collection.db")).unwrap();
        Self { store, dir }
    }

    /// New store with the Classic note type seeded.
    pub fn seeded() -> Self {
        let fx = Self::new();
        fx.write(|tx| tx.upsert_notetype(&classic()).unwrap());
        fx
    }

    pub fn db_path(&self) -> std::path::PathBuf {
        self.dir.path().join("collection.db")
    }

    /// A raw connection to the same file (schema/FTS inspection).
    pub fn raw(&self) -> Connection {
        Connection::open(self.db_path()).unwrap()
    }

    /// Runs `f` in a write transaction and commits.
    pub fn write<R>(&self, f: impl FnOnce(&mut dyn Writes) -> R) -> R {
        let mut tx = self.store.write().unwrap();
        let r = f(&mut *tx);
        tx.commit().unwrap();
        r
    }

    /// Runs `f` in a read snapshot.
    pub fn read<R>(&self, f: impl FnOnce(&dyn crate::repo::Reads) -> R) -> R {
        let tx = self.store.read().unwrap();
        f(&*tx)
    }
}

pub(crate) fn u(n: u128) -> Uuid {
    // Version-7-looking ids so ordering by id is meaningful in tests.
    Uuid::from_u128(0x0190_0000_0000_7000_8000_0000_0000_0000 | n)
}

pub(crate) fn did(n: u128) -> DeckId {
    DeckId(u(0x1000 + n))
}

pub(crate) fn nid(n: u128) -> NoteId {
    NoteId(u(0x2000 + n))
}

pub(crate) fn cid(n: u128) -> CardId {
    CardId(u(0x3000 + n))
}

pub(crate) fn tag(s: &str) -> TagName {
    TagName::parse(s).unwrap()
}

/// Test id for a tag name (the app would take it from its `IdGen`).
pub(crate) fn tid_of(name: &TagName) -> TagId {
    TagId(u(
        0x4_0000_0000 + u128::from(fnv1a64(name.key().as_bytes()) & 0xffff_ffff)
    ))
}

/// Deck number of the default home deck used by [`put_note`].
pub(crate) const HOME: u128 = 0xff;

/// Inserts the default home deck if missing.
pub(crate) fn ensure_home(tx: &mut dyn Writes) {
    if tx.get_deck(did(HOME)).unwrap().is_none() {
        tx.insert_deck(&deck(HOME, None, "Home")).unwrap();
    }
}

/// What the app does before writing a note: resolve/create its tags with caller ids.
pub(crate) fn ensure_note_tags(tx: &mut dyn Writes, n: &Note) {
    let pairs: Vec<(TagName, TagId)> = n.tags.iter().map(|t| (t.clone(), tid_of(t))).collect();
    tx.ensure_tags(&pairs, n.updated_at).unwrap();
}

/// Inserts `n` homed in the default home deck (created on demand), creating its tags first.
pub(crate) fn put_note(tx: &mut dyn Writes, n: &Note, text: &str) -> StorageResult<()> {
    ensure_home(tx);
    ensure_note_tags(tx, n);
    tx.insert_note(&homed(n.clone(), did(HOME)), text)
}

/// Updates `n`, creating its tags first.
pub(crate) fn put_update(tx: &mut dyn Writes, n: &Note, text: &str) -> StorageResult<()> {
    ensure_note_tags(tx, n);
    tx.update_note(n, text)
}

pub(crate) fn classic() -> NoteType {
    builtin::classic(Timestamp(1), "Classic", "Front", "Back")
}

pub(crate) fn deck(n: u128, parent: Option<u128>, name: &str) -> Deck {
    Deck {
        id: did(n),
        parent_id: parent.map(did),
        name: name.to_owned(),
        description: String::new(),
        icon: None,
        color: None,
        preset_id: None,
        position: 0,
        created_at: Timestamp(10),
        updated_at: Timestamp(10),
        deleted_at: None,
    }
}

pub(crate) fn note(n: u128, front: &str, back: &str, tags: &[&str], at: i64) -> Note {
    let nt = classic();
    let fields: FieldValues = [
        (FieldKey::new(builtin::FRONT), front.to_owned()),
        (FieldKey::new(builtin::BACK), back.to_owned()),
    ]
    .into_iter()
    .collect();
    Note {
        id: nid(n),
        notetype_id: nt.id,
        sort_field: sort_field_text(&nt, &fields),
        checksum: note_checksum(&nt, &fields),
        fields,
        tags: tags.iter().map(|t| tag(t)).collect(),
        created_at: Timestamp(at),
        updated_at: Timestamp(at),
        deleted_at: None,
        home_deck_id: did(HOME),
    }
}

/// `n` re-homed in `deck`.
pub(crate) fn homed(mut n: Note, deck: DeckId) -> Note {
    n.home_deck_id = deck;
    n
}

pub(crate) fn search_text(n: &Note) -> String {
    searchable_text(&classic(), &n.fields)
}

pub(crate) fn card(n: u128, note: u128, deck: u128) -> Card {
    let spec = standard_card_specs(&classic()).remove(0);
    Card::new(cid(n), nid(note), did(deck), spec, Timestamp(10))
}

/// Inserts a note with one card (card id = note uuid) in `deck`.
pub(crate) fn add_note(tx: &mut dyn Writes, n: &Note, deck: u128) {
    put_note(tx, n, &search_text(n)).unwrap();
    let spec = standard_card_specs(&classic()).remove(0);
    tx.insert_card(&Card::new(
        CardId(n.id.0),
        n.id,
        did(deck),
        spec,
        n.created_at,
    ))
    .unwrap();
}

/// The interface must stay object-safe: the app holds `Arc<dyn Store>`.
#[allow(dead_code)]
fn object_safety(
    store: &dyn Store,
    r: &dyn crate::store::ReadTx,
    w: Box<dyn crate::store::WriteTx + '_>,
) {
    let _: &dyn crate::Reads = r;
    let _: &dyn crate::Reads = &*w;
    let _ = store;
}
