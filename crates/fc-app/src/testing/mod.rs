//! Test support (feature `testing`, always on for this crate's own tests):
//!
//! * [`MemStore`]: an in-memory implementation of every `fc-storage` trait;
//! * [`TestApp`]: an [`App`] with a ticking clock, seeded ids and captured events;
//! * [`suite`] + [`store_suite!`](crate::store_suite): storage-agnostic service tests that run
//!   against any [`Store`] factory (the in-memory one here, `SqliteStore` once it lands).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

mod mem_store;
pub mod suite;

pub use mem_store::{MemState, MemStore, MemTx};

use crate::events::{EventBus, InMemoryEventBus};
use crate::{App, AppDeps};
use fc_api::v1::events::EventEnvelope;
use fc_api::v1::oplog::Actor;
use fc_api::{ApiError, ApiMethod};
use fc_core::rng::SplitMix64;
use fc_core::{Clock, Timestamp, V7IdGen};
use fc_storage::{ReadTx, SqliteStore, StorageResult, Store, WriteTx};
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

/// A clock that advances 1 ms on every read, so every dispatch gets a distinct `now`.
#[derive(Debug)]
pub struct TickClock(AtomicI64);

impl TickClock {
    /// Starts at `start` ms.
    pub fn new(start: i64) -> Self {
        Self(AtomicI64::new(start))
    }
}

impl Clock for TickClock {
    fn now(&self) -> Timestamp {
        Timestamp(self.0.fetch_add(1, Ordering::SeqCst))
    }
}

/// Distinct id seeds for apps sharing a store within one process.
fn next_seed() -> u64 {
    static SEED: AtomicU64 = AtomicU64::new(42);
    SEED.fetch_add(1, Ordering::Relaxed)
}

/// A [`SqliteStore`] on a fresh file in its own temporary directory (removed on drop).
pub struct TempSqlite {
    store: SqliteStore,
    dir: tempfile::TempDir,
}

impl TempSqlite {
    /// Creates and migrates `collection.db` in a new temporary directory.
    pub fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let store = SqliteStore::open(&dir.path().join("collection.db")).unwrap();
        Self { store, dir }
    }

    /// Database file path.
    pub fn path(&self) -> std::path::PathBuf {
        self.dir.path().join("collection.db")
    }
}

impl Default for TempSqlite {
    fn default() -> Self {
        Self::new()
    }
}

impl Store for TempSqlite {
    fn read(&self) -> StorageResult<Box<dyn ReadTx + '_>> {
        self.store.read()
    }

    fn write(&self) -> StorageResult<Box<dyn WriteTx + '_>> {
        self.store.write()
    }

    fn schema_version(&self) -> StorageResult<u32> {
        self.store.schema_version()
    }
}

/// Builds a store for one test.
pub type StoreFactory = dyn Fn() -> Arc<dyn Store>;

/// An app wired for tests.
pub struct TestApp {
    /// The app (`load_state` already done).
    pub app: App,
    /// Its store.
    pub store: Arc<dyn Store>,
    /// Every published event, in order.
    pub events: Arc<Mutex<Vec<EventEnvelope>>>,
}

impl TestApp {
    /// New app over `store`: ticking clock from 2026-01-01, deterministic ids.
    pub fn new(store: Arc<dyn Store>) -> Self {
        Self::with_clock(store, Arc::new(TickClock::new(1_767_225_600_000)))
    }

    /// New app over `store` reading time from `clock` (v1.0: reminders, the app lock).
    pub fn with_clock(store: Arc<dyn Store>, clock: Arc<dyn Clock>) -> Self {
        let bus = Arc::new(InMemoryEventBus::new());
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = events.clone();
        bus.subscribe(Arc::new(move |e: &EventEnvelope| {
            sink.lock().unwrap().push(e.clone());
        }));
        let app = App::new(AppDeps {
            store: store.clone(),
            ids: Box::new(V7IdGen::new(clock.clone(), SplitMix64::new(next_seed()))),
            clock,
            events: bus,
        })
        .unwrap();
        app.load_state().unwrap();
        Self { app, store, events }
    }

    /// In-memory app.
    pub fn mem() -> Self {
        Self::new(Arc::new(MemStore::new()))
    }

    /// Typed call as the user; panics on error.
    pub fn ok<M: ApiMethod>(&self, req: M::Request) -> M::Response {
        match self.app.call::<M>(&req, &Actor::User) {
            Ok(r) => r,
            Err(e) => panic!("{} failed: {e:?}", M::NAME),
        }
    }

    /// Typed call as the user; panics on success.
    pub fn err<M: ApiMethod>(&self, req: M::Request) -> ApiError {
        match self.app.call::<M>(&req, &Actor::User) {
            Ok(_) => panic!("{} unexpectedly succeeded", M::NAME),
            Err(e) => e,
        }
    }

    /// Takes the captured events.
    pub fn take_events(&self) -> Vec<EventEnvelope> {
        std::mem::take(&mut *self.events.lock().unwrap())
    }
}
