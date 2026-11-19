//! The application service: the single entry point every transport calls.

use crate::error::{AppError, AppResult};
use crate::events::{EventBus, InMemoryEventBus};
use crate::handler::{CommandCtx, PendingOplogEntry, QueryCtx};
use crate::jobs::Invoker;
use crate::registry::{unknown_method, ErasedHandler, Registry, RegistryError};
use crate::settings::SettingsSchema;
use crate::system::{OsRng, SystemClock};
use fc_api::method::parse_qualified_name;
use fc_api::v1::events::DomainEvent;
use fc_api::v1::events::EventEnvelope;
use fc_api::v1::oplog::{Actor, OplogKind};
use fc_api::{ApiError, ApiMethod, ErrorCode, InvokeRequest, Visibility};
use fc_core::i18n::Locale;
use fc_core::notetype::builtin;
use fc_core::settings::SettingScope;
use fc_core::{BatchId, Clock, IdGen, OplogId, V7IdGen};
use fc_storage::repo::{OplogRecord, OplogRecordKind};
use fc_storage::{Reads, Store};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};

/// External dependencies of the app (injected so tests use a fixed clock / seeded ids).
pub struct AppDeps {
    /// Storage.
    pub store: Arc<dyn Store>,
    /// Clock.
    pub clock: Arc<dyn Clock>,
    /// Id generator.
    pub ids: Box<dyn IdGen>,
    /// Event bus.
    pub events: Arc<dyn EventBus>,
}

impl AppDeps {
    /// Production defaults: system clock, OS-seeded UUIDv7 ids, in-memory event bus.
    pub fn with_store(store: Arc<dyn Store>) -> Self {
        let clock: Arc<dyn Clock> = Arc::new(SystemClock);
        Self {
            store,
            ids: Box::new(V7IdGen::new(clock.clone(), OsRng::new())),
            clock,
            events: Arc::new(InMemoryEventBus::new()),
        }
    }
}

/// The application service.
pub struct App {
    store: Arc<dyn Store>,
    clock: Arc<dyn Clock>,
    ids: Mutex<Box<dyn IdGen>>,
    events: Arc<dyn EventBus>,
    registry: Registry,
    settings: SettingsSchema,
    module_enabled: RwLock<HashMap<&'static str, bool>>,
    locale: RwLock<Locale>,
    event_seq: AtomicU64,
}

impl App {
    /// Builds the app: registers all modules (`modules::register_all`) and validates the
    /// registry. Does not touch the database (call [`App::load_state`] after opening).
    pub fn new(deps: AppDeps) -> Result<Self, RegistryError> {
        let mut registry = Registry::new();
        crate::modules::register_all(&mut registry);
        let registry = registry.finish()?;
        let module_enabled = registry
            .modules()
            .iter()
            .map(|m| (m.id, m.enabled_by_default))
            .collect();
        let settings = SettingsSchema::from_modules(registry.modules());
        Ok(Self {
            store: deps.store,
            clock: deps.clock,
            ids: Mutex::new(deps.ids),
            events: deps.events,
            registry,
            settings,
            module_enabled: RwLock::new(module_enabled),
            locale: RwLock::new(Locale::default()),
            event_seq: AtomicU64::new(0),
        })
    }

    /// Startup work after the store is open (and migrated — `SqliteStore::open` migrates):
    /// seeds built-ins (the Classic note type, localized by `app.locale`), marks jobs left
    /// running/queued as interrupted, then loads module switches and the locale into memory.
    /// Idempotent. Seeding is not an undoable command, so it writes no oplog entry.
    pub fn load_state(&self) -> AppResult<()> {
        let now = self.clock.now();
        let mut tx = self.store.write()?;
        let locale = read_locale(&*tx)?;
        if tx.get_notetype(builtin::CLASSIC_ID)?.is_none() {
            let (name, front, back) = match locale {
                Locale::Tr => ("Klasik", "Ön", "Arka"),
                Locale::En => ("Classic", "Front", "Back"),
            };
            tx.upsert_notetype(&builtin::classic(now, name, front, back))?;
        }
        tx.mark_unfinished_jobs_interrupted(now)?;
        tx.commit()?;
        self.reload_cached_state()
    }

    /// Opens (creating and migrating if needed) the SQLite collection at `path` and returns a
    /// ready app: production deps, [`App::load_state`] done.
    pub fn open_sqlite(path: &std::path::Path) -> AppResult<Self> {
        let store = Arc::new(fc_storage::SqliteStore::open(path)?);
        let app = Self::new(AppDeps::with_store(store))
            .map_err(|e| AppError::api(ErrorCode::Internal, e.to_string()))?;
        app.load_state()?;
        Ok(app)
    }

    /// Re-reads module switches and the locale from storage into the in-memory caches.
    pub fn reload_cached_state(&self) -> AppResult<()> {
        let tx = self.store.read()?;
        let states = crate::support::module_states(&*tx, &self.registry)?;
        if let Ok(mut m) = self.module_enabled.write() {
            m.extend(states);
        }
        self.set_locale(read_locale(&*tx)?);
        Ok(())
    }

    /// Keeps in-memory caches in step with committed changes (also when they come from
    /// undo/redo, whose nested calls emit the same events).
    fn apply_committed(&self, events: &[DomainEvent]) {
        let relevant = events.iter().any(|e| match e {
            DomainEvent::ModuleToggled { .. } => true,
            DomainEvent::SettingChanged { scope, key } => {
                *scope == SettingScope::App && key == crate::modules::core::LOCALE
            }
            _ => false,
        });
        if relevant {
            // A failed reload keeps the previous cache; the next start reloads anyway.
            let _ = self.reload_cached_state();
        }
    }

    /// The registry (for `commands.list`, transports and tests).
    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    /// Settings schema of all modules.
    pub fn settings_schema(&self) -> &SettingsSchema {
        &self.settings
    }

    /// Event bus (transports subscribe here).
    pub fn events(&self) -> &Arc<dyn EventBus> {
        &self.events
    }

    /// Whether module `id` is enabled.
    pub fn is_module_enabled(&self, id: &str) -> bool {
        self.module_enabled
            .read()
            .map(|m| m.get(id).copied().unwrap_or(false))
            .unwrap_or(false)
    }

    /// Updates the in-memory module switch (the `modules.set_enabled` handler persists it).
    pub fn set_module_enabled_cached(&self, id: &'static str, enabled: bool) {
        if let Ok(mut m) = self.module_enabled.write() {
            m.insert(id, enabled);
        }
    }

    /// Current UI locale.
    pub fn locale(&self) -> Locale {
        self.locale.read().map(|l| *l).unwrap_or_default()
    }

    /// Sets the UI locale (from the `app.locale` setting).
    pub fn set_locale(&self, locale: Locale) {
        if let Ok(mut l) = self.locale.write() {
            *l = locale;
        }
    }

    /// Typed convenience over [`App::invoke`].
    pub fn call<M: ApiMethod>(
        &self,
        request: &M::Request,
        actor: &Actor,
    ) -> Result<M::Response, ApiError> {
        let params = serde_json::to_value(request)
            .map_err(|e| ApiError::new(ErrorCode::ValidationInvalidParams, e.to_string()))?;
        let value = self.invoke(
            &format!("{}.{}", fc_api::API_VERSION, M::NAME),
            params,
            actor,
        )?;
        serde_json::from_value(value).map_err(|e| ApiError::new(ErrorCode::Internal, e.to_string()))
    }

    /// Executes a method by qualified wire name (`v1.decks.create`). This is what every
    /// transport calls.
    ///
    /// Queries: parse params → read snapshot → handler.
    /// Commands: parse + validate → write tx → handler (nested calls share tx and batch) →
    /// oplog entries → commit → publish events.
    pub fn invoke(&self, method: &str, params: Value, actor: &Actor) -> Result<Value, ApiError> {
        self.invoke_inner(method, params, actor)
            .map_err(AppError::into_api)
    }

    fn invoke_inner(&self, method: &str, params: Value, actor: &Actor) -> AppResult<Value> {
        let name = parse_qualified_name(method).ok_or_else(|| unknown_method(method))?;
        let entry = self
            .registry
            .entry(name)
            .ok_or_else(|| unknown_method(method))?;
        let d = &entry.descriptor;
        if d.visibility == Visibility::Internal {
            return Err(AppError::Api(
                ApiError::new(ErrorCode::PermissionDenied, format!("{method} is internal"))
                    .with_detail("permission", d.permission),
            ));
        }
        if !self.is_module_enabled(d.module) {
            return Err(AppError::Api(
                ApiError::new(
                    ErrorCode::ConflictModuleDisabled,
                    format!("module {} is disabled", d.module),
                )
                .with_detail("module", d.module),
            ));
        }
        match &entry.handler {
            ErasedHandler::Query(h) => {
                h.validate(&params)?;
                let tx = self.store.read()?;
                let ctx = QueryCtx {
                    tx: &*tx,
                    now: self.clock.now(),
                    locale: self.locale(),
                    registry: &self.registry,
                };
                h.run(params, &ctx)
            }
            ErasedHandler::Command(h) => {
                h.validate(&params)?;
                let now = self.clock.now();
                let locale = self.locale();
                let mut ids = self
                    .ids
                    .lock()
                    .map_err(|_| AppError::api(ErrorCode::Internal, "id generator poisoned"))?;
                let mut tx = self.store.write()?;
                let batch_id = BatchId::generate(&mut **ids);
                let mut ctx = CommandCtx {
                    tx: &mut *tx,
                    now,
                    ids: &mut **ids,
                    actor,
                    locale,
                    batch_id,
                    batch_kind: OplogKind::Do,
                    reverts_batch_id: None,
                    registry: &self.registry,
                    oplog: Vec::new(),
                    events: Vec::new(),
                };
                let outcome = h.execute(params.clone(), &mut ctx)?;
                let mut oplog = std::mem::take(&mut ctx.oplog);
                let mut events = std::mem::take(&mut ctx.events);
                let (kind, reverts) = (ctx.batch_kind, ctx.reverts_batch_id);
                if d.undoable && !outcome.inverse.is_empty() {
                    oplog.insert(
                        0,
                        PendingOplogEntry {
                            command: method.to_owned(),
                            payload: params,
                            inverse: outcome.inverse,
                            summary: outcome.summary,
                        },
                    );
                }
                events.extend(outcome.events);
                let actor_json = serde_json::to_value(actor)
                    .map_err(|e| AppError::api(ErrorCode::Internal, e.to_string()))?;
                for (seq, entry) in oplog.into_iter().enumerate() {
                    let record = OplogRecord {
                        id: OplogId::generate(&mut **ids),
                        batch_id,
                        seq: u32::try_from(seq).unwrap_or(u32::MAX),
                        command: entry.command,
                        payload: entry.payload,
                        inverse: serde_json::to_value(&entry.inverse)
                            .map_err(|e| AppError::api(ErrorCode::Internal, e.to_string()))?,
                        actor: actor_json.clone(),
                        kind: match kind {
                            OplogKind::Do => OplogRecordKind::Do,
                            OplogKind::Undo => OplogRecordKind::Undo,
                            OplogKind::Redo => OplogRecordKind::Redo,
                        },
                        reverts_batch_id: reverts,
                        summary: serde_json::to_value(&entry.summary)
                            .map_err(|e| AppError::api(ErrorCode::Internal, e.to_string()))?,
                        created_at: now,
                    };
                    tx.append_oplog(&record)?;
                }
                tx.commit()?;
                drop(ids);
                self.apply_committed(&events);
                for event in events {
                    let envelope = EventEnvelope {
                        seq: self.event_seq.fetch_add(1, Ordering::SeqCst),
                        at: now,
                        batch_id: Some(batch_id),
                        event,
                    };
                    self.events.publish(&envelope);
                }
                Ok(outcome.response)
            }
        }
    }
}

/// Locale from the `app.locale` setting (schema default `tr`).
fn read_locale(reads: &dyn Reads) -> AppResult<Locale> {
    let value = reads.get_setting(&SettingScope::App, crate::modules::core::LOCALE)?;
    Ok(match value.as_ref().and_then(|v| v.as_str()) {
        Some("en") => Locale::En,
        _ => Locale::Tr,
    })
}

impl Invoker for App {
    fn invoke_request(&self, request: &InvokeRequest, actor: &Actor) -> Result<Value, ApiError> {
        self.invoke(&request.method, request.params.clone(), actor)
    }
}
