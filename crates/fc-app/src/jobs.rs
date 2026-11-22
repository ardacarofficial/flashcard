//! Background jobs (docs/03 §12). Long work (import/export, backups, later AI batches and
//! optimization) runs as a job:
//!
//! * a **command** enqueues it ([`enqueue`]: a `queued` row in `jobs` + a `job_progress`
//!   event), so starting a job is validated like any other write;
//! * a worker claims it ([`App::run_next_job`]; [`spawn_worker`] runs one in the background,
//!   tests call [`App::run_pending_jobs`] for deterministic runs) and calls the module's
//!   [`JobHandler`];
//! * the handler reports progress (events only — the writer may be busy), checks
//!   [`JobContext::is_cancelled`] between units of work, and writes data **only through
//!   commands** ([`JobContext::invoke`]), so everything stays validated, logged and undoable;
//! * `jobs.cancel` sets the job's [`CancelToken`] *before* waiting for the writer, so even a
//!   long command running for the job can observe it and roll back;
//! * the final status (`succeeded`/`failed`/`cancelled`), result and error are persisted.
//!
//! Jobs left `queued`/`running` when the app closed are marked `interrupted` at startup.
//!
//! **Lanes.** Each handler names a lane ([`JobHandler::lane`]); [`spawn_worker`] runs one
//! thread per lane, so a long AI request (seconds to minutes of waiting on the network) never
//! holds up an import or export, and vice versa. Jobs of one lane run one at a time, oldest
//! first. Writes still go through the single writer.

use crate::app::{App, OplogMode, UndoGroup};
use crate::error::{AppError, AppResult};
use fc_api::v1::common::I18nMessage;
use fc_api::v1::events::DomainEvent;
use fc_api::v1::jobs::{JobDto, JobStatus};
use fc_api::v1::oplog::Actor;
use fc_api::{ApiError, ApiMethod, ErrorCode, InvokeRequest};
use fc_core::i18n::Locale;
use fc_core::{JobId, Timestamp};
use fc_storage::repo::JobRecord;
use fc_storage::ReadTx;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

/// Cooperative cancellation flag shared between the queue and a running job.
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<TokenState>);

#[derive(Debug, Default)]
struct TokenState {
    cancelled: AtomicBool,
    /// Test hook: cancel at the n-th [`CancelToken::check`] (0 = off).
    #[cfg(test)]
    countdown: std::sync::atomic::AtomicU32,
}

impl CancelToken {
    /// New, not cancelled.
    pub fn new() -> Self {
        Self::default()
    }

    /// A token that cancels itself at its `n`-th [`check`](Self::check) — lets tests stop a
    /// job in the middle of its work deterministically.
    #[cfg(test)]
    pub(crate) fn cancelling_at_check(n: u32) -> Self {
        let t = Self::default();
        t.0.countdown.store(n, Ordering::SeqCst);
        t
    }

    /// Requests cancellation.
    pub fn cancel(&self) {
        self.0.cancelled.store(true, Ordering::SeqCst);
    }

    /// Whether cancellation was requested. Jobs check this between units of work.
    pub fn is_cancelled(&self) -> bool {
        self.0.cancelled.load(Ordering::SeqCst)
    }

    /// `Err(CONFLICT_JOB_CANCELLED)` once cancelled.
    pub fn check(&self) -> AppResult<()> {
        #[cfg(test)]
        {
            let left = self.0.countdown.load(Ordering::SeqCst);
            if left > 0 {
                self.0.countdown.store(left - 1, Ordering::SeqCst);
                if left == 1 {
                    self.cancel();
                }
            }
        }
        if self.is_cancelled() {
            Err(cancelled())
        } else {
            Ok(())
        }
    }
}

/// The error a cancelled job (or a command it runs) stops with.
pub fn cancelled() -> AppError {
    AppError::api(ErrorCode::ConflictJobCancelled, "job cancelled")
}

/// Receives progress from a running job.
pub trait ProgressSink: Send + Sync {
    /// `progress` in 0.0–1.0 (`None` = indeterminate).
    fn report(&self, progress: Option<f32>, message: Option<I18nMessage>);
}

/// What a job sees while running.
pub struct JobContext<'a> {
    /// Job id.
    pub job_id: JobId,
    /// Cancellation flag.
    pub cancel: CancelToken,
    /// Progress reporting.
    pub progress: &'a dyn ProgressSink,
    app: &'a App,
}

impl JobContext<'_> {
    /// Whether cancellation was requested.
    pub fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }

    /// Reports progress (0–1) with an i18n message key.
    pub fn report(&self, progress: Option<f32>, key: &str) {
        self.progress
            .report(progress, Some(I18nMessage::key(key.to_owned())));
    }

    /// Current time from the app clock.
    pub fn now(&self) -> Timestamp {
        self.app.now()
    }

    /// Whether module `id` is (still) enabled — a job that acts for a module checks this
    /// before doing anything outside the app (a request queued before the module was turned
    /// off must not go out).
    pub fn module_enabled(&self, id: &str) -> bool {
        self.app.is_module_enabled(id)
    }

    /// `Err(CONFLICT_MODULE_DISABLED)` unless module `id` is enabled.
    pub fn ensure_module_enabled(&self, id: &str) -> AppResult<()> {
        if self.module_enabled(id) {
            return Ok(());
        }
        Err(AppError::Api(
            ApiError::new(
                ErrorCode::ConflictModuleDisabled,
                format!("module {id} is disabled"),
            )
            .with_detail("module", id.to_owned()),
        ))
    }

    /// Publishes events for this job (no database write), e.g. streamed partial output.
    pub fn publish(&self, events: Vec<DomainEvent>) {
        self.app.publish(events, None);
    }

    /// Folder of the collection (see [`App::data_dir`]).
    pub fn data_dir(&self) -> Option<&std::path::Path> {
        self.app.data_dir()
    }

    /// UI locale.
    pub fn locale(&self) -> Locale {
        self.app.locale()
    }

    /// A read snapshot (exports read through one snapshot for consistency).
    pub fn read(&self) -> AppResult<Box<dyn ReadTx + '_>> {
        Ok(self.app.store().read()?)
    }

    /// Executes method `M` for this job: like a client call, except that internal methods
    /// are allowed (a job is trusted app code). Commands run in their own transaction and
    /// undo batch.
    pub fn invoke<M: ApiMethod>(&self, request: &M::Request) -> AppResult<M::Response> {
        let params = serde_json::to_value(request)
            .map_err(|e| AppError::api(ErrorCode::Internal, e.to_string()))?;
        let request = InvokeRequest {
            method: format!("{}.{}", fc_api::API_VERSION, M::NAME),
            params,
        };
        let value = self
            .app
            .invoke_trusted(&request, &Actor::System)
            .map_err(AppError::Api)?;
        serde_json::from_value(value).map_err(|e| AppError::api(ErrorCode::Internal, e.to_string()))
    }

    /// Starts an undo step that spans several commands (each in its own transaction); see
    /// [`UndoGroup`]. Nothing reaches the oplog until [`JobContext::commit_group`].
    ///
    /// The group is crash-safe ([`UndoGroup`]): `rollback` (method `R`, an internal command
    /// whose request has a `calls` list) removes what it committed if the app stops before
    /// [`JobContext::commit_group`] / [`JobContext::discard_group`].
    pub(crate) fn begin_group<R: ApiMethod>(&self, rollback: &R::Request) -> AppResult<UndoGroup> {
        self.app
            .begin_group(Some(self.job_id), &request_of::<R>(rollback)?)
    }

    /// Like [`JobContext::invoke`] for a command, but its oplog entries join `group` instead
    /// of becoming their own undo step.
    pub(crate) fn invoke_in_group<M: ApiMethod>(
        &self,
        group: &mut UndoGroup,
        request: &M::Request,
    ) -> AppResult<M::Response> {
        let value = self.app.invoke_with(
            &request_of::<M>(request)?,
            &Actor::System,
            OplogMode::Defer(group),
        )?;
        serde_json::from_value(value).map_err(|e| AppError::api(ErrorCode::Internal, e.to_string()))
    }

    /// Reverts `group` with compensating command `M` (records no undo step) and ends the
    /// group: its marker is removed in the same transaction. An empty group just ends.
    pub(crate) fn discard_group<M: ApiMethod>(
        &self,
        group: &UndoGroup,
        request: &M::Request,
    ) -> AppResult<()> {
        if group.is_empty() {
            return self.app.close_empty_group(group);
        }
        self.app.invoke_with(
            &request_of::<M>(request)?,
            &Actor::System,
            OplogMode::Discard(Some(group.batch_id())),
        )?;
        Ok(())
    }

    /// Writes `group` as one undo step (retried briefly if the writer is busy).
    pub(crate) fn commit_group(
        &self,
        group: &UndoGroup,
        summary: Option<I18nMessage>,
    ) -> AppResult<()> {
        retry(|| {
            self.app
                .commit_group(group, &Actor::System, summary.clone())
        })
    }

    /// Between chunks of a long job: lets commands waiting for the writer go first (up to
    /// ~2 s), so a long import never makes study or editing time out.
    pub(crate) fn yield_writer(&self) {
        for _ in 0..2000 {
            if !self.app.writers_waiting() {
                return;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

fn request_of<M: ApiMethod>(request: &M::Request) -> AppResult<InvokeRequest> {
    let params = serde_json::to_value(request)
        .map_err(|e| AppError::api(ErrorCode::Internal, e.to_string()))?;
    Ok(InvokeRequest {
        method: format!("{}.{}", fc_api::API_VERSION, M::NAME),
        params,
    })
}

/// Runs `f` up to [`RETRIES`] times while it fails (e.g. the writer stays busy past its
/// timeout), pausing longer each time (~1.5 s in all).
pub(crate) fn retry<T>(mut f: impl FnMut() -> AppResult<T>) -> AppResult<T> {
    let mut last = None;
    for attempt in 0..RETRIES {
        match f() {
            Ok(v) => return Ok(v),
            Err(e) => {
                last = Some(e);
                std::thread::sleep(Duration::from_millis(50 << attempt.min(4)));
            }
        }
    }
    Err(last.unwrap_or_else(|| AppError::api(ErrorCode::Internal, "retry")))
}

/// Attempts of [`retry`].
const RETRIES: u32 = 6;

/// The lane jobs run in unless their handler names another.
pub const MAIN_LANE: &str = "main";

/// Implements one job kind. Registered by its module ([`crate::Registry::job`]).
pub trait JobHandler: Send + Sync {
    /// Kind (`io.import`, `fsrs.optimize`, …).
    fn kind(&self) -> &'static str;
    /// Worker lane ([`MAIN_LANE`] by default): jobs of different lanes run side by side.
    fn lane(&self) -> &'static str {
        MAIN_LANE
    }
    /// Runs to completion, failure or cancellation; returns the result JSON.
    fn run(&self, payload: Value, ctx: &JobContext<'_>) -> AppResult<Value>;
}

/// Something that can execute API calls (implemented by [`App`]).
pub trait Invoker: Send + Sync {
    /// Executes `request` as `actor`.
    fn invoke_request(&self, request: &InvokeRequest, actor: &Actor) -> Result<Value, ApiError>;
}

/// In-memory job state of an [`App`]: cancel tokens of running jobs and the wake-up signal of
/// the worker.
#[derive(Default)]
pub(crate) struct JobRuntime {
    running: Mutex<HashMap<JobId, CancelToken>>,
    /// Ids cancelled before their worker registered a token.
    early: Mutex<Vec<JobId>>,
    /// Bumped whenever a job is queued; each worker remembers the value it last saw, so no
    /// lane misses a wake-up another lane consumed.
    signal: Mutex<u64>,
    wake: Condvar,
}

impl JobRuntime {
    /// Sets the token of `id` (if running) — called before `jobs.cancel` waits for the writer.
    pub(crate) fn request_cancel(&self, id: JobId) {
        if let Ok(running) = self.running.lock() {
            if let Some(t) = running.get(&id) {
                t.cancel();
                return;
            }
        }
        if let Ok(mut early) = self.early.lock() {
            early.push(id);
        }
    }

    fn start(&self, id: JobId) -> CancelToken {
        let token = CancelToken::new();
        if let Ok(mut early) = self.early.lock() {
            if early.contains(&id) {
                token.cancel();
                early.retain(|e| *e != id);
            }
        }
        if let Ok(mut running) = self.running.lock() {
            running.insert(id, token.clone());
        }
        token
    }

    fn finish(&self, id: JobId) {
        if let Ok(mut running) = self.running.lock() {
            running.remove(&id);
        }
    }

    /// Wakes the workers (a job was queued).
    pub(crate) fn notify(&self) {
        if let Ok(mut s) = self.signal.lock() {
            *s = s.wrapping_add(1);
            self.wake.notify_all();
        }
    }

    /// Current wake-up generation.
    fn generation(&self) -> u64 {
        self.signal.lock().map(|s| *s).unwrap_or(0)
    }

    /// Waits until the generation moves past `seen` (or `timeout`).
    fn wait(&self, seen: u64, timeout: Duration) {
        if let Ok(s) = self.signal.lock() {
            if *s == seen {
                let _ = self.wake.wait_timeout(s, timeout);
            }
        }
    }
}

/// Converts a `jobs` row to its DTO.
pub fn job_dto(r: &JobRecord) -> AppResult<JobDto> {
    let status: JobStatus = serde_json::from_value(Value::String(r.status.clone()))
        .map_err(|e| AppError::api(ErrorCode::Internal, e.to_string()))?;
    Ok(JobDto {
        id: r.id,
        kind: r.kind.clone(),
        status,
        progress: r.progress.map(|p| p as f32),
        message: None,
        error: r
            .error
            .as_ref()
            .map(|e| serde_json::from_value(e.clone()))
            .transpose()
            .map_err(|e| AppError::api(ErrorCode::Internal, e.to_string()))?,
        result: r.result.clone(),
        created_at: r.created_at,
        updated_at: r.updated_at,
    })
}

/// Enqueues a job from inside a command: writes the `queued` row and the `job_progress` event
/// (which wakes the worker after commit). Returns the job.
pub fn enqueue(ctx: &mut crate::CommandCtx<'_>, kind: &str, payload: Value) -> AppResult<JobDto> {
    let record = JobRecord {
        id: JobId::generate(ctx.ids),
        kind: kind.to_owned(),
        status: "queued".into(),
        progress: None,
        payload,
        result: None,
        error: None,
        created_at: ctx.now,
        updated_at: ctx.now,
    };
    ctx.tx.upsert_job(&record)?;
    let dto = job_dto(&record)?;
    ctx.events
        .push(DomainEvent::JobProgress { job: dto.clone() });
    Ok(dto)
}

/// Emits `job_progress` events for a running job (no database write: the job's own commands
/// may hold the writer).
struct EventSink<'a> {
    app: &'a App,
    job: Mutex<JobDto>,
}

impl ProgressSink for EventSink<'_> {
    fn report(&self, progress: Option<f32>, message: Option<I18nMessage>) {
        let snapshot = match self.job.lock() {
            Ok(mut j) => {
                j.progress = progress.map(|p| p.clamp(0.0, 1.0));
                j.message = message;
                j.updated_at = self.app.now();
                j.clone()
            }
            Err(_) => return,
        };
        self.app
            .publish(vec![DomainEvent::JobProgress { job: snapshot }], None);
    }
}

impl App {
    /// Claims the oldest queued job (any lane) and runs it on this thread. `Ok(None)` if none
    /// is queued.
    pub fn run_next_job(&self) -> AppResult<Option<JobDto>> {
        let claimed = {
            let mut tx = self.store().write()?;
            let job = tx.claim_next_job(self.now())?;
            tx.commit()?;
            job
        };
        self.run_claimed(claimed)
    }

    /// Claims the oldest queued job of `lane` and runs it on this thread.
    pub fn run_next_job_in(&self, lane: &str) -> AppResult<Option<JobDto>> {
        let (kinds, only) = self.registry().lane_kinds(lane);
        let claimed = {
            let mut tx = self.store().write()?;
            let job = tx.claim_next_job_in(self.now(), &kinds, only)?;
            tx.commit()?;
            job
        };
        self.run_claimed(claimed)
    }

    fn run_claimed(&self, claimed: Option<JobRecord>) -> AppResult<Option<JobDto>> {
        let Some(record) = claimed else {
            return Ok(None);
        };
        let token = self.job_runtime().start(record.id);
        let running = job_dto(&record)?;
        self.publish(
            vec![DomainEvent::JobProgress {
                job: running.clone(),
            }],
            None,
        );
        let sink = EventSink {
            app: self,
            job: Mutex::new(running),
        };
        let ctx = JobContext {
            job_id: record.id,
            cancel: token.clone(),
            progress: &sink,
            app: self,
        };
        let outcome = match self.registry().job_handler(&record.kind) {
            Some(h) => h.run(record.payload.clone(), &ctx),
            None => Err(AppError::api(
                ErrorCode::InternalNotImplemented,
                format!("no handler for job kind {}", record.kind),
            )),
        };
        self.job_runtime().finish(record.id);
        let (status, result, error) = match outcome {
            Ok(result) => ("succeeded", Some(result), None),
            Err(e) => {
                let api = e.into_api();
                let status = if token.is_cancelled() || api.code == ErrorCode::ConflictJobCancelled
                {
                    "cancelled"
                } else {
                    "failed"
                };
                let error = serde_json::to_value(&api)
                    .map_err(|e| AppError::api(ErrorCode::Internal, e.to_string()))?;
                (status, None, Some(error))
            }
        };
        // The final status must land, or the job would show as running forever: retried while
        // the writer is busy; failing that, the next start marks it interrupted.
        let done = retry(|| {
            let mut tx = self.store().write()?;
            let mut done = tx.get_job(record.id)?.unwrap_or_else(|| record.clone());
            done.updated_at = self.now();
            done.status = status.into();
            if status == "succeeded" {
                done.progress = Some(1.0);
            }
            done.result = result.clone();
            done.error = error.clone();
            tx.upsert_job(&done)?;
            tx.commit()?;
            Ok(done)
        })?;
        let dto = job_dto(&done)?;
        self.publish(vec![DomainEvent::JobProgress { job: dto.clone() }], None);
        Ok(Some(dto))
    }

    /// Runs queued jobs on this thread until none is left; returns how many ran. (Tests and
    /// the CLI use this; the desktop app runs [`spawn_worker`].)
    pub fn run_pending_jobs(&self) -> AppResult<usize> {
        let mut n = 0;
        while self.run_next_job()?.is_some() {
            n += 1;
        }
        Ok(n)
    }
}

/// The background worker threads running queued jobs, one per lane. Stop (after their
/// current job) when dropped.
pub struct JobWorker {
    stop: Arc<AtomicBool>,
    app: Arc<App>,
    handles: Vec<JoinHandle<()>>,
}

impl JobWorker {
    /// Lanes served, one thread each.
    pub fn lanes(&self) -> usize {
        self.handles.len()
    }
}

/// Starts the workers of `app`: one thread per lane ([`JobHandler::lane`]), each running its
/// lane's queued jobs one at a time, waking up when a job is queued and every few seconds as
/// a fallback. While idle, the main lane queues automatic backups when due
/// ([`crate::modules::io::auto_backup_tick`]).
pub fn spawn_worker(app: Arc<App>) -> std::io::Result<JobWorker> {
    let stop = Arc::new(AtomicBool::new(false));
    let mut worker = JobWorker {
        stop: stop.clone(),
        app: app.clone(),
        handles: Vec::new(),
    };
    for lane in app.registry().lanes() {
        let (a, s) = (app.clone(), stop.clone());
        let handle = std::thread::Builder::new()
            .name(format!("fc-jobs-{lane}"))
            .spawn(move || {
                while !s.load(Ordering::SeqCst) {
                    let seen = a.job_runtime().generation();
                    match a.run_next_job_in(lane) {
                        Ok(Some(_)) => continue,
                        Ok(None) | Err(_) => {
                            // Idle: queue an automatic backup if one is due (v1.4; it looks
                            // at most every few minutes, best effort).
                            if lane == MAIN_LANE {
                                let _ = crate::modules::io::auto_backup_tick(&a, false);
                            }
                            a.job_runtime().wait(seen, Duration::from_secs(5));
                        }
                    }
                }
            })?;
        // On an error the threads already started stop when `worker` drops.
        worker.handles.push(handle);
    }
    Ok(worker)
}

impl Drop for JobWorker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.app.job_runtime().notify();
        for h in self.handles.drain(..) {
            let _ = h.join();
        }
    }
}
