//! The method registry: one handler per `fc_api::v1` catalog entry, grouped by module.
//!
//! Modules register themselves from their own file (`modules/<id>.rs` → `register()`), so
//! parallel lanes never edit a shared list except the one-line `modules::register_all`.

use crate::error::{AppError, AppResult};
use crate::handler::{
    CommandCtx, CommandHandler, CommandOutcome, PendingOplogEntry, QueryCtx, QueryHandler,
};
use crate::jobs::JobHandler;
use crate::module::ModuleDescriptor;
use fc_api::method::parse_qualified_name;
use fc_api::v1::oplog::Actor;
use fc_api::{
    ApiError, ApiMethod, ErrorCode, InvokeRequest, MethodDescriptor, MethodKind, Visibility,
};
use fc_providers::speech::TtsProvider;
use fc_providers::strategy::CardTypeEngine;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::marker::PhantomData;
use std::sync::{Arc, RwLock};

/// Type-erased command handler.
pub(crate) trait ErasedCommand: Send + Sync {
    fn validate(&self, params: &Value) -> AppResult<()>;
    /// The outcome, plus the oplog payload if the handler replaces the request
    /// ([`CommandHandler::recorded_payload`]).
    fn execute(
        &self,
        params: Value,
        ctx: &mut CommandCtx<'_>,
    ) -> AppResult<(CommandOutcome<Value>, Option<Value>)>;
}

/// Type-erased query handler.
pub(crate) trait ErasedQuery: Send + Sync {
    fn validate(&self, params: &Value) -> AppResult<()>;
    fn run(&self, params: Value, ctx: &QueryCtx<'_>) -> AppResult<Value>;
}

/// Type-erased action handler (see [`Registry::action`]).
pub(crate) trait ErasedAction: Send + Sync {
    fn validate(&self, params: &Value) -> AppResult<()>;
    fn run(&self, params: Value, app: &crate::App, actor: &Actor) -> AppResult<Value>;
}

/// Handles method `M` **outside** any transaction, with the app at hand (plugin operations:
/// the plugin's host calls are separate invokes, recorded as one undo step by the handler).
pub trait ActionHandler<M: ApiMethod>: Send + Sync + 'static {
    /// Runs the action.
    fn run(&self, req: M::Request, app: &crate::App, actor: &Actor) -> AppResult<M::Response>;
}

impl<M, F> ActionHandler<M> for F
where
    M: ApiMethod,
    F: Fn(M::Request, &crate::App, &Actor) -> AppResult<M::Response> + Send + Sync + 'static,
{
    fn run(&self, req: M::Request, app: &crate::App, actor: &Actor) -> AppResult<M::Response> {
        self(req, app, actor)
    }
}

struct TypedCommand<M, H>(H, PhantomData<fn() -> M>);
struct TypedQuery<M, H>(H, PhantomData<fn() -> M>);
struct TypedAction<M, H>(H, PhantomData<fn() -> M>);

impl<M: ApiMethod, H: ActionHandler<M>> ErasedAction for TypedAction<M, H> {
    fn validate(&self, params: &Value) -> AppResult<()> {
        parse::<M::Request>(M::NAME, params.clone()).map(|_| ())
    }

    fn run(&self, params: Value, app: &crate::App, actor: &Actor) -> AppResult<Value> {
        let req: M::Request = parse(M::NAME, params)?;
        to_value(&self.0.run(req, app, actor)?)
    }
}

/// A method registered at runtime (plugin commands, v0.7): not part of the static `v1`
/// catalog, keyed by its full unversioned name (`plugin.<plugin id>.<command>`). Always a
/// public command whose sub-calls are what the oplog records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DynamicMethod {
    /// Unversioned name.
    pub name: Arc<str>,
    /// Owning module (`plugins`).
    pub module: Arc<str>,
    /// Permission needed to call it.
    pub permission: Arc<str>,
    /// i18n key of its label (command palette).
    pub label_key: Arc<str>,
}

impl DynamicMethod {
    /// Versioned wire name.
    pub fn qualified_name(&self) -> String {
        format!("{}.{}", fc_api::API_VERSION, self.name)
    }
}

/// Runs a [`DynamicMethod`] (outside any transaction, like an action).
pub trait DynamicHandler: Send + Sync {
    /// Runs the call.
    fn invoke(&self, params: Value, app: &crate::App, actor: &Actor) -> AppResult<Value>;
}

struct DynamicEntry {
    method: DynamicMethod,
    handler: Arc<dyn DynamicHandler>,
}

fn parse<T: DeserializeOwned>(method: &str, params: Value) -> AppResult<T> {
    serde_json::from_value(params).map_err(|e| {
        AppError::Api(
            ApiError::new(ErrorCode::ValidationInvalidParams, format!("{method}: {e}"))
                .with_detail("method", method)
                .with_detail("reason", e.to_string()),
        )
    })
}

fn to_value<T: serde::Serialize>(v: &T) -> AppResult<Value> {
    serde_json::to_value(v)
        .map_err(|e| AppError::api(ErrorCode::Internal, format!("serialize response: {e}")))
}

impl<M: ApiMethod, H: CommandHandler<M>> ErasedCommand for TypedCommand<M, H> {
    fn validate(&self, params: &Value) -> AppResult<()> {
        let req: M::Request = parse(M::NAME, params.clone())?;
        self.0.validate(&req)
    }

    fn execute(
        &self,
        params: Value,
        ctx: &mut CommandCtx<'_>,
    ) -> AppResult<(CommandOutcome<Value>, Option<Value>)> {
        let req: M::Request = parse(M::NAME, params)?;
        self.0.validate(&req)?;
        let outcome = self.0.execute(req, ctx)?;
        let recorded = self.0.recorded_payload(&outcome.response);
        let response = to_value(&outcome.response)?;
        Ok((outcome.map(|_| response), recorded))
    }
}

impl<M: ApiMethod, H: QueryHandler<M>> ErasedQuery for TypedQuery<M, H> {
    fn validate(&self, params: &Value) -> AppResult<()> {
        parse::<M::Request>(M::NAME, params.clone()).map(|_| ())
    }

    fn run(&self, params: Value, ctx: &QueryCtx<'_>) -> AppResult<Value> {
        let req: M::Request = parse(M::NAME, params)?;
        to_value(&self.0.run(req, ctx)?)
    }
}

/// A registered handler. v0.7 added [`ErasedHandler::Action`]: code that only needs to
/// validate params should use [`ErasedHandler::validate`] instead of an exhaustive `match`,
/// and other matches should end in a `_` arm where they can, so a further kind does not
/// break them.
pub(crate) enum ErasedHandler {
    Command(Box<dyn ErasedCommand>),
    Query(Box<dyn ErasedQuery>),
    Action(Box<dyn ErasedAction>),
}

impl ErasedHandler {
    /// Parses `params` as the method's request (any handler kind).
    pub(crate) fn validate(&self, params: &Value) -> AppResult<()> {
        match self {
            Self::Command(h) => h.validate(params),
            Self::Query(h) => h.validate(params),
            Self::Action(h) => h.validate(params),
        }
    }
}

pub(crate) struct Entry {
    pub descriptor: MethodDescriptor,
    pub handler: ErasedHandler,
}

/// Registry construction error (a programming error caught at startup and in tests).
#[derive(Debug, thiserror::Error)]
#[error("invalid registry: {0:?}")]
pub struct RegistryError(pub Vec<String>);

/// Methods, modules and card type engines known to the app.
///
/// The static part (catalog methods, modules, built-in engines, jobs) is filled by
/// `register()` functions and frozen by [`Registry::finish`]. The **runtime** part (v0.7) can
/// change while the app runs, through `&self`: methods with `Arc<str>` keys
/// ([`Registry::register_dynamic`]), card type engines and TTS providers contributed by
/// plugins, and the permission grants of plugin actors ([`Registry::set_plugin_grants`]).
#[derive(Default)]
pub struct Registry {
    entries: BTreeMap<&'static str, Entry>,
    modules: Vec<ModuleDescriptor>,
    card_types: RwLock<BTreeMap<String, Arc<dyn CardTypeEngine>>>,
    jobs: BTreeMap<&'static str, Arc<dyn JobHandler>>,
    /// Shared services modules hand to the app (see [`Registry::provide`]).
    extensions: Vec<Arc<dyn std::any::Any + Send + Sync>>,
    dynamic: RwLock<BTreeMap<Arc<str>, DynamicEntry>>,
    tts: RwLock<BTreeMap<String, Arc<dyn TtsProvider>>>,
    grants: RwLock<HashMap<String, Arc<BTreeSet<String>>>>,
    errors: Vec<String>,
}

fn read<T>(lock: &RwLock<T>) -> std::sync::RwLockReadGuard<'_, T> {
    lock.read()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn write<T>(lock: &RwLock<T>) -> std::sync::RwLockWriteGuard<'_, T> {
    lock.write()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl Registry {
    /// Empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Declares a module. Must be called before registering its methods.
    pub fn module(&mut self, descriptor: ModuleDescriptor) {
        if self.modules.iter().any(|m| m.id == descriptor.id) {
            self.errors
                .push(format!("module {} registered twice", descriptor.id));
        }
        self.modules.push(descriptor);
    }

    /// Registers a card type engine under `engine.info().id` (docs/02 §4: a new card type is
    /// an engine registered here; templates refer to it by id).
    pub fn card_type(&mut self, engine: Arc<dyn CardTypeEngine>) {
        let id = engine.info().id;
        if write(&self.card_types).insert(id.clone(), engine).is_some() {
            self.errors.push(format!("card type {id} registered twice"));
        }
    }

    /// Runtime registration of a card type engine (plugins). Returns `false` (and changes
    /// nothing) if the id is taken.
    pub fn register_card_type(&self, engine: Arc<dyn CardTypeEngine>) -> bool {
        let id = engine.info().id;
        let mut map = write(&self.card_types);
        if map.contains_key(&id) {
            return false;
        }
        map.insert(id, engine);
        true
    }

    /// Removes runtime-registered card types whose id starts with `prefix`.
    pub fn unregister_card_types(&self, prefix: &str) {
        write(&self.card_types).retain(|id, _| !id.starts_with(prefix));
    }

    /// Card type engine by id.
    pub fn card_type_engine(&self, id: &str) -> Option<Arc<dyn CardTypeEngine>> {
        read(&self.card_types).get(id).cloned()
    }

    /// Registered card type engines, by id.
    pub fn card_types(&self) -> Vec<Arc<dyn CardTypeEngine>> {
        read(&self.card_types).values().cloned().collect()
    }

    /// Runtime registration of a TTS provider (built-in or plugin). Replaces one with the
    /// same id.
    pub fn register_tts(&self, provider: Arc<dyn TtsProvider>) {
        write(&self.tts).insert(provider.info().id, provider);
    }

    /// Removes TTS providers whose id starts with `prefix`.
    pub fn unregister_tts(&self, prefix: &str) {
        write(&self.tts).retain(|id, _| !id.starts_with(prefix));
    }

    /// TTS provider by id.
    pub fn tts_provider(&self, id: &str) -> Option<Arc<dyn TtsProvider>> {
        read(&self.tts).get(id).cloned()
    }

    /// TTS providers, by id.
    pub fn tts_providers(&self) -> Vec<Arc<dyn TtsProvider>> {
        read(&self.tts).values().cloned().collect()
    }

    /// Runtime registration of a method outside the static catalog. Returns `false` if the
    /// name is a catalog method or already registered.
    pub fn register_dynamic(
        &self,
        method: DynamicMethod,
        handler: Arc<dyn DynamicHandler>,
    ) -> bool {
        if self.entries.contains_key(&*method.name) {
            return false;
        }
        let mut map = write(&self.dynamic);
        if map.contains_key(&method.name) {
            return false;
        }
        map.insert(method.name.clone(), DynamicEntry { method, handler });
        true
    }

    /// Removes runtime methods whose name starts with `prefix`.
    pub fn unregister_dynamic(&self, prefix: &str) {
        write(&self.dynamic).retain(|name, _| !name.starts_with(prefix));
    }

    /// Runtime methods, by name.
    pub fn dynamic_methods(&self) -> Vec<DynamicMethod> {
        read(&self.dynamic)
            .values()
            .map(|e| e.method.clone())
            .collect()
    }

    pub(crate) fn dynamic_entry(
        &self,
        name: &str,
    ) -> Option<(DynamicMethod, Arc<dyn DynamicHandler>)> {
        read(&self.dynamic)
            .get(name)
            .map(|e| (e.method.clone(), e.handler.clone()))
    }

    /// Sets (`Some`) or revokes (`None`) everything plugin `id` may do as an actor. A plugin
    /// without an entry may do nothing.
    pub fn set_plugin_grants(&self, id: &str, grants: Option<BTreeSet<String>>) {
        let mut map = write(&self.grants);
        match grants {
            Some(g) => {
                map.insert(id.to_owned(), Arc::new(g));
            }
            None => {
                map.remove(id);
            }
        }
    }

    /// Whether `actor` holds `permission`: the single place permissions are enforced.
    ///
    /// * The user, the CLI and the app hold every permission.
    /// * An MCP client holds a fixed, curated set (`modules::mcp::MCP_PERMISSIONS`, docs/06
    ///   §2.1) — and [`ensure_access`] further limits it to the methods its tools call.
    /// * A plugin holds only what it was granted, only while it is active, and never an area
    ///   no plugin may hold (`io`, `mcp`, `ai_keys`, `ai:write`, `oplog`… —
    ///   `fc_plugins::permission::api_grantable`) even if a stored grant says otherwise.
    /// * A local API client holds the scopes the user approved for it, only those a client may
    ///   be granted (`modules::local_api::GRANTABLE_SCOPES`), only while approved (v0.9).
    /// * Automations hold **nothing** until their track defines their grants (deny by default:
    ///   a new actor kind never inherits the user's authority).
    pub fn permits(&self, actor: &Actor, permission: &str) -> bool {
        match actor {
            Actor::User | Actor::Cli | Actor::System => true,
            Actor::Mcp(_) => crate::modules::mcp::MCP_PERMISSIONS.contains(&permission),
            Actor::Plugin(id) => {
                fc_plugins::permission::api_grantable(permission)
                    && read(&self.grants)
                        .get(id)
                        .is_some_and(|g| g.contains(permission))
            }
            Actor::Automation(_) => false,
            // v0.9: its approved scopes (∩ `local_api::GRANTABLE_SCOPES`), refreshed per request.
            Actor::LocalApi(id) => crate::modules::local_api::permits(self, id, permission),
        }
    }

    fn insert<M: ApiMethod>(&mut self, expected: MethodKind, handler: ErasedHandler) {
        if M::KIND != expected {
            self.errors.push(format!(
                "{} registered as {:?} but is {:?}",
                M::NAME,
                expected,
                M::KIND
            ));
        }
        if !self.modules.iter().any(|m| m.id == M::MODULE) {
            self.errors.push(format!(
                "{}: module {} not registered first",
                M::NAME,
                M::MODULE
            ));
        }
        let entry = Entry {
            descriptor: MethodDescriptor::of::<M>(),
            handler,
        };
        if self.entries.insert(M::NAME, entry).is_some() {
            self.errors.push(format!("{} registered twice", M::NAME));
        }
    }

    /// Registers the handler of command `M`.
    pub fn command<M: ApiMethod>(&mut self, handler: impl CommandHandler<M>) {
        let erased = TypedCommand::<M, _>(handler, PhantomData);
        self.insert::<M>(
            MethodKind::Command,
            ErasedHandler::Command(Box::new(erased)),
        );
    }

    /// Registers the handler of query `M`.
    pub fn query<M: ApiMethod>(&mut self, handler: impl QueryHandler<M>) {
        let erased = TypedQuery::<M, _>(handler, PhantomData);
        self.insert::<M>(MethodKind::Query, ErasedHandler::Query(Box::new(erased)));
    }

    /// Registers `M` as an **action**: it runs outside any transaction with the app at hand
    /// (plugin operations). The catalog kind stays as declared; access checks are the same.
    pub fn action<M: ApiMethod>(&mut self, handler: impl ActionHandler<M>) {
        let erased = TypedAction::<M, _>(handler, PhantomData);
        self.insert::<M>(M::KIND, ErasedHandler::Action(Box::new(erased)));
    }

    /// Registers the handler of a background job kind (docs/03 §12).
    pub fn job(&mut self, handler: Arc<dyn JobHandler>) {
        let kind = handler.kind();
        if self.jobs.insert(kind, handler).is_some() {
            self.errors
                .push(format!("job kind {kind} registered twice"));
        }
    }

    /// Handler of job `kind`.
    pub fn job_handler(&self, kind: &str) -> Option<&Arc<dyn JobHandler>> {
        self.jobs.get(kind)
    }

    /// Worker lanes of the registered jobs ([`crate::jobs::MAIN_LANE`] always first).
    pub fn lanes(&self) -> Vec<&'static str> {
        let mut lanes = vec![crate::jobs::MAIN_LANE];
        for h in self.jobs.values() {
            if !lanes.contains(&h.lane()) {
                lanes.push(h.lane());
            }
        }
        lanes
    }

    /// The claim filter of `lane`: its job kinds (`true` = only these), or for the main lane
    /// the kinds of every other lane (`false` = all but these — unknown kinds too, so they
    /// fail visibly instead of waiting forever).
    pub(crate) fn lane_kinds(&self, lane: &str) -> (Vec<&'static str>, bool) {
        let main = lane == crate::jobs::MAIN_LANE;
        let kinds = self
            .jobs
            .values()
            .filter(|h| {
                if main {
                    h.lane() != crate::jobs::MAIN_LANE
                } else {
                    h.lane() == lane
                }
            })
            .map(|h| h.kind())
            .collect();
        (kinds, !main)
    }

    /// Makes a module's shared service reachable from the app (`App` methods a transport
    /// calls, e.g. installing the keychain the `ai` module stores keys in). One per type.
    pub fn provide<T: std::any::Any + Send + Sync>(&mut self, service: Arc<T>) {
        if self.extension::<T>().is_some() {
            self.errors.push(format!(
                "service {} provided twice",
                std::any::type_name::<T>()
            ));
        }
        self.extensions.push(service);
    }

    /// The service of type `T`, if a module provided one.
    pub fn extension<T: std::any::Any + Send + Sync>(&self) -> Option<Arc<T>> {
        self.extensions
            .iter()
            .find_map(|e| Arc::clone(e).downcast::<T>().ok())
    }

    /// Registers a placeholder for `M` that fails with `INTERNAL_NOT_IMPLEMENTED`. Lets the
    /// registry cover the whole catalog while lanes implement handlers one by one.
    pub fn stub<M: ApiMethod>(&mut self) {
        match M::KIND {
            MethodKind::Command => self.command::<M>(
                |_req: M::Request,
                 _ctx: &mut CommandCtx<'_>|
                 -> AppResult<CommandOutcome<M::Response>> {
                    Err(ApiError::not_implemented(M::NAME).into())
                },
            ),
            MethodKind::Query => self.query::<M>(
                |_req: M::Request, _ctx: &QueryCtx<'_>| -> AppResult<M::Response> {
                    Err(ApiError::not_implemented(M::NAME).into())
                },
            ),
        }
    }

    /// Validates the registry (no duplicates, kinds match, modules declared).
    pub fn finish(self) -> Result<Self, RegistryError> {
        if self.errors.is_empty() {
            Ok(self)
        } else {
            Err(RegistryError(self.errors))
        }
    }

    /// Descriptor by unversioned name.
    pub fn descriptor(&self, name: &str) -> Option<&MethodDescriptor> {
        self.entries.get(name).map(|e| &e.descriptor)
    }

    /// All registered methods, sorted by name.
    pub fn methods(&self) -> impl Iterator<Item = &MethodDescriptor> {
        self.entries.values().map(|e| &e.descriptor)
    }

    /// Declared modules, in registration order.
    pub fn modules(&self) -> &[ModuleDescriptor] {
        &self.modules
    }

    pub(crate) fn entry(&self, name: &str) -> Option<&Entry> {
        self.entries.get(name)
    }

    /// See [`CommandCtx::call`]. Internal methods are allowed here (inverses, undo/redo);
    /// callers that forward client calls (`batch.execute`) check [`ensure_invocable`] first.
    /// The module switch applies to every nested call (read inside this transaction), unless
    /// `ctx.check_modules` is off (crash recovery).
    pub(crate) fn execute_nested(
        &self,
        ctx: &mut CommandCtx<'_>,
        request: &InvokeRequest,
    ) -> AppResult<Value> {
        let entry = parse_qualified_name(&request.method)
            .and_then(|n| self.entry(n))
            .ok_or_else(|| unknown_method(&request.method))?;
        let ErasedHandler::Command(handler) = &entry.handler else {
            return Err(AppError::api(
                ErrorCode::ValidationInvalidParams,
                format!(
                    "{} is not a transactional command; only those can be nested",
                    request.method
                ),
            ));
        };
        if ctx.check_modules
            && !crate::support::module_enabled(ctx.reads(), self, entry.descriptor.module)?
        {
            return Err(module_disabled(entry.descriptor.module));
        }
        run_recorded(
            ctx,
            &entry.descriptor,
            handler.as_ref(),
            &request.method,
            request.params.clone(),
        )
    }
}

/// Runs a command handler and records its oplog entry (if undoable and not a no-op) in a slot
/// reserved **before** the handler runs: a command's `seq` precedes those of its nested calls,
/// whatever depth they run at. Collects its events; returns the serialized response.
pub(crate) fn run_recorded(
    ctx: &mut CommandCtx<'_>,
    descriptor: &MethodDescriptor,
    handler: &dyn ErasedCommand,
    method: &str,
    params: Value,
) -> AppResult<Value> {
    let slot = ctx.oplog.len();
    ctx.oplog.push(None);
    let (outcome, recorded) = handler.execute(params.clone(), ctx)?;
    if descriptor.undoable && !outcome.inverse.is_empty() {
        ctx.oplog[slot] = Some(PendingOplogEntry {
            command: method.to_owned(),
            payload: recorded.unwrap_or(params),
            inverse: outcome.inverse,
            summary: outcome.summary,
        });
    }
    ctx.events.extend(outcome.events);
    Ok(outcome.response)
}

/// The access check every client-originated call goes through — top-level
/// [`App::invoke`](crate::App::invoke) and each call inside `batch.execute` alike:
/// internal methods are rejected (`PERMISSION_DENIED`), then the actor's permission, then the
/// owning module's switch (`CONFLICT_MODULE_DISABLED`).
pub(crate) fn ensure_invocable(
    registry: &Registry,
    d: &MethodDescriptor,
    actor: &Actor,
    module_enabled: bool,
) -> AppResult<()> {
    ensure_access(
        registry,
        &d.qualified_name(),
        d.visibility,
        d.permission,
        d.module,
        actor,
        module_enabled,
    )
}

/// [`ensure_invocable`] over the parts of a method (static or [`DynamicMethod`]).
pub(crate) fn ensure_access(
    registry: &Registry,
    qualified: &str,
    visibility: Visibility,
    permission: &str,
    module: &str,
    actor: &Actor,
    module_enabled: bool,
) -> AppResult<()> {
    // An MCP client reaches exactly the methods its tools call (`mcp::tools::catalog`), not
    // everything its permissions would cover: no tool deletes, so `notes.delete`,
    // `decks.delete`, `browse.purge_trash`… stay unreachable however the call arrives.
    let name = qualified
        .strip_prefix(fc_api::API_VERSION)
        .and_then(|n| n.strip_prefix('.'))
        .unwrap_or(qualified);
    let mcp_blocked =
        matches!(actor, Actor::Mcp(_)) && !crate::modules::mcp::tools::method_reachable(name);
    // A plugin never reaches what its redaction layer refuses (whole-collection work, AI
    // writes, unclassified methods) — through its host functions or any other way in.
    let plugin_blocked = matches!(actor, Actor::Plugin(_))
        && matches!(
            crate::modules::plugins::redact::exposure(name),
            crate::modules::plugins::redact::Exposure::Refused(_)
        );
    // A local API client reaches only the HTTP allow-list (public reads and undoable,
    // replayable commands), not everything its scopes would cover (v0.9).
    let local_api_blocked =
        matches!(actor, Actor::LocalApi(_)) && !crate::modules::local_api::method_reachable(name);
    if visibility == Visibility::Internal
        || mcp_blocked
        || local_api_blocked
        || plugin_blocked
        || !registry.permits(actor, permission)
    {
        return Err(AppError::Api(
            ApiError::new(
                ErrorCode::PermissionDenied,
                format!("{qualified} may not be called here"),
            )
            .with_detail("method", qualified)
            .with_detail("permission", permission),
        ));
    }
    if !module_enabled {
        return Err(module_disabled(module));
    }
    Ok(())
}

/// `CONFLICT_MODULE_DISABLED` for `module`.
pub(crate) fn module_disabled(module: &str) -> AppError {
    AppError::Api(
        ApiError::new(
            ErrorCode::ConflictModuleDisabled,
            format!("module {module} is disabled"),
        )
        .with_detail("module", module),
    )
}

pub(crate) fn unknown_method(name: &str) -> AppError {
    AppError::Api(
        ApiError::new(ErrorCode::NotFoundMethod, format!("unknown method {name}"))
            .with_detail("method", name),
    )
}
