//! Background job contract (docs/03 §12). Long work (import/export, AI batches, optimization,
//! database check) runs as a job: persisted in `jobs`, reports progress as `JobProgress`
//! events, is cancellable cooperatively, and is offered for resume after a crash.
//!
//! v0.1 defines the interface only; the worker pool lands with the first real job (v0.5).

use crate::error::AppResult;
use fc_api::v1::common::I18nMessage;
use fc_api::v1::jobs::JobDto;
use fc_api::v1::oplog::Actor;
use fc_api::{ApiError, InvokeRequest};
use fc_core::JobId;
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Cooperative cancellation flag shared between the queue and a running job.
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    /// New, not cancelled.
    pub fn new() -> Self {
        Self::default()
    }

    /// Requests cancellation.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    /// Whether cancellation was requested. Jobs check this between units of work.
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// Receives progress from a running job (persists it and emits `JobProgress`).
pub trait ProgressSink: Send + Sync {
    /// `progress` in 0.0–1.0 (`None` = indeterminate).
    fn report(&self, progress: Option<f32>, message: Option<I18nMessage>);
}

/// Something that can execute API calls (implemented by [`crate::App`]); jobs write data
/// only through commands so everything stays validated, logged and undoable.
pub trait Invoker: Send + Sync {
    /// Executes `request` as `actor`.
    fn invoke_request(&self, request: &InvokeRequest, actor: &Actor) -> Result<Value, ApiError>;
}

/// What a job sees while running.
pub struct JobContext<'a> {
    /// Job id.
    pub job_id: JobId,
    /// Cancellation flag.
    pub cancel: CancelToken,
    /// Progress reporting.
    pub progress: &'a dyn ProgressSink,
    /// API access.
    pub invoker: &'a dyn Invoker,
}

/// Implements one job kind.
pub trait JobHandler: Send + Sync {
    /// Kind (`import.text`, `fsrs.optimize`, …).
    fn kind(&self) -> &'static str;
    /// Runs to completion, failure or cancellation; returns the result JSON.
    fn run(&self, payload: Value, ctx: &JobContext<'_>) -> AppResult<Value>;
}

/// The queue.
pub trait JobQueue: Send + Sync {
    /// Registers a handler for its kind.
    fn register(&self, handler: Arc<dyn JobHandler>);
    /// Persists and schedules a job.
    fn enqueue(&self, kind: &str, payload: Value) -> AppResult<JobId>;
    /// Requests cancellation.
    fn cancel(&self, id: JobId) -> AppResult<JobDto>;
    /// Current snapshot.
    fn get(&self, id: JobId) -> AppResult<JobDto>;
}
