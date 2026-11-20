//! The application service: the single entry point every transport calls.

use crate::error::{AppError, AppResult};
use crate::events::{EventBus, InMemoryEventBus};
use crate::handler::{CommandCtx, PendingOplogEntry, QueryCtx};
use crate::jobs::Invoker;
use crate::modules::plugins::{Hooks, PluginHost};
use crate::registry::{
    ensure_access, ensure_invocable, run_recorded, unknown_method, ErasedCommand, ErasedHandler,
    Registry, RegistryError,
};
use crate::settings::SettingsSchema;
use crate::system::{OsRng, SystemClock};
use fc_api::method::parse_qualified_name;
use fc_api::method::MethodDescriptor;
use fc_api::v1::common::I18nMessage;
use fc_api::v1::events::DomainEvent;
use fc_api::v1::events::EventEnvelope;
use fc_api::v1::jobs::{JobRef, JobStatus};
use fc_api::v1::methods::JobsCancel;
use fc_api::v1::oplog::{Actor, OplogKind};
use fc_api::{ApiError, ApiMethod, ErrorCode, InvokeRequest, Visibility};
use fc_core::i18n::Locale;
use fc_core::notetype::builtin;
use fc_core::settings::SettingScope;
use fc_core::{BatchId, Clock, IdGen, JobId, OplogId, V7IdGen};
use fc_storage::repo::{OplogRecord, OplogRecordKind, UnfinishedBatchRecord};
use fc_storage::{Reads, Store};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

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
    jobs: crate::jobs::JobRuntime,
    /// Commands waiting for the writer (long jobs yield to them between chunks).
    writers_waiting: AtomicUsize,
    /// Folder of the collection file (`None` until known; see [`App::data_dir`]).
    data_dir: OnceLock<PathBuf>,
    /// Oplog position this process has accounted for (see [`App::poll_external_changes`]).
    oplog_seen: Mutex<Option<i64>>,
    /// Plugin runtime: loaded plugins, hooks, UI sessions (v0.7).
    plugins: PluginHost,
}

/// Where a command's oplog entries go.
pub(crate) enum OplogMode<'g> {
    /// Written with the command, as one batch (every normal call).
    Write,
    /// Held back in an [`UndoGroup`] that a long job commits as one batch at its end.
    Defer(&'g mut UndoGroup),
    /// Dropped: compensation that leaves no history (discarding an unfinished import). With
    /// a group's batch id, its durable marker is removed in the same transaction.
    Discard(Option<BatchId>),
}

/// One undo step written across several transactions (a long import writes in chunks so
/// other commands are never locked out for long). Its oplog entries are held here and written
/// together by [`App::commit_group`] — the oplog requires a batch's entries to be contiguous,
/// and an unfinished group must never be an undo target.
///
/// **Crash safety.** A group is durable: [`App::begin_group`] writes a marker row
/// (`unfinished_batches`, state `started`) with the compensating `rollback` call before
/// anything else, and every command run in the group appends its inverse calls to the marker
/// **in its own transaction** (state `writing`). Committing or discarding the group removes
/// the marker in that same transaction. So if the app stops in between, the marker describes
/// exactly the committed parts, and [`App::load_state`] rolls them back — a change is never
/// left without an undo step.
pub(crate) struct UndoGroup {
    batch_id: BatchId,
    pending: Vec<PendingOplogEntry>,
    /// Marker row of this group (see above).
    marker: UnfinishedBatchRecord,
    /// Parts appended to the marker so far.
    parts: u32,
}

impl UndoGroup {
    /// Inverse calls of everything run so far, newest first (to revert the group).
    pub(crate) fn inverses_rev(&self) -> Vec<InvokeRequest> {
        self.pending
            .iter()
            .rev()
            .flat_map(|e| e.inverse.iter().cloned())
            .collect()
    }

    /// Whether nothing has been recorded.
    pub(crate) fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// Batch id of the group (also its marker's key).
    pub(crate) fn batch_id(&self) -> BatchId {
        self.batch_id
    }
}

/// Inverse calls of `entries` as stored in a marker part: one list per entry, in order.
fn part_inverses(entries: &[PendingOplogEntry]) -> AppResult<Value> {
    let lists: Vec<&Vec<InvokeRequest>> = entries.iter().map(|e| &e.inverse).collect();
    serde_json::to_value(lists).map_err(|e| AppError::api(ErrorCode::Internal, e.to_string()))
}

/// The rollback call of a marker with its parts' inverses filled in (newest first) as
/// `params.calls`.
pub(crate) fn rollback_request(rollback: &Value, parts: &[Value]) -> AppResult<InvokeRequest> {
    let internal = |e: serde_json::Error| AppError::api(ErrorCode::Internal, e.to_string());
    let mut calls: Vec<InvokeRequest> = Vec::new();
    for part in parts.iter().rev() {
        let lists: Vec<Vec<InvokeRequest>> =
            serde_json::from_value(part.clone()).map_err(internal)?;
        calls.extend(lists.into_iter().rev().flatten());
    }
    let mut request: InvokeRequest = serde_json::from_value(rollback.clone()).map_err(internal)?;
    let calls = serde_json::to_value(calls).map_err(internal)?;
    match &mut request.params {
        Value::Object(m) => {
            m.insert("calls".into(), calls);
        }
        other => *other = serde_json::json!({ "calls": calls }),
    }
    Ok(request)
}

/// Decrements [`App::writers_waiting`] when dropped.
struct WaitingWriter<'a>(&'a AtomicUsize, bool);

impl WaitingWriter<'_> {
    fn acquired(&mut self) {
        if self.1 {
            self.0.fetch_sub(1, Ordering::SeqCst);
            self.1 = false;
        }
    }
}

impl Drop for WaitingWriter<'_> {
    fn drop(&mut self) {
        self.acquired();
    }
}

impl App {
    /// Builds the app: registers all modules (`modules::register_all`) and validates the
    /// registry. Does not touch the database (call [`App::load_state`] after opening).
    pub fn new(deps: AppDeps) -> Result<Self, RegistryError> {
        let mut registry = Registry::new();
        crate::modules::register_all(&mut registry);
        Self::with_registry(deps, registry)
    }

    /// Builds the app over a custom registry (tests wire in probe handlers this way).
    pub(crate) fn with_registry(deps: AppDeps, registry: Registry) -> Result<Self, RegistryError> {
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
            jobs: crate::jobs::JobRuntime::default(),
            writers_waiting: AtomicUsize::new(0),
            data_dir: OnceLock::new(),
            oplog_seen: Mutex::new(None),
            plugins: PluginHost::default(),
        })
    }

    /// Startup work after the store is open (and migrated — `SqliteStore::open` migrates):
    /// seeds built-ins (the Classic note type, localized by `app.locale`), marks jobs left
    /// running/queued as interrupted, removes media files without a media entry (left by an
    /// undone `media.add`; redo works until the next start), then loads module switches and
    /// the locale into memory, rolls back crash-interrupted undo groups and purges the expired
    /// trash (`browse.purge_trash`, v0.4). Idempotent. Seeding is not an undoable command, so it writes no oplog entry.
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
        crate::support::content::seed_builtin_notetypes(&mut *tx, locale, now)?;
        tx.mark_unfinished_jobs_interrupted(now)?;
        tx.delete_orphan_media_files()?;
        tx.commit()?;
        self.reload_cached_state()?;
        self.roll_back_unfinished_groups()?;
        // Plugins load best effort: a broken plugin is reported on its row, never blocks.
        self.plugins.reload(self);
        // Best effort: an expired-trash purge that fails must not keep the collection closed
        // (it runs again next start, and the user can run `browse.purge_trash`).
        let _ = crate::modules::browse::purge_on_startup(self);
        // Position from which other processes' commits count as news.
        self.poll_external_changes()?;
        Ok(())
    }

    /// Startup: undoes what undo groups left unfinished by a crash wrote (see [`UndoGroup`]).
    /// The rollback runs unlogged and removes the marker in its own transaction. A rollback
    /// that fails keeps its marker (tried again at the next start) and does not stop the
    /// app from opening.
    fn roll_back_unfinished_groups(&self) -> AppResult<()> {
        let markers = {
            let tx = self.store.read()?;
            let mut out = Vec::new();
            for m in tx.list_unfinished_batches()? {
                let parts = tx.unfinished_batch_parts(m.batch_id)?;
                out.push((m, parts));
            }
            out
        };
        for (marker, parts) in markers {
            if parts.is_empty() {
                let mut tx = self.store.write()?;
                tx.delete_unfinished_batch(marker.batch_id)?;
                tx.commit()?;
                continue;
            }
            let Ok(request) = rollback_request(&marker.rollback, &parts) else {
                continue;
            };
            // Recovery must not depend on module switches: run the handler directly.
            let _ = self.invoke_entry(
                &request,
                &Actor::System,
                OplogMode::Discard(Some(marker.batch_id)),
                false,
            );
        }
        Ok(())
    }

    /// Opens (creating and migrating if needed) the SQLite collection at `path` and returns a
    /// ready app: production deps, [`App::load_state`] done.
    pub fn open_sqlite(path: &std::path::Path) -> AppResult<Self> {
        let store = Arc::new(fc_storage::SqliteStore::open(path)?);
        let app = Self::new(AppDeps::with_store(store))
            .map_err(|e| AppError::api(ErrorCode::Internal, e.to_string()))?;
        if let Some(dir) = path.parent() {
            app.set_data_dir(dir.to_path_buf());
        }
        app.load_state()?;
        Ok(app)
    }

    /// Like [`App::open_sqlite`] for an existing collection that must **not** be migrated
    /// (secondary processes such as `fc mcp serve`): an older schema is refused with
    /// `IO_DATABASE` ("must be migrated by the app first"), never upgraded behind the app's back.
    /// It only [attaches](App::attach_state): the app that owns the file may be running.
    pub fn open_sqlite_current(path: &std::path::Path) -> AppResult<Self> {
        let store = Arc::new(fc_storage::SqliteStore::open_current(path)?);
        let app = Self::new(AppDeps::with_store(store))
            .map_err(|e| AppError::api(ErrorCode::Internal, e.to_string()))?;
        if let Some(dir) = path.parent() {
            app.set_data_dir(dir.to_path_buf());
        }
        app.attach_state()?;
        Ok(app)
    }

    /// Opens the collection for a process that may run next to the desktop app (the CLI): a
    /// missing file is created (migrated, seeded: [`App::open_sqlite`]); an existing current
    /// one is only [attached](App::attach_state), so the running app's jobs and import groups
    /// stay untouched. An older schema is migrated only when `allow_migrate` is set —
    /// otherwise it is refused (`IO_DATABASE`), never upgraded behind the app's back.
    pub fn open_sqlite_shared(path: &std::path::Path, allow_migrate: bool) -> AppResult<Self> {
        if !path.is_file() {
            return Self::open_sqlite(path);
        }
        match Self::open_sqlite_current(path) {
            Err(AppError::Storage(fc_storage::StorageError::NeedsMigration { .. }))
                if allow_migrate =>
            {
                Self::open_sqlite(path)
            }
            other => other,
        }
    }

    /// Startup of a **secondary** process on a collection the app owns (`fc mcp serve`): loads
    /// module switches and the locale and records the oplog position — and writes nothing.
    /// [`App::load_state`]'s startup work belongs to the owning app alone: in a second process
    /// it would mark the app's running jobs interrupted, roll back an import still in progress
    /// as if it had crashed, and purge the trash (a history barrier the user did not ask for).
    pub fn attach_state(&self) -> AppResult<()> {
        self.reload_cached_state()?;
        self.poll_external_changes()?;
        Ok(())
    }

    /// Notices commits other processes made (the CLI, `fc mcp serve`, …): when oplog entries
    /// appeared since the last call whose actor is not this app's own (`user`/`system`), it
    /// reloads the cached module/locale state and publishes [`DomainEvent::DataChanged`], so
    /// open screens refetch. Returns whether it did. Cheap when nothing changed (one
    /// `max(rowid)`): the desktop calls it on a timer and when its window gets focus. The
    /// first call only records the position.
    pub fn poll_external_changes(&self) -> AppResult<bool> {
        let tx = self.store.read()?;
        let now = tx.oplog_watermark()?;
        let mut seen = self
            .oplog_seen
            .lock()
            .map_err(|_| AppError::api(ErrorCode::Internal, "oplog watermark poisoned"))?;
        let Some(before) = *seen else {
            *seen = Some(now);
            return Ok(false);
        };
        if now == before {
            return Ok(false);
        }
        // Fewer entries than before: the log was replaced (restore) — treat as foreign.
        let foreign = now < before
            || tx.oplog_actors_after(before)?.iter().any(|a| {
                !matches!(
                    serde_json::from_value::<Actor>(a.clone()),
                    Ok(Actor::User | Actor::System)
                )
            });
        *seen = Some(now);
        drop(seen);
        drop(tx);
        if foreign {
            self.reload_cached_state()?;
            self.publish(vec![DomainEvent::DataChanged], None);
        }
        Ok(foreign)
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
        if events.iter().any(
            |e| matches!(e, DomainEvent::JobProgress { job } if job.status == JobStatus::Queued),
        ) {
            self.jobs.notify();
        }
        let plugins = events.iter().any(|e| match e {
            DomainEvent::PluginsChanged { .. } | DomainEvent::CollectionReplaced => true,
            DomainEvent::SettingChanged { scope, key } => {
                *scope == SettingScope::App && key.starts_with("plugins.")
            }
            _ => false,
        });
        if plugins {
            self.plugins.reload(self);
        }
        let relevant = events.iter().any(|e| match e {
            DomainEvent::ModuleToggled { .. } | DomainEvent::CollectionReplaced => true,
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

    /// Current time from the injected clock.
    pub(crate) fn now(&self) -> fc_core::Timestamp {
        self.clock.now()
    }

    /// The store.
    pub(crate) fn store(&self) -> &Arc<dyn Store> {
        &self.store
    }

    /// Folder of the collection (app data: automatic backups go under it). Set by
    /// [`App::open_sqlite`]; transports opening the store themselves call
    /// [`App::set_data_dir`].
    pub fn data_dir(&self) -> Option<&std::path::Path> {
        self.data_dir.get().map(PathBuf::as_path)
    }

    /// Sets [`App::data_dir`] (first call wins).
    pub fn set_data_dir(&self, dir: PathBuf) {
        let _ = self.data_dir.set(dir);
    }

    /// Whether a command is waiting for the writer.
    pub(crate) fn writers_waiting(&self) -> bool {
        self.writers_waiting.load(Ordering::SeqCst) > 0
    }

    /// In-memory job state (cancel tokens, worker wake-up).
    pub(crate) fn job_runtime(&self) -> &crate::jobs::JobRuntime {
        &self.jobs
    }

    /// Publishes events outside a command (job progress).
    pub(crate) fn publish(&self, events: Vec<DomainEvent>, batch_id: Option<BatchId>) {
        let at = self.clock.now();
        for event in events {
            let envelope = EventEnvelope {
                seq: self.event_seq.fetch_add(1, Ordering::SeqCst),
                at,
                batch_id,
                event,
            };
            self.events.publish(&envelope);
        }
    }

    /// The plugin host.
    pub(crate) fn plugins(&self) -> &PluginHost {
        &self.plugins
    }

    /// Installs the native file dialog that serves plugins' `fs:pick` (the desktop shell
    /// sets one; without it `fs.open`/`fs.save` report `PROVIDER_UNAVAILABLE`).
    pub fn set_file_picker(&self, picker: Arc<dyn crate::modules::plugins::FilePicker>) {
        self.plugins.set_file_picker(picker);
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
        self.invoke_inner(method, params, actor, false)
            .map_err(AppError::into_api)
    }

    /// [`App::invoke`] for trusted in-process callers (background jobs): internal methods are
    /// allowed; the module switch still applies.
    pub(crate) fn invoke_trusted(
        &self,
        request: &InvokeRequest,
        actor: &Actor,
    ) -> Result<Value, ApiError> {
        self.invoke_inner(&request.method, request.params.clone(), actor, true)
            .map_err(AppError::into_api)
    }

    fn invoke_inner(
        &self,
        method: &str,
        params: Value,
        actor: &Actor,
        trusted: bool,
    ) -> AppResult<Value> {
        let hooks = if trusted {
            Hooks::Off
        } else {
            Hooks::Run { skip: None }
        };
        self.invoke_hooked(method, params, actor, trusted, hooks)
    }

    /// [`App::invoke_inner`] with an explicit plugin hook policy (v0.7): client calls run
    /// `before:`/`after:` hooks and deliver queued events to plugins; plugins' own host calls
    /// choose their policy (see `modules::plugins`).
    pub(crate) fn invoke_hooked(
        &self,
        method: &str,
        params: Value,
        actor: &Actor,
        trusted: bool,
        hooks: Hooks<'_>,
    ) -> AppResult<Value> {
        // The outermost dispatch on this thread is one user action (hook budget, v0.7).
        let _action = self.plugins.begin_action(self);
        let name = parse_qualified_name(method).ok_or_else(|| unknown_method(method))?;
        let Some(entry) = self.registry.entry(name) else {
            // Methods registered at runtime (plugin commands): run outside a transaction.
            let (m, handler) = self
                .registry
                .dynamic_entry(name)
                .ok_or_else(|| unknown_method(method))?;
            if !trusted {
                ensure_access(
                    &self.registry,
                    &m.qualified_name(),
                    Visibility::Public,
                    &m.permission,
                    &m.module,
                    actor,
                    self.is_module_enabled(&m.module),
                )?;
            }
            let result = handler.invoke(params, self, actor);
            self.plugins.after_operation(self);
            return result;
        };
        let d = &entry.descriptor;
        if trusted {
            if !self.is_module_enabled(d.module) {
                return Err(AppError::Api(
                    ApiError::new(
                        ErrorCode::ConflictModuleDisabled,
                        format!("module {} is disabled", d.module),
                    )
                    .with_detail("module", d.module),
                ));
            }
        } else {
            ensure_invocable(&self.registry, d, actor, self.is_module_enabled(d.module))?;
        }
        if name == JobsCancel::NAME {
            // Signal a running job before waiting for the writer it may be holding.
            if let Ok(r) = serde_json::from_value::<JobRef>(params.clone()) {
                self.jobs.request_cancel(r.id);
            }
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
            ErasedHandler::Action(h) => {
                h.validate(&params)?;
                let result = h.run(params, self, actor);
                self.plugins.after_operation(self);
                result
            }
            ErasedHandler::Command(h) => {
                let Hooks::Run { skip } = hooks else {
                    return self.run_command(
                        h.as_ref(),
                        d,
                        method,
                        params,
                        actor,
                        OplogMode::Write,
                        true,
                    );
                };
                let params = self.plugins.before_hooks(self, name, params, actor, skip)?;
                let kept = self.plugins.has_after_hooks(name).then(|| params.clone());
                let response =
                    self.run_command(h.as_ref(), d, method, params, actor, OplogMode::Write, true)?;
                if let Some(params) = kept {
                    self.plugins
                        .after_hooks(self, name, &params, &response, actor);
                }
                self.plugins.after_operation(self);
                Ok(response)
            }
        }
    }

    /// Runs a trusted command (see [`App::invoke_trusted`]) with its oplog entries going to
    /// `mode` instead of being written as their own batch.
    pub(crate) fn invoke_with(
        &self,
        request: &InvokeRequest,
        actor: &Actor,
        mode: OplogMode<'_>,
    ) -> AppResult<Value> {
        self.invoke_entry(request, actor, mode, true)
    }

    /// [`App::invoke_with`]; `check_module = false` skips the module switch (crash recovery).
    fn invoke_entry(
        &self,
        request: &InvokeRequest,
        actor: &Actor,
        mode: OplogMode<'_>,
        check_module: bool,
    ) -> AppResult<Value> {
        let method = request.method.as_str();
        let name = parse_qualified_name(method).ok_or_else(|| unknown_method(method))?;
        let entry = self
            .registry
            .entry(name)
            .ok_or_else(|| unknown_method(method))?;
        let d = &entry.descriptor;
        if check_module && !self.is_module_enabled(d.module) {
            return Err(AppError::Api(
                ApiError::new(
                    ErrorCode::ConflictModuleDisabled,
                    format!("module {} is disabled", d.module),
                )
                .with_detail("module", d.module),
            ));
        }
        let ErasedHandler::Command(h) = &entry.handler else {
            return Err(AppError::api(
                ErrorCode::ValidationInvalidParams,
                format!("{method} is a query"),
            ));
        };
        self.run_command(
            h.as_ref(),
            d,
            method,
            request.params.clone(),
            actor,
            mode,
            check_module,
        )
    }

    /// A new, empty [`UndoGroup`] of `job`, with its durable marker written. `rollback` is
    /// the internal command that removes what the group committed if the app stops before
    /// the group ends; it receives the group's inverse calls (newest first) as
    /// `params.calls`.
    pub(crate) fn begin_group(
        &self,
        job: Option<JobId>,
        rollback: &InvokeRequest,
    ) -> AppResult<UndoGroup> {
        let now = self.clock.now();
        let batch_id = {
            let mut ids = self
                .ids
                .lock()
                .map_err(|_| AppError::api(ErrorCode::Internal, "id generator poisoned"))?;
            BatchId::generate(&mut **ids)
        };
        let marker = UnfinishedBatchRecord {
            batch_id,
            job_id: job,
            state: "started".into(),
            rollback: serde_json::to_value(rollback)
                .map_err(|e| AppError::api(ErrorCode::Internal, e.to_string()))?,
            created_at: now,
            updated_at: now,
        };
        let mut tx = self.store.write()?;
        tx.upsert_unfinished_batch(&marker)?;
        tx.commit()?;
        Ok(UndoGroup {
            batch_id,
            pending: Vec::new(),
            marker,
            parts: 0,
        })
    }

    /// Ends a group that recorded nothing: removes its marker.
    pub(crate) fn close_empty_group(&self, group: &UndoGroup) -> AppResult<()> {
        let mut tx = self.store.write()?;
        tx.delete_unfinished_batch(group.batch_id)?;
        tx.commit()?;
        Ok(())
    }

    /// Writes the oplog entries of `group` as one batch (one undo step). `summary` replaces
    /// the first entry's summary (what the undo UI shows for the whole group).
    pub(crate) fn commit_group(
        &self,
        group: &UndoGroup,
        actor: &Actor,
        summary: Option<I18nMessage>,
    ) -> AppResult<()> {
        if group.pending.is_empty() {
            return self.close_empty_group(group);
        }
        let mut entries = group.pending.clone();
        if let (Some(first), Some(s)) = (entries.first_mut(), summary) {
            first.summary = s;
        }
        let now = self.clock.now();
        let mut ids = self
            .ids
            .lock()
            .map_err(|_| AppError::api(ErrorCode::Internal, "id generator poisoned"))?;
        let mut tx = self.store.write()?;
        write_oplog(
            &mut *tx,
            &mut **ids,
            entries,
            (group.batch_id, OplogKind::Do, None),
            actor,
            now,
        )?;
        tx.delete_unfinished_batch(group.batch_id)?;
        tx.commit()?;
        Ok(())
    }

    /// Commands: write tx → handler (nested calls share tx and batch) → oplog entries (per
    /// `mode`) → commit → publish events.
    // One flag per dispatch concern (oplog mode, module switches); a struct would only wrap
    // the two call sites.
    #[allow(clippy::too_many_arguments)]
    fn run_command(
        &self,
        h: &dyn ErasedCommand,
        d: &MethodDescriptor,
        method: &str,
        params: Value,
        actor: &Actor,
        mode: OplogMode<'_>,
        check_modules: bool,
    ) -> AppResult<Value> {
        h.validate(&params)?;
        let now = self.clock.now();
        let locale = self.locale();
        self.writers_waiting.fetch_add(1, Ordering::SeqCst);
        let mut waiting = WaitingWriter(&self.writers_waiting, true);
        let mut ids = self
            .ids
            .lock()
            .map_err(|_| AppError::api(ErrorCode::Internal, "id generator poisoned"))?;
        let mut tx = self.store.write()?;
        waiting.acquired();
        let batch_id = match &mode {
            OplogMode::Defer(g) => g.batch_id,
            _ => BatchId::generate(&mut **ids),
        };
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
            check_modules,
        };
        let response = run_recorded(&mut ctx, d, h, method, params)?;
        let oplog: Vec<PendingOplogEntry> = std::mem::take(&mut ctx.oplog)
            .into_iter()
            .flatten()
            .collect();
        let events = std::mem::take(&mut ctx.events);
        let (kind, reverts) = (ctx.batch_kind, ctx.reverts_batch_id);
        match mode {
            OplogMode::Write => write_oplog(
                &mut *tx,
                &mut **ids,
                oplog,
                (batch_id, kind, reverts),
                actor,
                now,
            )?,
            OplogMode::Defer(group) => {
                // The marker learns this part in the part's own transaction.
                if !oplog.is_empty() {
                    tx.add_unfinished_batch_part(
                        group.batch_id,
                        group.parts,
                        &part_inverses(&oplog)?,
                    )?;
                    if group.marker.state != "writing" {
                        group.marker.state = "writing".into();
                        group.marker.updated_at = now;
                        tx.upsert_unfinished_batch(&group.marker)?;
                    }
                }
                tx.commit()?;
                if !oplog.is_empty() {
                    group.parts += 1;
                }
                group.pending.extend(oplog);
                drop(ids);
                self.after_commit(events, now, batch_id, actor);
                return Ok(response);
            }
            OplogMode::Discard(group) => {
                if let Some(batch) = group {
                    tx.delete_unfinished_batch(batch)?;
                }
            }
        }
        tx.commit()?;
        drop(ids);
        self.after_commit(events, now, batch_id, actor);
        Ok(response)
    }

    fn after_commit(
        &self,
        events: Vec<DomainEvent>,
        now: fc_core::Timestamp,
        batch_id: BatchId,
        actor: &Actor,
    ) {
        self.apply_committed(&events);
        self.plugins.queue_events(&events, actor);
        for event in events {
            let envelope = EventEnvelope {
                seq: self.event_seq.fetch_add(1, Ordering::SeqCst),
                at: now,
                batch_id: Some(batch_id),
                event,
            };
            self.events.publish(&envelope);
        }
    }
}

/// Appends `entries` as batch `batch` (`(id, kind, reverts)`), `seq` from 0.
fn write_oplog(
    tx: &mut dyn fc_storage::WriteTx,
    ids: &mut dyn IdGen,
    entries: Vec<PendingOplogEntry>,
    batch: (BatchId, OplogKind, Option<BatchId>),
    actor: &Actor,
    now: fc_core::Timestamp,
) -> AppResult<()> {
    let (batch_id, kind, reverts) = batch;
    let actor_json = serde_json::to_value(actor)
        .map_err(|e| AppError::api(ErrorCode::Internal, e.to_string()))?;
    for (seq, entry) in entries.into_iter().enumerate() {
        let record = OplogRecord {
            id: OplogId::generate(ids),
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
    Ok(())
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
