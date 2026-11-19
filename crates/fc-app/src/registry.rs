//! The method registry: one handler per `fc_api::v1` catalog entry, grouped by module.
//!
//! Modules register themselves from their own file (`modules/<id>.rs` → `register()`), so
//! parallel lanes never edit a shared list except the one-line `modules::register_all`.

use crate::error::{AppError, AppResult};
use crate::handler::{
    CommandCtx, CommandHandler, CommandOutcome, PendingOplogEntry, QueryCtx, QueryHandler,
};
use crate::module::ModuleDescriptor;
use fc_api::method::parse_qualified_name;
use fc_api::{ApiError, ApiMethod, ErrorCode, InvokeRequest, MethodDescriptor, MethodKind};
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::collections::BTreeMap;
use std::marker::PhantomData;

/// Type-erased command handler.
pub(crate) trait ErasedCommand: Send + Sync {
    fn validate(&self, params: &Value) -> AppResult<()>;
    fn execute(&self, params: Value, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<Value>>;
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

    fn execute(&self, params: Value, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<Value>> {
        let req: M::Request = parse(M::NAME, params)?;
        self.0.validate(&req)?;
        let outcome = self.0.execute(req, ctx)?;
        let response = to_value(&outcome.response)?;
        Ok(outcome.map(|_| response))
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

/// Methods and modules known to the app.
#[derive(Default)]
pub struct Registry {
    entries: BTreeMap<&'static str, Entry>,
    modules: Vec<ModuleDescriptor>,
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

    /// See [`CommandCtx::call`].
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
        let outcome = handler.execute(request.params.clone(), ctx)?;
        if entry.descriptor.undoable {
            ctx.oplog.push(PendingOplogEntry {
                command: request.method.clone(),
                payload: request.params.clone(),
                inverse: outcome.inverse,
                summary: outcome.summary,
            });
        }
        ctx.events.extend(outcome.events);
        Ok(outcome.response)
    }
}

pub(crate) fn unknown_method(name: &str) -> AppError {
    AppError::Api(
        ApiError::new(ErrorCode::NotFoundMethod, format!("unknown method {name}"))
            .with_detail("method", name),
    )
}
