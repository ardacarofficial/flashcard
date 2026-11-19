//! Handler traits and their execution contexts.
//!
//! **Command contract** (docs/03 §5–6, CLAUDE.md rule 4): a command handler
//! 1. validates (stateless checks in [`CommandHandler::validate`], stateful checks at the start
//!    of `execute` using `ctx.tx` reads),
//! 2. mutates state only through `ctx.tx` (one write transaction per top-level invoke),
//! 3. returns a [`CommandOutcome`] with the response, the **inverse** (calls that exactly undo
//!    it, executed in order), a summary for the undo UI, and the domain events to publish
//!    after commit.
//!
//! The dispatcher (not the handler) writes the oplog entry, commits and publishes. Handlers
//! never read the system clock (`ctx.now`) or generate ids other than via `ctx.ids`.

use crate::error::AppResult;
use crate::registry::Registry;
use fc_api::v1::common::I18nMessage;
use fc_api::v1::events::DomainEvent;
use fc_api::v1::oplog::{Actor, OplogKind};
use fc_api::{ApiMethod, InvokeRequest};
use fc_core::i18n::Locale;
use fc_core::{BatchId, IdGen, Timestamp};
use fc_storage::{Reads, WriteTx};
use serde_json::Value;

/// Result of a command.
#[derive(Debug, Clone)]
pub struct CommandOutcome<R> {
    /// Response returned to the caller.
    pub response: R,
    /// Calls that revert this command, in execution order. Usually one call; build them with
    /// [`inverse_call`]. Empty only for no-op commands.
    pub inverse: Vec<InvokeRequest>,
    /// One-line description for the undo UI ("Deck created: {name}").
    pub summary: I18nMessage,
    /// Events to publish after commit.
    pub events: Vec<DomainEvent>,
}

impl<R> CommandOutcome<R> {
    /// Maps the response, keeping the rest.
    pub fn map<S>(self, f: impl FnOnce(R) -> S) -> CommandOutcome<S> {
        CommandOutcome {
            response: f(self.response),
            inverse: self.inverse,
            summary: self.summary,
            events: self.events,
        }
    }
}

/// Builds an inverse call to method `M` with typed params.
pub fn inverse_call<M: ApiMethod>(params: &M::Request) -> AppResult<InvokeRequest> {
    let params = serde_json::to_value(params).map_err(|e| {
        crate::error::AppError::api(
            fc_api::ErrorCode::Internal,
            format!("serialize inverse: {e}"),
        )
    })?;
    Ok(InvokeRequest {
        method: format!("{}.{}", fc_api::API_VERSION, M::NAME),
        params,
    })
}

/// An oplog entry produced during a dispatch, written by the dispatcher before commit.
#[derive(Debug, Clone)]
pub struct PendingOplogEntry {
    /// Qualified method name.
    pub command: String,
    /// Params as executed.
    pub payload: Value,
    /// Inverse calls.
    pub inverse: Vec<InvokeRequest>,
    /// Summary.
    pub summary: I18nMessage,
}

/// Context of a command execution.
pub struct CommandCtx<'a> {
    /// The write transaction.
    pub tx: &'a mut dyn WriteTx,
    /// Time of this dispatch (same for every nested call).
    pub now: Timestamp,
    /// Id generator.
    pub ids: &'a mut dyn IdGen,
    /// Issuer.
    pub actor: &'a Actor,
    /// UI locale (for built-in data like attribute templates).
    pub locale: Locale,
    /// Batch being written.
    pub batch_id: BatchId,
    /// Kind of the batch (undo/redo handlers switch this).
    pub batch_kind: OplogKind,
    /// For undo/redo: batch being reverted.
    pub reverts_batch_id: Option<BatchId>,
    pub(crate) registry: &'a Registry,
    pub(crate) oplog: Vec<PendingOplogEntry>,
    pub(crate) events: Vec<DomainEvent>,
}

impl CommandCtx<'_> {
    /// Executes another command **inside the same transaction and batch** (used by
    /// `batch.execute`, `oplog.undo`, `oplog.redo` and composite commands). Its oplog entry
    /// (if the method is undoable, or if we are inside an undo/redo batch) and events are
    /// collected into this dispatch. Returns the serialized response.
    pub fn call(&mut self, request: &InvokeRequest) -> AppResult<Value> {
        let registry = self.registry;
        registry.execute_nested(self, request)
    }

    /// Read access through the write transaction.
    pub fn reads(&self) -> &dyn Reads {
        &*self.tx
    }
}

/// Context of a query execution.
pub struct QueryCtx<'a> {
    /// Read snapshot.
    pub tx: &'a dyn Reads,
    /// Current time.
    pub now: Timestamp,
    /// UI locale.
    pub locale: Locale,
    pub(crate) registry: &'a Registry,
}

/// Handles command `M`.
pub trait CommandHandler<M: ApiMethod>: Send + Sync + 'static {
    /// Stateless validation, run before the transaction opens. Default: accept.
    fn validate(&self, _req: &M::Request) -> AppResult<()> {
        Ok(())
    }

    /// Executes inside the write transaction.
    fn execute(
        &self,
        req: M::Request,
        ctx: &mut CommandCtx<'_>,
    ) -> AppResult<CommandOutcome<M::Response>>;
}

impl<M, F> CommandHandler<M> for F
where
    M: ApiMethod,
    F: Fn(M::Request, &mut CommandCtx<'_>) -> AppResult<CommandOutcome<M::Response>>
        + Send
        + Sync
        + 'static,
{
    fn execute(
        &self,
        req: M::Request,
        ctx: &mut CommandCtx<'_>,
    ) -> AppResult<CommandOutcome<M::Response>> {
        self(req, ctx)
    }
}

/// Handles query `M`.
pub trait QueryHandler<M: ApiMethod>: Send + Sync + 'static {
    /// Runs on a read snapshot.
    fn run(&self, req: M::Request, ctx: &QueryCtx<'_>) -> AppResult<M::Response>;
}

impl<M, F> QueryHandler<M> for F
where
    M: ApiMethod,
    F: Fn(M::Request, &QueryCtx<'_>) -> AppResult<M::Response> + Send + Sync + 'static,
{
    fn run(&self, req: M::Request, ctx: &QueryCtx<'_>) -> AppResult<M::Response> {
        self(req, ctx)
    }
}
