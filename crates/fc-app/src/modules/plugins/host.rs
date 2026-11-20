//! The plugin host: loaded plugins, hooks around the dispatcher, the host functions plugins
//! call (the public API filtered by permissions), failure accounting.
//!
//! **Where plugin code runs**
//!
//! | Context | Host access | Writes |
//! |---|---|---|
//! | `before:` hook (before the write transaction opens) | queries, own storage/settings, log | none (commands are refused) |
//! | `after:` / `event:` hook (after commit) | everything granted | one undo step per reaction; **no** hooks fire for them |
//! | operation (`plugins.run`, plugin commands, `plugins.ui.*`) | everything granted | one undo step for the whole operation; rolled back if the plugin fails; other plugins' `before:` hooks apply |
//! | card type / TTS engine | log only ([`PureHost`]) | none |
//!
//! Changes made by plugins never trigger `after:`/`event:` hooks (no cascades). A failing
//! plugin never stops the core operation (a failing `before:` hook = continue without it,
//! unless `plugins.before_hook_failure = reject`); faults (trap, fuel, deadline, memory, ABI)
//! are counted and the plugin is switched off after `plugins.max_failures` in a row.

use super::{model_store, settings_values};
use crate::app::{OplogMode, UndoGroup};
use crate::error::{AppError, AppResult};
use crate::registry::{ensure_invocable, ErasedHandler};
use crate::App;
use fc_api::method::parse_qualified_name;
use fc_api::v1::common::I18nMessage;
use fc_api::v1::events::DomainEvent;
use fc_api::v1::methods::{
    OplogRedo, OplogUndo, PluginsKvSet, PluginsRecordFailure, PluginsRollback,
};
use fc_api::v1::oplog::Actor;
use fc_api::v1::plugins::{
    LogLevel, PluginLogEntry, PluginNotice, RecordPluginFailure, SetPluginKv, UiText,
};
use fc_api::{ApiError, ApiMethod, ErrorCode, InvokeRequest};
use fc_core::settings::SettingDef;
use fc_plugins::abi::{host_fns, ops, BeforeHookReply, WireError};
use fc_plugins::{
    CompiledPlugin, HookPoint, HostCalls, Limits, Manifest, PluginError, Runtime,
};
use fc_storage::repo::PluginRecord;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::Duration;

/// Hook policy of one dispatch.
#[derive(Debug, Clone, Copy)]
pub(crate) enum Hooks<'a> {
    /// Run `before:`/`after:` hooks, skipping plugin `skip` (a plugin's own writes).
    Run {
        /// Plugin whose hooks do not run.
        skip: Option<&'a str>,
    },
    /// No hooks (trusted/internal calls, plugin reactions).
    Off,
}

/// Runtime settings (module `plugins`).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Config {
    pub limits: Limits,
    pub max_failures: u32,
    pub reject_on_hook_failure: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            limits: Limits::default(),
            max_failures: 3,
            reject_on_hook_failure: false,
        }
    }
}

/// An installed plugin as loaded in memory.
pub(crate) struct LoadedPlugin {
    pub record: PluginRecord,
    pub manifest: Manifest,
    /// Granted ∩ requested.
    pub granted: BTreeSet<String>,
    pub settings: Vec<SettingDef>,
    pub compiled: Option<Arc<CompiledPlugin>>,
    /// Why it cannot run (compile error…).
    pub broken: Option<String>,
    /// Enabled, not auto-disabled, compiled.
    pub active: bool,
}

impl LoadedPlugin {
    pub fn id(&self) -> &str {
        &self.record.id
    }

    pub fn has(&self, permission: &str) -> bool {
        self.granted.contains(permission)
    }

    fn actor(&self) -> Actor {
        Actor::Plugin(self.record.id.clone())
    }
}

/// State shared with engines registered in the registry (card types, TTS).
#[derive(Default)]
pub(crate) struct Core {
    runtime: OnceLock<Option<Arc<Runtime>>>,
    config: RwLock<Config>,
    counts: Mutex<HashMap<String, u32>>,
    pending: Mutex<Vec<RecordPluginFailure>>,
    logs: Mutex<HashMap<String, VecDeque<PluginLogEntry>>>,
}

const LOG_LINES: usize = 200;

impl Core {
    pub fn runtime(&self) -> Option<Arc<Runtime>> {
        self.runtime
            .get_or_init(|| Runtime::new().ok().map(Arc::new))
            .clone()
    }

    pub fn config(&self) -> Config {
        self.config.read().map(|c| *c).unwrap_or_default()
    }

    pub fn log(&self, plugin: &str, level: LogLevel, message: impl Into<String>, at: fc_core::Timestamp) {
        if let Ok(mut logs) = self.logs.lock() {
            let q = logs.entry(plugin.to_owned()).or_default();
            if q.len() == LOG_LINES {
                q.pop_front();
            }
            let mut message: String = message.into();
            message.truncate(2000);
            q.push_back(PluginLogEntry { at, level, message });
        }
    }

    pub fn logs(&self, plugin: &str) -> Vec<PluginLogEntry> {
        self.logs
            .lock()
            .map(|l| l.get(plugin).map(|q| q.iter().cloned().collect()).unwrap_or_default())
            .unwrap_or_default()
    }

    /// Counts the outcome of a call; faults in a row past the limit disable the plugin (the
    /// record is written by [`PluginHost::after_operation`], outside any transaction).
    pub fn record<T>(&self, plugin: &str, result: &Result<T, PluginError>) {
        let max = self.config().max_failures.max(1);
        let Ok(mut counts) = self.counts.lock() else {
            return;
        };
        let count = counts.entry(plugin.to_owned()).or_insert(0);
        let entry = match result {
            Ok(_) if *count > 0 => {
                *count = 0;
                RecordPluginFailure {
                    id: plugin.to_owned(),
                    failures: 0,
                    error: None,
                    disable: false,
                }
            }
            Err(e) if e.kind.is_fault() => {
                *count += 1;
                RecordPluginFailure {
                    id: plugin.to_owned(),
                    failures: *count,
                    error: Some(e.to_string()),
                    disable: *count >= max,
                }
            }
            _ => return,
        };
        if let Ok(mut p) = self.pending.lock() {
            p.retain(|r| r.id != entry.id || r.disable);
            p.push(entry);
        }
    }

    /// Runs `op` with no host access (engines).
    pub fn call_pure(
        &self,
        plugin: &LoadedPlugin,
        op: &str,
        input: Value,
    ) -> Result<Value, PluginError> {
        let r = match (self.runtime(), &plugin.compiled) {
            (Some(rt), Some(c)) => rt.call(c, op, input, &self.config().limits, &mut fc_plugins::runtime::PureHost),
            _ => Err(PluginError::new(
                fc_plugins::PluginErrorKind::Abi,
                "plugin is not loaded",
            )),
        };
        self.record(plugin.id(), &r);
        r
    }
}

#[derive(Default)]
struct Loaded {
    plugins: BTreeMap<String, Arc<LoadedPlugin>>,
    before: HashMap<String, Vec<String>>,
    after: HashMap<String, Vec<String>>,
    events: HashMap<String, Vec<String>>,
}

/// A declarative UI session.
pub(crate) struct Session {
    pub plugin_id: String,
    pub view: String,
    pub state: Value,
}

/// The plugin side of an [`App`].
#[derive(Default)]
pub struct PluginHost {
    pub(crate) core: Arc<Core>,
    loaded: RwLock<Loaded>,
    pub(crate) sessions: Mutex<HashMap<String, Session>>,
    next_session: AtomicU64,
    events: Mutex<Vec<DomainEvent>>,
}

/// Plain-text form of a wire error code.
fn code_str(code: ErrorCode) -> String {
    serde_json::to_value(code)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_else(|| "INTERNAL".into())
}

fn to_wire(e: AppError) -> WireError {
    let api = e.into_api();
    WireError {
        code: code_str(api.code),
        message: api.message,
        details: Value::Object(api.details),
    }
}

/// A plugin failure as an API error.
pub(crate) fn plugin_failed(plugin: &str, e: &PluginError) -> AppError {
    AppError::Api(
        ApiError::new(ErrorCode::ProviderPluginFailed, format!("plugin {plugin}: {e}"))
            .with_detail("plugin_id", plugin)
            .with_detail("kind", e.kind.code())
            .with_detail("code", e.code.clone())
            .with_detail("message", e.message.clone()),
    )
}

fn denied(func: &str, permission: &str, reason: &str) -> AppError {
    AppError::Api(
        ApiError::new(
            ErrorCode::PermissionDenied,
            format!("{func}: {reason}"),
        )
        .with_detail("function", func)
        .with_detail("permission", permission),
    )
}

fn read_settings_config(reads: &dyn fc_storage::Reads) -> Config {
    use fc_core::settings::SettingScope;
    let get = |key: &str| -> Option<Value> { reads.get_setting(&SettingScope::App, key).ok().flatten() };
    let int = |key: &str, default: u64| get(key).and_then(|v| v.as_u64()).unwrap_or(default);
    let d = Config::default();
    Config {
        limits: Limits {
            fuel: int(super::SETTING_FUEL, 2000).saturating_mul(1_000_000),
            timeout: Duration::from_millis(int(super::SETTING_TIMEOUT, 3000)),
            memory_bytes: usize::try_from(int(super::SETTING_MEMORY, 64))
                .unwrap_or(64)
                .saturating_mul(1024 * 1024),
        },
        max_failures: u32::try_from(int(super::SETTING_MAX_FAILURES, u64::from(d.max_failures)))
            .unwrap_or(d.max_failures),
        reject_on_hook_failure: get(super::SETTING_HOOK_FAILURE)
            .is_some_and(|v| v.as_str() == Some("reject")),
    }
}

impl PluginHost {
    /// Active plugin by id.
    pub(crate) fn get(&self, id: &str) -> Option<Arc<LoadedPlugin>> {
        self.loaded
            .read()
            .ok()?
            .plugins
            .get(id)
            .filter(|p| p.active)
            .cloned()
    }

    /// Any loaded plugin (inactive ones too).
    pub(crate) fn get_any(&self, id: &str) -> Option<Arc<LoadedPlugin>> {
        self.loaded.read().ok()?.plugins.get(id).cloned()
    }

    /// (Re)loads installed plugins from storage and syncs the registry: grants, card types,
    /// TTS providers, commands, hook index. Best effort: problems end up on the plugin
    /// (`broken`), never as an error.
    pub(crate) fn reload(&self, app: &App) {
        let registry = app.registry();
        let (rows, config) = match app.store().read() {
            Ok(tx) => (
                tx.list_plugins(false).unwrap_or_default(),
                read_settings_config(&*tx),
            ),
            Err(_) => return,
        };
        if let Ok(mut c) = self.core.config.write() {
            *c = config;
        }
        let previous: BTreeMap<String, Arc<LoadedPlugin>> = self
            .loaded
            .read()
            .map(|l| l.plugins.clone())
            .unwrap_or_default();
        let mut next = Loaded::default();
        for record in rows {
            let loaded = self.load_one(app, record, &previous);
            next.plugins.insert(loaded.id().to_owned(), Arc::new(loaded));
        }
        // Registry: drop everything of the previous generation, add the active ones.
        for id in previous.keys().chain(next.plugins.keys()) {
            let prefix = format!("plugin.{id}.");
            registry.unregister_card_types(&prefix);
            registry.unregister_tts(&prefix);
            registry.unregister_dynamic(&prefix);
            registry.set_plugin_grants(id, None);
        }
        if let Ok(mut counts) = self.core.counts.lock() {
            counts.clear();
        }
        let mut keep = Vec::new();
        for (id, p) in &next.plugins {
            if let Ok(mut counts) = self.core.counts.lock() {
                counts.insert(id.clone(), p.record.failures);
            }
            if !p.active {
                continue;
            }
            if let Some(c) = &p.compiled {
                keep.push(c.sha256().to_owned());
            }
            registry.set_plugin_grants(id, Some(p.granted.clone()));
            super::engines::register(registry, &self.core, p);
            for perm in &p.granted {
                let Ok(perm) = fc_plugins::Permission::parse(perm) else {
                    continue;
                };
                match perm.hook() {
                    Some(HookPoint::Before(m)) => next.before.entry(m).or_default().push(id.clone()),
                    Some(HookPoint::After(m)) => next.after.entry(m).or_default().push(id.clone()),
                    Some(HookPoint::Event(e)) => next.events.entry(e).or_default().push(id.clone()),
                    None => {}
                }
            }
        }
        if let Some(rt) = self.core.runtime() {
            let keep: Vec<&str> = keep.iter().map(String::as_str).collect();
            rt.retain(&keep);
        }
        if let Ok(mut l) = self.loaded.write() {
            *l = next;
        }
        // Sessions of plugins that went away end.
        if let Ok(mut s) = self.sessions.lock() {
            s.retain(|_, sess| self.get(&sess.plugin_id).is_some());
        }
    }

    fn load_one(
        &self,
        app: &App,
        record: PluginRecord,
        previous: &BTreeMap<String, Arc<LoadedPlugin>>,
    ) -> LoadedPlugin {
        let manifest: Result<Manifest, String> =
            serde_json::from_value(record.manifest.clone()).map_err(|e| e.to_string());
        let reuse = previous
            .get(&record.id)
            .and_then(|p| p.compiled.clone())
            .filter(|c| c.sha256() == record.package_sha256);
        let mut broken = None;
        let mut settings = Vec::new();
        let compiled = match app.store().read() {
            Ok(tx) => {
                let package = tx
                    .get_plugin_package(&record.id, &record.package_sha256, reuse.is_none())
                    .ok()
                    .flatten();
                if let Some(p) = &package {
                    settings = serde_json::from_value(p.settings.clone()).unwrap_or_default();
                }
                match (reuse, package) {
                    (Some(c), _) => Some(c),
                    (None, Some(p)) => match self.core.runtime() {
                        Some(rt) => match rt.compile(&p.wasm) {
                            Ok(c) => Some(c),
                            Err(e) => {
                                broken = Some(e.to_string());
                                None
                            }
                        },
                        None => {
                            broken = Some("plugin runtime unavailable".into());
                            None
                        }
                    },
                    (None, None) => {
                        broken = Some("package missing".into());
                        None
                    }
                }
            }
            Err(e) => {
                broken = Some(e.to_string());
                None
            }
        };
        let manifest = match manifest {
            Ok(m) => m,
            Err(e) => {
                broken = Some(format!("manifest: {e}"));
                Manifest {
                    id: record.id.clone(),
                    name: record.id.clone(),
                    version: record.version.clone(),
                    api_version: String::new(),
                    authors: vec![],
                    description: String::new(),
                    entry: String::new(),
                    min_app_version: None,
                    permissions: vec![],
                    contributes: Default::default(),
                }
            }
        };
        let requested: BTreeSet<&str> = manifest.permissions.iter().map(|p| p.as_str()).collect();
        let granted = record
            .granted
            .iter()
            .filter(|g| requested.contains(g.as_str()))
            .cloned()
            .collect();
        let active = record.enabled && !record.auto_disabled && compiled.is_some() && broken.is_none();
        if let Some(b) = &broken {
            self.core
                .log(&record.id, LogLevel::Error, format!("cannot load: {b}"), app.now());
        }
        LoadedPlugin {
            record,
            manifest,
            granted,
            settings,
            compiled,
            broken,
            active,
        }
    }

    /// Runs an operation with full host access (see the module table).
    pub(crate) fn run_op(
        &self,
        app: &App,
        plugin: &Arc<LoadedPlugin>,
        op: &str,
        input: Value,
        mode: Mode,
    ) -> (Result<Value, PluginError>, Vec<PluginNotice>) {
        let (Some(rt), Some(compiled)) = (self.core.runtime(), plugin.compiled.clone()) else {
            return (
                Err(PluginError::new(
                    fc_plugins::PluginErrorKind::Abi,
                    "plugin is not loaded",
                )),
                vec![],
            );
        };
        let limits = self.core.config().limits;
        let mut host = OpHost {
            app,
            plugin,
            mode,
            group: None,
            notices: Vec::new(),
        };
        let r = rt.call(&compiled, op, input, &limits, &mut host);
        let notices = host.finish(r.is_ok());
        self.core.record(plugin.id(), &r);
        if let Err(e) = &r {
            self.core
                .log(plugin.id(), LogLevel::Error, format!("{op}: {e}"), app.now());
        }
        (r, notices)
    }

    /// New UI session id.
    pub(crate) fn new_session_id(&self) -> String {
        format!("s{}", self.next_session.fetch_add(1, Ordering::SeqCst) + 1)
    }

    /// `before:<method>` hooks: may replace params or reject the call.
    pub(crate) fn before_hooks(
        &self,
        app: &App,
        method: &str,
        mut params: Value,
        _actor: &Actor,
        skip: Option<&str>,
    ) -> AppResult<Value> {
        let ids = match self.loaded.read() {
            Ok(l) => match l.before.get(method) {
                Some(ids) => ids.clone(),
                None => return Ok(params),
            },
            Err(_) => return Ok(params),
        };
        let hook = format!("before:{method}");
        for id in ids {
            if skip == Some(id.as_str()) {
                continue;
            }
            let Some(plugin) = self.get(&id) else {
                continue;
            };
            let input = json!({ "hook": hook, "method": method, "params": params.clone() });
            let (r, _) = self.run_op(app, &plugin, ops::HOOK_BEFORE, input, Mode::Hook);
            let decided = r.and_then(|v| {
                serde_json::from_value::<BeforeHookReply>(v).map_err(|e| {
                    PluginError::new(fc_plugins::PluginErrorKind::Abi, format!("{hook}: {e}"))
                })
            });
            match decided {
                Ok(BeforeHookReply::Continue { params: Some(p) }) => params = p,
                Ok(BeforeHookReply::Continue { params: None }) => {}
                Ok(BeforeHookReply::Reject { message }) => {
                    return Err(AppError::Api(
                        ApiError::new(
                            ErrorCode::ConflictPluginRejected,
                            format!("plugin {id} rejected {method}: {message}"),
                        )
                        .with_detail("plugin_id", id.clone())
                        .with_detail("message", message),
                    ));
                }
                Err(e) => {
                    if self.core.config().reject_on_hook_failure {
                        return Err(plugin_failed(&id, &e));
                    }
                }
            }
        }
        Ok(params)
    }

    /// Whether any plugin subscribed to `after:<method>`.
    pub(crate) fn has_after_hooks(&self, method: &str) -> bool {
        self.loaded
            .read()
            .is_ok_and(|l| l.after.contains_key(method))
    }

    /// `after:<method>` hooks (only for calls not made by plugins).
    pub(crate) fn after_hooks(
        &self,
        app: &App,
        method: &str,
        params: &Value,
        response: &Value,
        actor: &Actor,
    ) {
        if matches!(actor, Actor::Plugin(_)) {
            return;
        }
        let ids = self
            .loaded
            .read()
            .ok()
            .and_then(|l| l.after.get(method).cloned())
            .unwrap_or_default();
        for id in ids {
            if let Some(plugin) = self.get(&id) {
                let input = json!({
                    "hook": format!("after:{method}"),
                    "method": method,
                    "params": params,
                    "response": response,
                });
                let _ = self.run_op(app, &plugin, ops::HOOK_AFTER, input, Mode::Reaction);
            }
        }
    }

    /// Keeps committed events for `event:` hooks (not those caused by plugins).
    pub(crate) fn queue_events(&self, events: &[DomainEvent], actor: &Actor) {
        if matches!(actor, Actor::Plugin(_)) || events.is_empty() {
            return;
        }
        let Ok(l) = self.loaded.read() else {
            return;
        };
        if l.events.is_empty() {
            return;
        }
        let wanted: Vec<DomainEvent> = events
            .iter()
            .filter(|e| event_type(e).is_some_and(|t| l.events.contains_key(&t)))
            .cloned()
            .collect();
        drop(l);
        if let Ok(mut q) = self.events.lock() {
            q.extend(wanted);
        }
    }

    fn dispatch_events(&self, app: &App) {
        let queued = match self.events.lock() {
            Ok(mut q) if !q.is_empty() => std::mem::take(&mut *q),
            _ => return,
        };
        for event in queued {
            let Some(kind) = event_type(&event) else {
                continue;
            };
            let ids = self
                .loaded
                .read()
                .ok()
                .and_then(|l| l.events.get(&kind).cloned())
                .unwrap_or_default();
            let value = serde_json::to_value(&event).unwrap_or(Value::Null);
            for id in ids {
                if let Some(plugin) = self.get(&id) {
                    let input = json!({ "hook": format!("event:{kind}"), "event": value });
                    let _ = self.run_op(app, &plugin, ops::HOOK_EVENT, input, Mode::Reaction);
                }
            }
        }
    }

    fn flush_failures(&self, app: &App) {
        let pending = match self.core.pending.lock() {
            Ok(mut p) if !p.is_empty() => std::mem::take(&mut *p),
            _ => return,
        };
        for record in pending {
            let request = InvokeRequest {
                method: format!("{}.{}", fc_api::API_VERSION, PluginsRecordFailure::NAME),
                params: serde_json::to_value(&record).unwrap_or(Value::Null),
            };
            // Best effort: the plugin may have been uninstalled meanwhile.
            let _ = app.invoke_trusted(&request, &Actor::System);
        }
    }

    /// End of a top-level dispatch: delivers queued events to `event:` hooks, then writes
    /// failure counts (and automatic disables).
    pub(crate) fn after_operation(&self, app: &App) {
        self.dispatch_events(app);
        self.flush_failures(app);
    }

    /// Active plugins.
    pub(crate) fn active(&self) -> Vec<Arc<LoadedPlugin>> {
        self.loaded
            .read()
            .map(|l| l.plugins.values().filter(|p| p.active).cloned().collect())
            .unwrap_or_default()
    }
}

/// Wire name of a domain event (`note_created`).
pub(crate) fn event_type(e: &DomainEvent) -> Option<String> {
    serde_json::to_value(e)
        .ok()?
        .get("type")?
        .as_str()
        .map(str::to_owned)
}

/// What a plugin may do from where it runs (see the module table).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    /// `before:` hook.
    Hook,
    /// `after:` / `event:` hook.
    Reaction,
    /// User-started operation.
    Operation,
}

/// Host functions of one plugin call.
pub(crate) struct OpHost<'a> {
    app: &'a App,
    plugin: &'a Arc<LoadedPlugin>,
    mode: Mode,
    group: Option<UndoGroup>,
    notices: Vec<PluginNotice>,
}

#[derive(serde::Deserialize)]
struct InvokeArgs {
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(serde::Deserialize)]
struct KeyArgs {
    key: String,
    #[serde(default)]
    value: Option<Value>,
}

impl OpHost<'_> {
    fn args<T: serde::de::DeserializeOwned>(func: &str, args: Value) -> AppResult<T> {
        serde_json::from_value(args).map_err(|e| {
            AppError::Api(
                ApiError::new(ErrorCode::ValidationInvalidParams, format!("{func}: {e}"))
                    .with_detail("function", func),
            )
        })
    }

    fn require(&self, func: &str, permission: &str) -> AppResult<()> {
        if self.plugin.has(permission) {
            Ok(())
        } else {
            Err(denied(func, permission, "permission not granted"))
        }
    }

    fn group(&mut self) -> AppResult<&mut UndoGroup> {
        if self.group.is_none() {
            let rollback = InvokeRequest {
                method: format!("{}.{}", fc_api::API_VERSION, PluginsRollback::NAME),
                params: json!({}),
            };
            self.group = Some(self.app.begin_group(None, &rollback)?);
        }
        self.group
            .as_mut()
            .ok_or_else(|| AppError::api(ErrorCode::Internal, "undo group missing"))
    }

    /// Commits the operation's undo step, or rolls its writes back if the plugin failed.
    fn finish(&mut self, ok: bool) -> Vec<PluginNotice> {
        if let Some(group) = self.group.take() {
            let actor = self.plugin.actor();
            if ok || group.is_empty() {
                let summary = I18nMessage::key("oplog.summary.plugins.run")
                    .param("plugin", &self.plugin.manifest.name);
                let _ = self.app.commit_group(&group, &actor, Some(summary));
            } else {
                let request = InvokeRequest {
                    method: format!("{}.{}", fc_api::API_VERSION, PluginsRollback::NAME),
                    params: json!({ "calls": group.inverses_rev() }),
                };
                let _ = self.app.invoke_with(
                    &request,
                    &Actor::System,
                    OplogMode::Discard(Some(group.batch_id())),
                );
            }
        }
        std::mem::take(&mut self.notices)
    }

    fn invoke(&mut self, method: &str, params: Value) -> AppResult<Value> {
        let app = self.app;
        let name = parse_qualified_name(method)
            .ok_or_else(|| crate::registry::unknown_method(method))?;
        let actor = self.plugin.actor();
        let Some(entry) = app.registry().entry(name) else {
            if app.registry().dynamic_entry(name).is_some() {
                return Err(denied(method, "", "plugins cannot run plugin operations"));
            }
            return Err(crate::registry::unknown_method(method));
        };
        let d = &entry.descriptor;
        ensure_invocable(app.registry(), d, &actor, app.is_module_enabled(d.module))?;
        if [OplogUndo::NAME, OplogRedo::NAME].contains(&d.name) {
            return Err(denied(method, d.permission, "plugins cannot undo or redo"));
        }
        match &entry.handler {
            ErasedHandler::Query(_) => app.invoke_hooked(method, params, &actor, true, crate::modules::plugins::Hooks::Off),
            ErasedHandler::Action(_) => Err(denied(method, d.permission, "plugins cannot run plugin operations")),
            ErasedHandler::Command(_) => {
                if self.mode == Mode::Hook {
                    return Err(denied(
                        method,
                        d.permission,
                        "a before: hook may not write; return changed params instead",
                    ));
                }
                let params = if self.mode == Mode::Operation {
                    app.plugins()
                        .before_hooks(app, name, params, &actor, Some(self.plugin.id()))?
                } else {
                    params
                };
                let request = InvokeRequest {
                    method: method.to_owned(),
                    params,
                };
                let group = self.group()?;
                app.invoke_with(&request, &actor, OplogMode::Defer(group))
            }
        }
    }

    fn kv_write(&self, key: String, value: Option<Value>) -> AppResult<Value> {
        let request = InvokeRequest {
            method: format!("{}.{}", fc_api::API_VERSION, PluginsKvSet::NAME),
            params: serde_json::to_value(SetPluginKv {
                plugin_id: self.plugin.id().to_owned(),
                key,
                value,
            })
            .map_err(|e| AppError::api(ErrorCode::Internal, e.to_string()))?,
        };
        self.app
            .invoke_trusted(&request, &self.plugin.actor())
            .map_err(AppError::Api)
    }

    fn dispatch(&mut self, func: &str, args: Value) -> AppResult<Value> {
        let id = self.plugin.id().to_owned();
        match func {
            host_fns::INVOKE => {
                let a: InvokeArgs = Self::args(func, args)?;
                self.invoke(&a.method, a.params)
            }
            host_fns::KV_GET => {
                let a: KeyArgs = Self::args(func, args)?;
                let tx = self.app.store().read()?;
                Ok(tx.get_plugin_kv(&id, &a.key)?.unwrap_or(Value::Null))
            }
            host_fns::KV_LIST => {
                #[derive(serde::Deserialize)]
                struct Prefix {
                    #[serde(default)]
                    prefix: String,
                }
                let a: Prefix = Self::args(func, args)?;
                let tx = self.app.store().read()?;
                Ok(json!(tx.list_plugin_kv(&id, &a.prefix)?))
            }
            host_fns::KV_SET => {
                let a: KeyArgs = Self::args(func, args)?;
                self.kv_write(a.key, Some(a.value.unwrap_or(Value::Null)))
            }
            host_fns::KV_DELETE => {
                let a: KeyArgs = Self::args(func, args)?;
                self.kv_write(a.key, None)
            }
            host_fns::SETTINGS_GET => {
                let tx = self.app.store().read()?;
                Ok(json!(settings_values(&*tx, &id, &self.plugin.settings)?))
            }
            host_fns::LOG => {
                #[derive(serde::Deserialize)]
                struct Log {
                    #[serde(default = "info")]
                    level: LogLevel,
                    message: String,
                }
                fn info() -> LogLevel {
                    LogLevel::Info
                }
                let a: Log = Self::args(func, args)?;
                self.app.plugins().core.log(&id, a.level, a.message, self.app.now());
                Ok(Value::Null)
            }
            host_fns::UI_NOTIFY => {
                #[derive(serde::Deserialize)]
                struct Notify {
                    text: UiText,
                    #[serde(default = "info")]
                    level: LogLevel,
                }
                fn info() -> LogLevel {
                    LogLevel::Info
                }
                let a: Notify = Self::args(func, args)?;
                let notice = PluginNotice {
                    plugin_id: id,
                    level: a.level,
                    text: a.text,
                };
                self.app.publish(
                    vec![DomainEvent::PluginNotice {
                        notice: notice.clone(),
                    }],
                    None,
                );
                self.notices.push(notice);
                Ok(Value::Null)
            }
            host_fns::TTS_VOICES => self.invoke("v1.tts.voices", args),
            host_fns::TTS_SYNTHESIZE => self.invoke("v1.tts.synthesize", args),
            host_fns::HTTP_FETCH => {
                #[derive(serde::Deserialize)]
                struct Fetch {
                    url: String,
                }
                let a: Fetch = Self::args(func, args)?;
                let host = a
                    .url
                    .split_once("://")
                    .map(|(_, rest)| rest)
                    .unwrap_or("")
                    .split(['/', '?', '#', ':'])
                    .next()
                    .unwrap_or("")
                    .to_lowercase();
                self.require(func, &format!("network:{host}"))?;
                Err(AppError::Api(
                    ApiError::new(
                        ErrorCode::ProviderUnavailable,
                        "this build has no HTTP client for plugins",
                    )
                    .with_detail("provider", "http"),
                ))
            }
            host_fns::AI_COMPLETE => {
                self.require(func, "ai:invoke")?;
                Err(AppError::Api(
                    ApiError::new(ErrorCode::ProviderUnavailable, "no AI provider registered")
                        .with_detail("provider", "ai"),
                ))
            }
            host_fns::MODELS_LIST => {
                self.require(func, "models:manage")?;
                let store = model_store(self.app);
                let models: Vec<Value> = self
                    .plugin
                    .manifest
                    .contributes
                    .models
                    .iter()
                    .map(|m| {
                        let status = store
                            .as_ref()
                            .and_then(|s| s.status(&id, m).ok())
                            .map(|s| serde_json::to_value(s).unwrap_or(Value::Null))
                            .unwrap_or(json!({ "state": "missing" }));
                        let path = store
                            .as_ref()
                            .and_then(|s| s.path(&id, &m.id).ok())
                            .filter(|p| p.is_file())
                            .map(|p| p.display().to_string());
                        json!({ "id": m.id, "status": status, "path": path })
                    })
                    .collect();
                Ok(json!(models))
            }
            other => Err(AppError::Api(
                ApiError::new(ErrorCode::NotFoundMethod, format!("unknown host function {other}"))
                    .with_detail("method", other),
            )),
        }
    }
}

impl HostCalls for OpHost<'_> {
    fn call(&mut self, func: &str, args: Value) -> Result<Value, WireError> {
        self.dispatch(func, args).map_err(to_wire)
    }
}
