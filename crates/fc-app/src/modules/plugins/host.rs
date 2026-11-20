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
//! Plugin storage (`kv.*`) and the plugin's own settings (`settings.set`) are written into the
//! same undo group: a failing call rolls them back with everything else. A call that only
//! touched its own storage/settings leaves no undo step (plugin bookkeeping is not the user's
//! history); one that also changed user data undoes both together.
//!
//! **What plugins never reach**, whatever they were granted: plugin management, app
//! settings and module switches (so never their own sandbox limits), undo/redo, free file
//! paths (`io.*`; files come through `fs:pick`), MCP servers, AI keys — see
//! `fc_plugins::permission::FORBIDDEN_API_AREAS` — and hooks on those methods
//! (`FORBIDDEN_HOOK_PREFIXES`). A hook also needs the hooked method's own permission.
//!
//! Changes made by plugins never trigger `after:`/`event:` hooks (no cascades): plugin
//! writes, and the rollback of a failed plugin call, run as the plugin's actor; and nothing
//! that happens *while* a hook runs (whatever the actor) is queued for `event:` hooks.
//! **Hook budget, per plugin:** in one user action *each* plugin may run at most
//! `plugins.max_hooks_per_action` hooks, for at most `plugins.hook_ms_per_action` together
//! (its own budget; no plugin can spend another's). A plugin past its budget runs no more
//! hooks in that action — the others run on — and the user gets one notice for the action
//! listing every plugin whose hooks were cut short (`plugin_hooks_skipped`). Exhausting a
//! budget never switches a plugin off: on a user's action the volume is the user's (a bulk
//! edit of 1000 notes is fine); only a plugin's own faults (trap, timeout, fuel, memory, ABI,
//! `plugins.max_failures` in a row) do. Inside a plugin's operation every hook is charged to
//! that plugin ([`initiator`]), and its call past its budget is refused rather than run
//! without the other plugins' `before:` hooks. No hook may start inside another
//! ([`Config::max_hook_depth`]): that ends the action's hooks. A plugin whose hook *faulted*
//! runs no more hooks in that action. So a user action waits on each plugin at most its hook
//! time budget plus one call's deadline.
//!
//! **Initiator:** every budget is charged to the plugin that started the chain, never to one
//! it ran into: the hook budget of its operation, the per-minute host call windows of hooks
//! run inside its calls, and faults of other plugins' hooks and engines inside its calls
//! (logged, not counted against them).
//!
//! **What plugins see:** every value handed to a plugin (results, errors, hook payloads,
//! events, jobs) is filtered by its read permissions in [`super::redact`].
//!
//! **Host resources** (docs/05 §12): every host call is checked against
//! `fc_plugins::limits` — request/reply size and count per plugin call (by the runtime),
//! count per minute per plugin (here, [`RateWindows`]); every plugin call has a deadline that
//! includes host time, a cap on host calls and on bytes moved (`plugins.timeout_ms`,
//! `plugins.max_host_calls`, `plugins.host_mb_per_call`); database statements of a host call
//! stop at that deadline (`fc_storage::interrupt`), and whole-collection methods are not
//! callable ([`super::redact::Exposure::Refused`]). What the host keeps for a plugin
//! is bounded too: its log (a ring of short lines), its storage (quota), UI sessions (size and
//! number), queued events (at most the action's hook budget). A failing
//! plugin never stops the core operation (a failing `before:` hook = continue without it,
//! unless `plugins.before_hook_failure = reject`); faults (trap, fuel, deadline, memory, ABI)
//! are counted and the plugin is switched off after `plugins.max_failures` in a row.

use super::{model_store, redact, settings_values};
use crate::app::{OplogMode, UndoGroup};
use crate::error::{AppError, AppResult};
use crate::registry::{ensure_invocable, ErasedHandler, Registry};
use crate::App;
use fc_api::method::parse_qualified_name;
use fc_api::v1::common::I18nMessage;
use fc_api::v1::events::DomainEvent;
use fc_api::v1::methods::{
    OplogRedo, OplogUndo, PluginsKvSet, PluginsRecordFailure, PluginsRollback,
    PluginsSettingsSet,
};
use fc_api::v1::oplog::Actor;
use fc_api::v1::plugins::{
    LogLevel, PluginLogEntry, PluginNotice, RecordPluginFailure, SetPluginKv, SetPluginSetting,
    UiText,
};
use fc_api::{ApiError, ApiMethod, ErrorCode, InvokeRequest};
use fc_core::settings::SettingDef;
use fc_plugins::abi::{host_fns, ops, BeforeHookReply, WireError};
use fc_plugins::limits::{RateWindows, KV_LIST_MAX_BYTES, KV_LIST_MAX_ENTRIES};
use fc_plugins::{
    CompiledPlugin, HookPoint, HostCalls, Limits, Manifest, PluginError, Runtime,
};
use fc_storage::repo::PluginRecord;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::thread::ThreadId;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// The native file dialog behind `fs:pick` (set by the desktop shell with
/// [`App::set_file_picker`]). It returns the path the user chose; the host reads or writes
/// the file itself and hands the plugin only the file name and bytes.
pub trait FilePicker: Send + Sync {
    /// An existing file to open (`None` = cancelled). `extensions` filter the dialog
    /// (`["csv"]`; empty = any).
    fn pick_open(&self, extensions: &[String]) -> Option<PathBuf>;
    /// Where to save a new file named like `suggested_name` (`None` = cancelled).
    fn pick_save(&self, suggested_name: &str, extensions: &[String]) -> Option<PathBuf>;
}

/// Largest file a plugin may open or save through `fs:pick` (base64 must fit one ABI
/// message).
pub const MAX_PICKED_FILE_BYTES: u64 = 8 * 1024 * 1024;

/// Whether `plugin` may hook command `method` now: a public command, not one of the methods
/// no plugin may hook, and the plugin holds the method's own permission (a hook never gives
/// more than calling the method would) and the read permissions of the data the hook sees
/// (the user's params and the response; [`redact::exposure`]). A method no plugin may call
/// ([`redact::Exposure::Refused`]: free file paths, programs, whole-collection work) cannot
/// be hooked either: a `before:` hook's params run with the user's authority.
pub(crate) fn hook_allowed(registry: &Registry, plugin: &LoadedPlugin, method: &str) -> bool {
    registry.descriptor(method).is_some_and(|d| {
        d.visibility == fc_api::Visibility::Public
            && d.kind == fc_api::MethodKind::Command
            && fc_plugins::permission::hook_target_allowed(method, d.permission).is_ok()
            && plugin.has(d.permission)
            && !matches!(redact::exposure(method), redact::Exposure::Refused(_))
            && redact::can_read(&plugin.granted, redact::exposure(method).domains())
    })
}

thread_local! {
    /// Plugins whose operations run on this thread, outermost first (host calls are
    /// serviced on the thread that started the plugin call, so nested work lands here).
    static CHAIN: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// The plugin that started the work running on this thread (`None` = the user or the
/// app). Every budget is charged to it: the hook budget of its operation, per-minute host call
/// windows, and faults of other plugins' hooks and engines its calls ran into.
pub(crate) fn initiator() -> Option<String> {
    CHAIN.with(|c| c.borrow().first().cloned())
}

/// Who pays for work `plugin` does now: the chain's initiator, else `plugin` itself.
fn payer(plugin: &str) -> String {
    initiator().unwrap_or_else(|| plugin.to_owned())
}

/// Whether `plugin` runs inside a chain another plugin started (its faults are then not
/// its own to count: the initiator chose its input).
fn foreign_chain(plugin: &str) -> Option<String> {
    initiator().filter(|i| i != plugin)
}

/// Membership of a plugin operation in [`CHAIN`].
struct ChainGuard;

impl ChainGuard {
    fn enter(plugin: &str) -> Self {
        CHAIN.with(|c| c.borrow_mut().push(plugin.to_owned()));
        Self
    }
}

impl Drop for ChainGuard {
    fn drop(&mut self) {
        CHAIN.with(|c| {
            c.borrow_mut().pop();
        });
    }
}

/// The refusal of a plugin's call once its hook budget for the action is spent.
fn hook_budget_refused(method: &str) -> AppError {
    AppError::Api(
        ApiError::new(
            ErrorCode::ValidationPluginHostLimit,
            format!("{method}: this plugin's hook budget for the action is spent"),
        )
        .with_detail("function", host_fns::INVOKE)
        .with_detail("limit", "hook_budget")
        .with_detail("method", method),
    )
}

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
    /// Hooks one user action may run, per plugin.
    pub max_hooks_per_action: u32,
    /// Hooks that may be running at once on one thread (1 = a hook never starts inside
    /// another; nothing legitimate nests them).
    pub max_hook_depth: u32,
    /// Wall-clock time the hooks of one plugin may take together in one user action.
    pub hook_time_per_action: Duration,
    /// Private storage per plugin: entries.
    pub kv_max_keys: u64,
    /// Private storage per plugin: bytes (keys + JSON values).
    pub kv_max_bytes: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            limits: Limits::default(),
            max_failures: 3,
            reject_on_hook_failure: false,
            max_hooks_per_action: 200,
            max_hook_depth: 1,
            hook_time_per_action: Duration::from_secs(10),
            kv_max_keys: 10_000,
            kv_max_bytes: 16 * 1024 * 1024,
        }
    }
}

/// What one payer's hooks used in an action.
#[derive(Debug, Default, Clone, Copy)]
struct Spent {
    hooks: u32,
    time: Duration,
}

/// Hook accounting of the user action running on one thread (host calls are serviced on
/// the thread that started the action, so nested work lands here too).
#[derive(Debug, Default)]
struct ActionState {
    /// Open [`ActionScope`]s (nested dispatches).
    scopes: u32,
    /// Payers of the hooks running now, outermost first.
    running: Vec<String>,
    /// Budget used so far in this action, per payer ([`payer`]: the hook's own plugin on a
    /// user's action, the initiator inside a plugin's operation).
    spent: HashMap<String, Spent>,
    /// Plugins whose hook faulted in this action (they run no more hooks in it).
    faulted: BTreeSet<String>,
    /// Payers whose budget is spent: their remaining hooks in this action are skipped, and
    /// the user is told once at the end of the action.
    exhausted: BTreeSet<String>,
    /// A hook started inside another: no more hooks in this action.
    tripped: bool,
}

/// One dispatch of [`App::invoke_hooked`]; the outermost one is the user action. When it
/// ends, the user is told which plugins' hooks were cut short (one notice per action).
pub(crate) struct ActionScope<'a>(&'a App);

impl Drop for ActionScope<'_> {
    fn drop(&mut self) {
        let skipped = self
            .0
            .plugins()
            .with_action(|a| {
                a.scopes = a.scopes.saturating_sub(1);
                if a.scopes == 0 {
                    std::mem::take(&mut a.exhausted)
                } else {
                    BTreeSet::new()
                }
            })
            .unwrap_or_default();
        if skipped.is_empty() {
            return;
        }
        let config = self.0.plugins().core.config();
        self.0.publish(
            vec![DomainEvent::PluginHooksSkipped {
                plugin_ids: skipped.into_iter().collect(),
                notice: I18nMessage::key("plugins.notice.hooks_skipped")
                    .param("max", config.max_hooks_per_action.to_string())
                    .param("ms", config.hook_time_per_action.as_millis().to_string()),
            }],
            None,
        );
    }
}

/// A running hook (see [`PluginHost::enter_hook`]); its time counts against its payer's
/// budget.
pub(crate) struct HookGuard<'a> {
    host: &'a PluginHost,
    started: Instant,
    payer: String,
}

impl Drop for HookGuard<'_> {
    fn drop(&mut self) {
        let took = self.started.elapsed();
        self.host.with_action(|a| {
            a.running.pop();
            let spent = a.spent.entry(std::mem::take(&mut self.payer)).or_default();
            spent.time = spent.time.saturating_add(took);
        });
    }
}

/// Whether a hook may run ([`PluginHost::enter_hook`]).
pub(crate) enum Admission<'a> {
    /// Run it; the guard ends it.
    Run(HookGuard<'a>),
    /// Not this plugin (its hook faulted earlier in this action); others may run.
    Skip,
    /// Its payer's hook budget for this action is spent: skip it (others may run); a
    /// plugin's own call is refused instead.
    Spent,
    /// No more hooks in this action (a hook inside a hook).
    Stop,
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
    actions: Mutex<HashMap<ThreadId, ActionState>>,
    logs: Mutex<HashMap<String, VecDeque<PluginLogEntry>>>,
    /// Per-minute host call counters, per plugin.
    rates: Mutex<HashMap<String, RateWindows>>,
}

const LOG_LINES: usize = 200;
/// Longest log line kept, bytes.
const LOG_LINE_BYTES: usize = 2000;

/// `s` cut to at most `max` bytes, on a character boundary.
fn truncate_utf8(s: &mut String, max: usize) {
    if s.len() > max {
        let mut end = max;
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        s.truncate(end);
    }
}

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
            truncate_utf8(&mut message, LOG_LINE_BYTES);
            q.push_back(PluginLogEntry { at, level, message });
        }
    }

    /// Counts a host call of `plugin` against its per-minute limits.
    fn admit_rate(&self, plugin: &str, func: &str) -> Result<(), WireError> {
        match self.rates.lock() {
            Ok(mut r) => r.entry(plugin.to_owned()).or_default().admit(func, Instant::now()),
            Err(_) => Ok(()),
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
                    reason: None,
                }
            }
            Err(e) if e.kind.is_fault() => {
                *count += 1;
                RecordPluginFailure {
                    id: plugin.to_owned(),
                    failures: *count,
                    error: Some(e.to_string()),
                    disable: *count >= max,
                    reason: None,
                }
            }
            _ => return,
        };
        drop(counts);
        self.push_pending(entry);
    }

    fn push_pending(&self, entry: RecordPluginFailure) {
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
        self.record_own(plugin.id(), &r, op, None);
        r
    }

    /// [`Core::record`], unless `plugin` ran inside a chain another plugin started: then a
    /// fault is logged (with who started it) but not counted — the initiator chose the input,
    /// and a plugin must not be able to switch another one off by feeding it. `at` = when, for
    /// the log line (`None` = no line: engines have no clock at hand).
    pub fn record_own<T>(
        &self,
        plugin: &str,
        result: &Result<T, PluginError>,
        op: &str,
        at: Option<fc_core::Timestamp>,
    ) {
        match (foreign_chain(plugin), result, at) {
            (None, ..) => self.record(plugin, result),
            (Some(by), Err(e), Some(at)) if e.kind.is_fault() => self.log(
                plugin,
                LogLevel::Warn,
                format!("{op}: {e} (in a call started by plugin {by}; not counted)"),
                at,
            ),
            (Some(_), ..) => {}
        }
    }
}

#[derive(Default)]
struct Loaded {
    plugins: BTreeMap<String, Arc<LoadedPlugin>>,
    before: HashMap<String, Vec<String>>,
    after: HashMap<String, Vec<String>>,
    events: HashMap<String, Vec<String>>,
}

/// Committed events waiting for `event:` hooks, with how many each subscribed plugin has.
#[derive(Default)]
struct EventQueue {
    events: Vec<DomainEvent>,
    per_plugin: HashMap<String, usize>,
}

impl EventQueue {
    fn clear(&mut self) {
        self.events.clear();
        self.per_plugin.clear();
    }
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
    events: Mutex<EventQueue>,
    picker: RwLock<Option<Arc<dyn FilePicker>>>,
}

/// Plain-text form of a wire error code.
pub(crate) fn code_str(code: ErrorCode) -> String {
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

pub(crate) fn read_settings_config(reads: &dyn fc_storage::Reads) -> Config {
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
            max_host_calls: u32::try_from(int(
                super::SETTING_MAX_HOST_CALLS,
                u64::from(d.limits.max_host_calls),
            ))
            .unwrap_or(d.limits.max_host_calls),
            host_bytes: usize::try_from(int(
                super::SETTING_HOST_MB,
                (d.limits.host_bytes / (1024 * 1024)) as u64,
            ))
            .unwrap_or(64)
            .saturating_mul(1024 * 1024),
        },
        max_failures: u32::try_from(int(super::SETTING_MAX_FAILURES, u64::from(d.max_failures)))
            .unwrap_or(d.max_failures),
        reject_on_hook_failure: get(super::SETTING_HOOK_FAILURE)
            .is_some_and(|v| v.as_str() == Some("reject")),
        max_hooks_per_action: u32::try_from(int(
            super::SETTING_MAX_HOOKS,
            u64::from(d.max_hooks_per_action),
        ))
        .unwrap_or(d.max_hooks_per_action),
        max_hook_depth: d.max_hook_depth,
        hook_time_per_action: Duration::from_millis(int(
            super::SETTING_HOOK_TIME,
            d.hook_time_per_action.as_millis() as u64,
        )),
        kv_max_keys: int(super::SETTING_KV_MAX_KEYS, d.kv_max_keys),
        kv_max_bytes: int(super::SETTING_KV_MAX_MB, d.kv_max_bytes / (1024 * 1024))
            .saturating_mul(1024 * 1024),
    }
}

impl PluginHost {
    /// See [`App::set_file_picker`].
    pub(crate) fn set_file_picker(&self, picker: Arc<dyn FilePicker>) {
        if let Ok(mut p) = self.picker.write() {
            *p = Some(picker);
        }
    }

    fn picker(&self) -> Option<Arc<dyn FilePicker>> {
        self.picker.read().ok().and_then(|p| p.clone())
    }

    /// Runs `f` on this thread's action state; drops the state once nothing uses it.
    fn with_action<T>(&self, f: impl FnOnce(&mut ActionState) -> T) -> Option<T> {
        let mut actions = self.core.actions.lock().ok()?;
        let id = std::thread::current().id();
        let state = actions.entry(id).or_default();
        let out = f(state);
        if state.scopes == 0 && state.running.is_empty() {
            actions.remove(&id);
        }
        Some(out)
    }

    /// Opens a dispatch; the outermost one on a thread starts fresh hook budgets.
    pub(crate) fn begin_action<'a>(&self, app: &'a App) -> ActionScope<'a> {
        self.with_action(|a| a.scopes += 1);
        ActionScope(app)
    }

    /// Events waiting for `event:` hooks.
    #[cfg(test)]
    pub(crate) fn queued_events(&self) -> usize {
        self.events.lock().map(|q| q.events.len()).unwrap_or(0)
    }

    /// Plugins waiting to be switched off at the end of the action.
    #[cfg(test)]
    pub(crate) fn pending_disables(&self) -> Vec<String> {
        self.core
            .pending
            .lock()
            .map(|p| p.iter().filter(|r| r.disable).map(|r| r.id.clone()).collect())
            .unwrap_or_default()
    }

    /// Whether a hook is running on this thread.
    fn in_hook(&self) -> bool {
        self.core
            .actions
            .lock()
            .is_ok_and(|a| {
                a.get(&std::thread::current().id())
                    .is_some_and(|s| !s.running.is_empty())
            })
    }

    /// Admits one hook run of `plugin` against its payer's budget for this action ([`payer`]:
    /// `plugin` itself on a user's action, the initiator inside a plugin's operation). Past
    /// that budget (too many hooks or too much hook time) the payer's remaining hooks in this
    /// action are skipped and the user is told when the action ends; nobody is switched off
    /// and other plugins' hooks run on. A hook inside a hook ends the action's hooks. A
    /// plugin whose hook faulted earlier in the action is skipped.
    pub(crate) fn enter_hook(&self, app: &App, plugin: &str, hook: &str) -> Admission<'_> {
        enum Verdict {
            Run,
            Skip,
            Spent,
            Stop,
            Exhausted(&'static str),
            Nested(Option<String>),
        }
        let config = self.core.config();
        let payer = payer(plugin);
        let verdict = self.with_action(|a| {
            if a.tripped {
                return Verdict::Stop;
            }
            if a.faulted.contains(plugin) {
                return Verdict::Skip;
            }
            if a.exhausted.contains(&payer) {
                return Verdict::Spent;
            }
            if a.running.len() >= config.max_hook_depth.max(1) as usize {
                a.tripped = true;
                return Verdict::Nested(a.running.last().cloned());
            }
            let spent = a.spent.entry(payer.clone()).or_default();
            let limit = if spent.hooks >= config.max_hooks_per_action.max(1) {
                Some("count")
            } else if spent.time >= config.hook_time_per_action {
                Some("time")
            } else {
                None
            };
            if let Some(limit) = limit {
                a.exhausted.insert(payer.clone());
                return Verdict::Exhausted(limit);
            }
            spent.hooks += 1;
            a.running.push(payer.clone());
            Verdict::Run
        });
        match verdict {
            None | Some(Verdict::Stop) => Admission::Stop,
            Some(Verdict::Skip) => Admission::Skip,
            Some(Verdict::Spent) => Admission::Spent,
            Some(Verdict::Run) => Admission::Run(HookGuard {
                host: self,
                started: Instant::now(),
                payer,
            }),
            Some(Verdict::Exhausted(limit)) => {
                let message = format!(
                    "HOOK_BUDGET: {hook} skipped: this plugin's hook budget for the action is spent ({limit}: at most {} hooks, {} ms per plugin); its remaining hooks in this action are skipped",
                    config.max_hooks_per_action,
                    config.hook_time_per_action.as_millis(),
                );
                self.core.log(&payer, LogLevel::Warn, message, app.now());
                Admission::Spent
            }
            Some(Verdict::Nested(outer)) => {
                if let Ok(mut q) = self.events.lock() {
                    q.clear();
                }
                let message = format!(
                    "HOOK_DEPTH: {hook} would have started inside another hook; no more hooks run in this action"
                );
                self.core
                    .log(outer.as_deref().unwrap_or(&payer), LogLevel::Error, message, app.now());
                Admission::Stop
            }
        }
    }

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
                    Some(HookPoint::Before(m)) if hook_allowed(registry, p, &m) => {
                        next.before.entry(m).or_default().push(id.clone());
                    }
                    Some(HookPoint::After(m)) if hook_allowed(registry, p, &m) => {
                        next.after.entry(m).or_default().push(id.clone());
                    }
                    Some(HookPoint::Before(_) | HookPoint::After(_)) => {}
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
        // A user-started operation starts a chain: what its calls cause is charged to it.
        let _chain = (mode == Mode::Operation && initiator().is_none())
            .then(|| ChainGuard::enter(plugin.id()));
        let mut host = OpHost {
            app,
            plugin,
            mode,
            group: None,
            history: false,
            notices: Vec::new(),
            paused: Duration::ZERO,
            deadline: None,
        };
        let r = rt.call(&compiled, op, input, &limits, &mut host);
        let notices = host.finish(r.is_ok());
        self.core.record_own(plugin.id(), &r, op, Some(app.now()));
        if mode != Mode::Operation && r.as_ref().is_err_and(|e| e.kind.is_fault()) {
            self.with_action(|a| a.faulted.insert(plugin.id().to_owned()));
        }
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
            // Checked again at call time (grants may have changed since the index was built).
            let Some(plugin) = self.get(&id).filter(|p| hook_allowed(app.registry(), p, method))
            else {
                continue;
            };
            let _running = match self.enter_hook(app, &id, &hook) {
                Admission::Run(guard) => guard,
                Admission::Skip => continue,
                // A plugin's call past its budget is refused: it must not go on without the
                // hooks the user installed to check it.
                Admission::Spent | Admission::Stop if skip.is_some() => {
                    return Err(hook_budget_refused(method))
                }
                Admission::Spent => continue,
                Admission::Stop => break,
            };
            let domains = redact::exposure(method).domains();
            let input = json!({
                "hook": hook,
                "method": method,
                "params": redact::filter(&plugin.granted, domains, params.clone()),
            });
            let (r, _) = self.run_op(app, &plugin, ops::HOOK_BEFORE, input, Mode::Hook);
            let decided = r.and_then(|v| {
                serde_json::from_value::<BeforeHookReply>(v).map_err(|e| {
                    PluginError::new(fc_plugins::PluginErrorKind::Abi, format!("{hook}: {e}"))
                })
            });
            match decided {
                Ok(BeforeHookReply::Continue { params: Some(p) }) => {
                    params = redact::keep_user_only(&params, p);
                }
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
            if let Some(plugin) = self.get(&id).filter(|p| hook_allowed(app.registry(), p, method)) {
                let hook = format!("after:{method}");
                let _running = match self.enter_hook(app, &id, &hook) {
                    Admission::Run(guard) => guard,
                    Admission::Skip | Admission::Spent => continue,
                    Admission::Stop => return,
                };
                let domains = redact::exposure(method).domains();
                let input = json!({
                    "hook": hook,
                    "method": method,
                    "params": redact::filter(&plugin.granted, domains, params.clone()),
                    "response": redact::filter(&plugin.granted, domains, response.clone()),
                });
                let _ = self.run_op(app, &plugin, ops::HOOK_AFTER, input, Mode::Reaction);
            }
        }
    }

    /// Keeps committed events for `event:` hooks (not those caused by plugins).
    pub(crate) fn queue_events(&self, events: &[DomainEvent], actor: &Actor) {
        // Re-entrancy guard: whatever a hook causes (its writes, a rollback, a nested
        // dispatch) never feeds hooks again, whatever the actor.
        if matches!(actor, Actor::Plugin(_)) || events.is_empty() || self.in_hook() {
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
        // Each queued event runs one hook of each plugin subscribed to it, so a plugin can
        // never run more than its hook budget of them (one more marks its budget spent): an
        // event is kept while one of its subscribers has room, and the queue stays bounded
        // (budget × plugins) whatever the action produced.
        let cap = self.core.config().max_hooks_per_action.max(1) as usize + 1;
        if let Ok(mut q) = self.events.lock() {
            for event in wanted {
                let Some(subscribers) = event_type(&event).and_then(|t| l.events.get(&t)) else {
                    continue;
                };
                if subscribers.iter().all(|id| q.per_plugin.get(id).is_some_and(|n| *n >= cap)) {
                    continue;
                }
                for id in subscribers {
                    *q.per_plugin.entry(id.clone()).or_default() += 1;
                }
                q.events.push(event);
            }
        }
    }

    fn dispatch_events(&self, app: &App) {
        let queued = match self.events.lock() {
            Ok(mut q) if !q.events.is_empty() => std::mem::take(&mut *q).events,
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
            for id in ids {
                if let Some(plugin) = self.get(&id) {
                    // Phase-3 domains (stats, AI, MCP…): not delivered without read.
                    if !redact::event_delivered(&plugin.granted, &event) {
                        continue;
                    }
                    let hook = format!("event:{kind}");
                    let _running = match self.enter_hook(app, &id, &hook) {
                        Admission::Run(guard) => guard,
                        Admission::Skip | Admission::Spent => continue,
                        Admission::Stop => return,
                    };
                    // Filtered by this plugin's read permissions for the event's domain.
                    let input = json!({ "hook": hook, "event": redact::event(&plugin.granted, &event) });
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

/// Last component of `path` (what a plugin may learn about a picked file).
fn file_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default()
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
    /// The group holds changes to user data (not only the plugin's own storage/settings).
    history: bool,
    notices: Vec<PluginNotice>,
    /// Time the current host call spent in a file dialog (the user's time, not the plugin's).
    paused: Duration,
    /// The plugin call's deadline ([`HostCalls::deadline`]): database statements of a host
    /// call stop there (`fc_storage::interrupt`).
    deadline: Option<Instant>,
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

    /// Commits the operation's undo step, or rolls its writes back if the plugin failed
    /// (plugin storage and settings included). Only storage/settings changes: kept, no undo
    /// step.
    fn finish(&mut self, ok: bool) -> Vec<PluginNotice> {
        if let Some(group) = self.group.take() {
            let actor = self.plugin.actor();
            if ok && !self.history {
                let _ = self.app.close_empty_group(&group);
            } else if ok || group.is_empty() {
                let summary = I18nMessage::key("oplog.summary.plugins.run")
                    .param("plugin", &self.plugin.manifest.name);
                let _ = self.app.commit_group(&group, &actor, Some(summary));
            } else {
                let request = InvokeRequest {
                    method: format!("{}.{}", fc_api::API_VERSION, PluginsRollback::NAME),
                    params: json!({ "calls": group.inverses_rev() }),
                };
                // As the plugin: undoing its writes must not look like a user or system change
                // (that would feed `event:` hooks and could cascade).
                let _ = self.app.invoke_with(
                    &request,
                    &actor,
                    OplogMode::Discard(Some(group.batch_id())),
                );
            }
        }
        std::mem::take(&mut self.notices)
    }

    /// `invoke`: the public API filtered by the plugin's grants, its result and errors
    /// filtered by its read permissions ([`redact`]).
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
        let granted = &self.plugin.granted;
        let exposure = redact::call_exposure(name, &params, self.plugin.id());
        let domains = exposure.domains();
        if let redact::Exposure::Refused(reason) = exposure {
            return Err(denied(method, d.permission, reason));
        }
        if redact::refuse_upfront(d.kind, exposure) && !redact::can_read(granted, domains) {
            let missing = redact::missing(granted, domains);
            return Err(AppError::Api(
                ApiError::new(
                    ErrorCode::PermissionDenied,
                    format!("{method}: its result needs {}", missing.join(", ")),
                )
                .with_detail("method", method)
                .with_detail("permission", missing.first().cloned().unwrap_or_default())
                .with_detail("missing", missing),
            ));
        }
        let r = match &entry.handler {
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
                        .before_hooks(app, name, params, &actor, Some(self.plugin.id()))
                } else {
                    Ok(params)
                };
                params.and_then(|params| {
                    let request = InvokeRequest {
                        method: method.to_owned(),
                        params,
                    };
                    let group = self.group()?;
                    let r = app.invoke_with(&request, &actor, OplogMode::Defer(group));
                    if r.is_ok() {
                        self.history = true;
                    }
                    r
                })
            }
        };
        let granted = &self.plugin.granted;
        match r {
            Ok(v) => Ok(redact::filter(granted, domains, redact::jobs(granted, name, v))),
            Err(e) => Err(AppError::Api(redact::error(
                granted,
                self.plugin.id(),
                domains,
                e.into_api(),
            ))),
        }
    }

    /// Runs one of the plugin's own bookkeeping commands (storage, settings) inside the
    /// operation's undo group.
    fn own_write<M: ApiMethod>(&mut self, request: &M::Request) -> AppResult<Value> {
        let request = InvokeRequest {
            method: format!("{}.{}", fc_api::API_VERSION, M::NAME),
            params: serde_json::to_value(request)
                .map_err(|e| AppError::api(ErrorCode::Internal, e.to_string()))?,
        };
        let actor = self.plugin.actor();
        let app = self.app;
        let group = self.group()?;
        app.invoke_with(&request, &actor, OplogMode::Defer(group))
    }

    fn kv_write(&mut self, key: String, value: Option<Value>) -> AppResult<Value> {
        self.own_write::<PluginsKvSet>(&SetPluginKv {
            plugin_id: self.plugin.id().to_owned(),
            key,
            value,
            restore: false,
        })
        .map(|_| Value::Null)
    }

    /// `fs:pick` checks: granted, user-started operation, a dialog available.
    fn picker(&self, func: &str) -> AppResult<Arc<dyn FilePicker>> {
        self.require(func, "fs:pick")?;
        if self.mode != Mode::Operation {
            return Err(denied(
                func,
                "fs:pick",
                "file dialogs open only during an operation the user started",
            ));
        }
        self.app.plugins().picker().ok_or_else(|| {
            AppError::Api(
                ApiError::new(ErrorCode::ProviderUnavailable, "no file dialog in this app")
                    .with_detail("provider", "fs"),
            )
        })
    }

    /// Runs a file dialog; its time is the user's ([`HostCalls::paused`]).
    fn dialog<T>(&mut self, f: impl FnOnce() -> T) -> T {
        let started = Instant::now();
        let out = f();
        self.paused += started.elapsed();
        out
    }

    fn fs_open(&mut self, func: &str, args: Value) -> AppResult<Value> {
        #[derive(serde::Deserialize)]
        struct Open {
            #[serde(default)]
            extensions: Vec<String>,
        }
        let a: Open = Self::args(func, args)?;
        let picker = self.picker(func)?;
        let Some(path) = self.dialog(|| picker.pick_open(&a.extensions)) else {
            return Ok(Value::Null);
        };
        let io = |e: std::io::Error| AppError::api(ErrorCode::IoFile, e.to_string());
        let too_large = || {
            AppError::Api(
                ApiError::new(ErrorCode::ValidationInvalidParams, "file too large for a plugin")
                    .with_detail("function", func)
                    .with_detail("max_bytes", MAX_PICKED_FILE_BYTES),
            )
        };
        let file = std::fs::File::open(&path).map_err(io)?;
        if file.metadata().map_err(io)?.len() > MAX_PICKED_FILE_BYTES {
            return Err(too_large());
        }
        // The size can lie (a device, a pipe, a file growing meanwhile): the limit holds
        // while reading, never more than one byte past it is read.
        let mut data = Vec::new();
        std::io::Read::read_to_end(
            &mut std::io::Read::take(file, MAX_PICKED_FILE_BYTES + 1),
            &mut data,
        )
        .map_err(io)?;
        if data.len() as u64 > MAX_PICKED_FILE_BYTES {
            return Err(too_large());
        }
        Ok(json!({
            "name": file_name(&path),
            "size": data.len(),
            "data_b64": base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &data),
        }))
    }

    fn fs_save(&mut self, func: &str, args: Value) -> AppResult<Value> {
        #[derive(serde::Deserialize)]
        struct Save {
            suggested_name: String,
            data_b64: String,
            #[serde(default)]
            extensions: Vec<String>,
        }
        let a: Save = Self::args(func, args)?;
        let picker = self.picker(func)?;
        if a.data_b64.len() as u64 > MAX_PICKED_FILE_BYTES.div_ceil(3) * 4 {
            return Err(crate::support::invalid(format!("{func}: file too large")));
        }
        let data = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &a.data_b64)
            .map_err(|e| crate::support::invalid(format!("{func}: data_b64: {e}")))?;
        if data.len() as u64 > MAX_PICKED_FILE_BYTES {
            return Err(crate::support::invalid(format!("{func}: file too large")));
        }
        // Only a bare name is suggested: a plugin cannot steer the dialog to a folder.
        let suggested = file_name(std::path::Path::new(&a.suggested_name));
        let Some(path) = self.dialog(|| picker.pick_save(&suggested, &a.extensions)) else {
            return Ok(Value::Null);
        };
        std::fs::write(&path, data).map_err(|e| AppError::api(ErrorCode::IoFile, e.to_string()))?;
        Ok(json!({ "name": file_name(&path) }))
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
                // Paged: at most `limit` (≤ KV_LIST_MAX_ENTRIES) entries and about
                // KV_LIST_MAX_BYTES per call; the next page starts `after` the last key.
                #[derive(serde::Deserialize)]
                struct Page {
                    #[serde(default)]
                    prefix: String,
                    #[serde(default)]
                    after: Option<String>,
                    #[serde(default)]
                    limit: Option<usize>,
                }
                let a: Page = Self::args(func, args)?;
                let limit = a.limit.unwrap_or(KV_LIST_MAX_ENTRIES).clamp(1, KV_LIST_MAX_ENTRIES);
                let tx = self.app.store().read()?;
                let rows = tx.list_plugin_kv_page(&id, &a.prefix, a.after.as_deref(), limit)?;
                let mut bytes = 0usize;
                let mut page = Vec::new();
                for (key, value) in rows {
                    bytes += key.len() + value.to_string().len();
                    if bytes > KV_LIST_MAX_BYTES && !page.is_empty() {
                        break;
                    }
                    page.push((key, value));
                }
                Ok(json!(page))
            }
            host_fns::KV_SET => {
                // Writing storage needs its own permission (declared and granted); deleting
                // never does, so a plugin can always clean up.
                self.require(func, "storage:kv")?;
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
            host_fns::SETTINGS_SET => {
                // Its own namespace only: the id is the caller's, the key must be in its
                // schema (checked by `plugins.settings.set`).
                let a: KeyArgs = Self::args(func, args)?;
                let r = self.own_write::<PluginsSettingsSet>(&SetPluginSetting {
                    id: id.clone(),
                    key: a.key,
                    value: a.value.filter(|v| !v.is_null()),
                })?;
                Ok(r.get("values").cloned().unwrap_or(Value::Null))
            }
            host_fns::FS_OPEN => self.fs_open(func, args),
            host_fns::FS_SAVE => self.fs_save(func, args),
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
                self.require(func, "ui:notify")?;
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
                        // No path: a plugin never learns where files live.
                        let ready = store
                            .as_ref()
                            .and_then(|s| s.path(&id, &m.id).ok())
                            .is_some_and(|p| p.is_file());
                        json!({ "id": m.id, "status": status, "ready": ready })
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
        self.paused = Duration::ZERO;
        // Per-minute windows are the initiator's (another plugin's hooks it runs into spend
        // its windows, never their own).
        self.app.plugins().core.admit_rate(&payer(self.plugin.id()), func)?;
        // Long database work stops at the plugin's deadline.
        let _deadline = self.deadline.map(fc_storage::interrupt::deadline);
        self.dispatch(func, args).map_err(to_wire)
    }

    fn paused(&mut self) -> Duration {
        std::mem::take(&mut self.paused)
    }

    fn deadline(&mut self, deadline: Instant) {
        self.deadline = Some(deadline);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestApp;
    use fc_api::v1::decks::CreateDeck;
    use fc_api::v1::methods::{DecksCreate, PluginsInstall};
    use fc_api::v1::plugins::InstallPlugin;

    /// Minor finding (a single long host call is not interrupted): database work of a host
    /// call stops at the plugin call's deadline, not after it. Here the deadline has passed
    /// when the call starts its listing: the statement is interrupted; without a deadline the
    /// same call answers.
    #[test]
    fn host_call_database_work_stops_at_the_deadline() {
        const ID: &str = "com.test.slow";
        let t = TestApp::new(Arc::new(fc_storage::SqliteStore::open_in_memory().unwrap()));
        for i in 0..300 {
            t.ok::<DecksCreate>(CreateDeck {
                path: format!("D{i}"),
                parent_id: None,
                description: None,
                icon: None,
                color: None,
            });
        }
        let manifest = format!(
            "id = \"{ID}\"\nname = \"{ID}\"\nversion = \"1.0.0\"\napi_version = \"1\"\n\
             permissions = [\"decks:read\"]\n"
        );
        let wat = fc_plugins::testing::host_loop_wat(r#"{"fn":"log","args":{"message":"x"}}"#, 1);
        let wasm = fc_plugins::testing::wat2wasm(wat.as_bytes()).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("slow.fcplugin");
        std::fs::write(&path, fc_plugins::package::pack(&manifest, &wasm, &[]).unwrap()).unwrap();
        t.ok::<PluginsInstall>(InstallPlugin {
            path: path.display().to_string(),
            grant: vec!["decks:read".into()],
        });
        let plugin = t.app.plugins().get(ID).expect("active");
        let mut host = OpHost {
            app: &t.app,
            plugin: &plugin,
            mode: Mode::Operation,
            group: None,
            history: false,
            notices: Vec::new(),
            paused: Duration::ZERO,
            deadline: None,
        };
        let list = json!({ "method": "v1.decks.list", "params": {} });
        let decks = HostCalls::call(&mut host, host_fns::INVOKE, list.clone()).unwrap();
        assert!(decks["decks"].as_array().is_some_and(|d| d.len() >= 300), "{decks}");
        HostCalls::deadline(&mut host, Instant::now());
        let e = HostCalls::call(&mut host, host_fns::INVOKE, list).unwrap_err();
        assert_eq!(e.details["interrupted"], json!(true), "{e:?}");
        // The deadline ends with the host call: the app itself is not interrupted.
        assert!(!fc_storage::interrupt::expired());
        t.ok::<DecksCreate>(CreateDeck {
            path: "After".into(),
            parent_id: None,
            description: None,
            icon: None,
            color: None,
        });
    }
}
