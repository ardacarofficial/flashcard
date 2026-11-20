//! `updates` module (v1.0, docs/02 §34): signed automatic updates, **asking the user**.
//!
//! The transport installs an [`Updater`] ([`App::set_updater`]): the desktop app wraps
//! `tauri-plugin-updater`, which fetches the update manifest (`latest.json`, a GitHub
//! release asset), downloads the installer and verifies its Ed25519 (minisign) signature
//! against the public key built into the app — an artifact not signed with the project's
//! private key is refused (`PROVIDER_UPDATE_SIGNATURE`). Without an updater (CLI, tests,
//! development builds) `updates.check` answers `PROVIDER_UPDATE_UNSUPPORTED`.
//!
//! Flow: `updates.check` (network, only when the user asks — or with the opt-in setting
//! `updates.check_on_start`, privacy: the app connects nowhere unasked) → `updates.install`
//! (a cancellable job in its own lane: download + verify) → `updates.apply` (install and
//! restart; the run is ended cleanly first, so no recovery offer follows). Nothing is ever
//! installed without the user's click.

use crate::error::{AppError, AppResult};
use crate::handler::{CommandCtx, CommandOutcome, QueryCtx};
use crate::jobs::{cancelled, enqueue, JobContext, JobHandler};
use crate::module::ModuleDescriptor;
use crate::modules::maintenance::{done, APP_VERSION};
use crate::registry::Registry;
use crate::settings::resolve_stored;
use crate::App;
use fc_api::v1::common::Empty;
use fc_api::v1::jobs::JobDto;
use fc_api::v1::maintenance::{UpdateInfoDto, UpdatePhase, UpdateStatus};
use fc_api::v1::methods::*;
use fc_api::{ApiError, ApiMethod, ErrorCode};
use fc_core::settings::{SettingDef, SettingKind, SettingScope, SettingScopeKind};
use fc_core::{JobId, Timestamp};
use fc_storage::Reads;
use serde_json::{json, Value};
use std::sync::{Arc, Mutex, RwLock};

/// Module id.
pub const ID: &str = "updates";
/// Setting: check for an update when the app starts (opt-in).
pub const CHECK_ON_START: &str = "updates.check_on_start";
/// Job kind of the download.
pub const JOB_KIND: &str = UpdatesInstall::NAME;
/// Worker lane of the download (never holds up imports or AI requests).
pub const LANE: &str = "updates";

/// Why an update step failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateError {
    /// Server unreachable or answered with an error.
    Network(String),
    /// The download's signature did not verify against the app's update key.
    Signature(String),
    /// No update to install (check first).
    NotAvailable,
    /// Nothing downloaded.
    NotReady,
    /// The download was cancelled.
    Cancelled,
    /// Anything else (writing the installer, starting it).
    Other(String),
}

impl UpdateError {
    fn into_api(self) -> ApiError {
        match self {
            Self::Network(m) => ApiError::new(ErrorCode::ProviderUpdateNetwork, m),
            Self::Signature(m) => ApiError::new(ErrorCode::ProviderUpdateSignature, m).retryable(false),
            Self::NotAvailable => ApiError::new(ErrorCode::ConflictUpdateNotAvailable, "no update available"),
            Self::NotReady => ApiError::new(ErrorCode::ConflictUpdateNotReady, "update not downloaded"),
            Self::Cancelled => cancelled().into_api(),
            Self::Other(m) => ApiError::new(ErrorCode::ProviderFailed, m),
        }
    }
}

/// What the module needs from the platform's updater.
pub trait Updater: Send + Sync {
    /// Asks the update server whether a newer version than the running one exists.
    fn check(&self) -> Result<Option<UpdateInfoDto>, UpdateError>;
    /// Downloads the update found by the last [`Updater::check`] and verifies its signature,
    /// keeping the verified bytes for [`Updater::apply`]. Reports `(bytes so far, total)`;
    /// stops with [`UpdateError::Cancelled`] once `cancelled()` is true.
    fn download(
        &self,
        progress: &dyn Fn(u64, Option<u64>),
        cancelled: &dyn Fn() -> bool,
    ) -> Result<(), UpdateError>;
    /// Installs the verified download and restarts the app. Must return quickly (the caller
    /// is a command holding the writer): schedule the install on another thread.
    fn apply(&self) -> Result<(), UpdateError>;
}

#[derive(Debug, Clone)]
struct State {
    last_checked_at: Option<Timestamp>,
    available: Option<UpdateInfoDto>,
    phase: UpdatePhase,
    job: Option<JobId>,
    last_error: Option<ApiError>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            last_checked_at: None,
            available: None,
            phase: UpdatePhase::Idle,
            job: None,
            last_error: None,
        }
    }
}

/// The updater and the state of the update flow (in memory: a restart starts over).
#[derive(Default)]
pub struct UpdateServices {
    updater: RwLock<Option<Arc<dyn Updater>>>,
    state: Mutex<State>,
}

impl UpdateServices {
    fn updater(&self) -> Option<Arc<dyn Updater>> {
        self.updater.read().ok().and_then(|u| u.clone())
    }

    fn state(&self) -> State {
        self.state.lock().map(|s| s.clone()).unwrap_or_default()
    }

    fn update(&self, f: impl FnOnce(&mut State)) {
        if let Ok(mut s) = self.state.lock() {
            f(&mut s);
        }
    }

    fn require(&self) -> AppResult<Arc<dyn Updater>> {
        self.updater().ok_or_else(|| {
            AppError::api(
                ErrorCode::ProviderUpdateUnsupported,
                "this build cannot update itself",
            )
        })
    }
}

/// Descriptor (UI: a section of the maintenance screen and the update banner).
pub fn descriptor() -> ModuleDescriptor {
    ModuleDescriptor {
        id: ID,
        name_key: "modules.updates.name",
        description_key: "modules.updates.description",
        enabled_by_default: true,
        can_disable: true,
        settings: vec![SettingDef {
            key: CHECK_ON_START.to_owned(),
            kind: SettingKind::Bool,
            default: json!(false),
            label_key: format!("settings.{CHECK_ON_START}.label"),
            description_key: Some(format!("settings.{CHECK_ON_START}.description")),
            advanced: false,
            scopes: vec![SettingScopeKind::App],
        }],
        ui: vec![],
    }
}

/// Registers the module, its methods and the download job.
pub fn register(reg: &mut Registry) {
    let services = Arc::new(UpdateServices::default());
    reg.module(descriptor());
    reg.provide(Arc::clone(&services));
    reg.query::<UpdatesStatus>(status);
    reg.query::<UpdatesCheck>(check);
    reg.command::<UpdatesInstall>(install);
    reg.command::<UpdatesApply>(apply);
    reg.job(Arc::new(DownloadJob(services)));
}

impl App {
    /// Installs the platform updater (the desktop release build).
    pub fn set_updater(&self, updater: Arc<dyn Updater>) {
        if let Some(s) = self.registry().extension::<UpdateServices>() {
            if let Ok(mut u) = s.updater.write() {
                *u = Some(updater);
            }
        }
    }
}

fn services(registry: &Registry) -> AppResult<Arc<UpdateServices>> {
    registry
        .extension::<UpdateServices>()
        .ok_or_else(|| AppError::api(ErrorCode::Internal, "update services missing"))
}

fn status_of(svc: &UpdateServices, reads: &dyn Reads) -> AppResult<UpdateStatus> {
    let s = svc.state();
    let check_on_start = resolve_stored(reads, &SettingScope::App, CHECK_ON_START)?
        .and_then(|(v, _)| v.as_bool())
        .unwrap_or(false);
    let supported = svc.updater().is_some();
    Ok(UpdateStatus {
        current_version: APP_VERSION.to_owned(),
        supported,
        check_on_start,
        check_due: supported
            && check_on_start
            && s.last_checked_at.is_none()
            && s.last_error.is_none(),
        last_checked_at: s.last_checked_at,
        available: s.available,
        phase: s.phase,
        install_job: s.job,
        last_error: s.last_error,
    })
}

fn status(_: Empty, ctx: &QueryCtx<'_>) -> AppResult<UpdateStatus> {
    status_of(&*services(ctx.registry)?, ctx.tx)
}

fn check(_: Empty, ctx: &QueryCtx<'_>) -> AppResult<UpdateStatus> {
    let svc = services(ctx.registry)?;
    let updater = svc.require()?;
    log::info!(target: "fc_app::updates", "checking for an update (running {APP_VERSION})");
    match updater.check() {
        Ok(found) => {
            match &found {
                Some(u) => log::info!(target: "fc_app::updates", "update {} available", u.version),
                None => log::info!(target: "fc_app::updates", "up to date"),
            }
            svc.update(|s| {
                s.last_checked_at = Some(ctx.now);
                s.last_error = None;
                let same = found.as_ref().map(|f| &f.version) == s.available.as_ref().map(|a| &a.version);
                // A download in progress, or one done for this very version, stays.
                let keep = s.phase == UpdatePhase::Downloading || (s.phase == UpdatePhase::Ready && same);
                if !keep {
                    s.phase = UpdatePhase::Idle;
                }
                s.available = found;
            });
            status_of(&svc, ctx.tx)
        }
        Err(e) => {
            let api = e.into_api();
            log::warn!(target: "fc_app::updates", "update check failed: {}", api.code);
            svc.update(|s| s.last_error = Some(api.clone()));
            Err(AppError::Api(api))
        }
    }
}

fn install(_: Empty, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<JobDto>> {
    let svc = services(ctx.registry)?;
    svc.require()?;
    let s = svc.state();
    let Some(available) = s.available else {
        return Err(AppError::Api(UpdateError::NotAvailable.into_api()));
    };
    if s.phase == UpdatePhase::Downloading {
        return Err(AppError::api(ErrorCode::ConflictUpdateNotReady, "already downloading"));
    }
    let job = enqueue(ctx, JOB_KIND, json!({ "version": available.version }))?;
    svc.update(|s| {
        s.phase = UpdatePhase::Downloading;
        s.job = Some(job.id);
        s.last_error = None;
    });
    Ok(done::<UpdatesInstall, _>(job))
}

fn apply(_: Empty, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<UpdateStatus>> {
    let svc = services(ctx.registry)?;
    let updater = svc.require()?;
    if svc.state().phase != UpdatePhase::Ready {
        return Err(AppError::Api(UpdateError::NotReady.into_api()));
    }
    log::info!(target: "fc_app::updates", "installing the update and restarting");
    updater.apply().map_err(|e| AppError::Api(e.into_api()))?;
    Ok(done::<UpdatesApply, _>(status_of(&svc, ctx.reads())?))
}

/// The download (`updates.install`): verified bytes are kept by the updater.
struct DownloadJob(Arc<UpdateServices>);

impl JobHandler for DownloadJob {
    fn kind(&self) -> &'static str {
        JOB_KIND
    }

    fn lane(&self) -> &'static str {
        LANE
    }

    fn run(&self, _payload: Value, ctx: &JobContext<'_>) -> AppResult<Value> {
        let svc = &self.0;
        let result = (|| {
            ctx.ensure_module_enabled(ID)?;
            let updater = svc.require()?;
            ctx.report(Some(0.0), "updates.downloading");
            updater
                .download(
                    &|done, total| {
                        let p = total.filter(|t| *t > 0).map(|t| (done as f64 / t as f64) as f32);
                        ctx.report(p, "updates.downloading");
                    },
                    &|| ctx.is_cancelled(),
                )
                .map_err(|e| AppError::Api(e.into_api()))
        })();
        let version = svc.state().available.map(|a| a.version);
        match result {
            Ok(()) => {
                log::info!(target: "fc_app::updates", "update downloaded and verified");
                svc.update(|s| {
                    s.phase = UpdatePhase::Ready;
                    s.job = None;
                });
                Ok(json!({ "version": version }))
            }
            Err(e) => {
                let api = e.into_api();
                log::warn!(target: "fc_app::updates", "update download failed: {}", api.code);
                svc.update(|s| {
                    s.phase = UpdatePhase::Idle;
                    s.job = None;
                    if api.code != ErrorCode::ConflictJobCancelled {
                        s.last_error = Some(api.clone());
                    }
                });
                Err(AppError::Api(api))
            }
        }
    }
}
