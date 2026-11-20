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
use fc_providers::strategy::CardTypeEngine;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::collections::BTreeMap;
use std::marker::PhantomData;
use std::sync::Arc;

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

struct TypedCommand<M, H>(H, PhantomData<fn() -> M>);
struct TypedQuery<M, H>(H, PhantomData<fn() -> M>);

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

pub(crate) enum ErasedHandler {
    Command(Box<dyn ErasedCommand>),
    Query(Box<dyn ErasedQuery>),
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
#[derive(Default)]
pub struct Registry {
    entries: BTreeMap<&'static str, Entry>,
    modules: Vec<ModuleDescriptor>,
    card_types: BTreeMap<String, Arc<dyn CardTypeEngine>>,
    jobs: BTreeMap<&'static str, Arc<dyn JobHandler>>,
    errors: Vec<String>,
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
        if self.card_types.insert(id.clone(), engine).is_some() {
            self.errors.push(format!("card type {id} registered twice"));
        }
    }

    /// Card type engine by id.
    pub fn card_type_engine(&self, id: &str) -> Option<&Arc<dyn CardTypeEngine>> {
        self.card_types.get(id)
    }

    /// Registered card type engines, by id.
    pub fn card_types(&self) -> impl Iterator<Item = &Arc<dyn CardTypeEngine>> {
        self.card_types.values()
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
                format!("{} is a query; only commands can be nested", request.method),
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
    d: &MethodDescriptor,
    actor: &Actor,
    module_enabled: bool,
) -> AppResult<()> {
    if d.visibility == Visibility::Internal || !permits(actor, d.permission) {
        return Err(AppError::Api(
            ApiError::new(
                ErrorCode::PermissionDenied,
                format!("{} may not be called here", d.qualified_name()),
            )
            .with_detail("method", d.qualified_name())
            .with_detail("permission", d.permission),
        ));
    }
    if !module_enabled {
        return Err(module_disabled(d.module));
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

/// Whether `actor` holds `permission`. v0.1 has no grant store: the user, the CLI and the app
/// itself hold every permission, and so do the other actors until plugin/local-API grants land
/// (v0.7) — at which point this is the single place that enforces them.
fn permits(actor: &Actor, _permission: &str) -> bool {
    match actor {
        Actor::User | Actor::Cli | Actor::System => true,
        Actor::Plugin(_) | Actor::Automation(_) | Actor::LocalApi(_) => true,
    }
}

pub(crate) fn unknown_method(name: &str) -> AppError {
    AppError::Api(
        ApiError::new(ErrorCode::NotFoundMethod, format!("unknown method {name}"))
            .with_detail("method", name),
    )
}
