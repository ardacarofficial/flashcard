//! `maintenance` module (v1.0, docs/02 §34, track O): crash recovery, database integrity,
//! local logs and their export, form drafts.
//!
//! * **Run marker** ([`run`]): the desktop app records each run; a run that did not end
//!   cleanly (crash, kill, power loss) is noticed at the next start.
//! * **Crash reports** ([`crash`]): a panic hook writes a redacted report into
//!   `<data dir>/crash/`. Never sent anywhere.
//! * **Logs** ([`logging`]): `<data dir>/logs/flashcard.log`, rotated by size, every line
//!   redacted before it is written.
//! * **Recovery** ([`recovery`]): after an unclean shutdown the app runs SQLite's quick
//!   integrity check at start and `maintenance.recovery.get` offers what is left: the study
//!   session to resume (sessions are persistent, v0.2), the form drafts to restore, the crash
//!   report, the integrity verdict. `maintenance.recovery.dismiss` hides it for this run.
//! * **Drafts** ([`drafts`]): forms autosave into `drafts` (migration 1050); scratch state,
//!   not undo steps.
//! * **Log export** ([`export`]): logs + crash reports + a system summary, redacted again
//!   (including the exact API keys the app holds), into a `.zip` the user chose.
//!
//! The module cannot be turned off: it is the safety net the other modules rely on. It is
//! out of reach of plugins (`maintenance` is a forbidden area), MCP, automation rules and the
//! local HTTP API (none of them hold a `maintenance:*` permission).
//!
//! Only the owning desktop app calls [`App::start_maintenance`]; in the CLI and tests without
//! it the methods still work (no log folder, no previous run).

pub mod crash;
pub mod drafts;
pub mod export;
pub mod logging;
pub mod recovery;
pub mod rescue;
pub mod run;
pub mod smoke;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod kill_tests;
#[cfg(test)]
mod rescue_tests;

use crate::module::{main_menu, ModuleDescriptor};
use crate::registry::Registry;
use crate::App;
use fc_api::v1::maintenance::{CrashReportDto, IntegrityDto, RunInfoDto};
use fc_api::v1::methods::*;
use fc_core::settings::{SettingDef, SettingKind, SettingScopeKind};
use logging::FileLog;
use run::RunStart;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, RwLock};

/// Module id.
pub const ID: &str = "maintenance";
/// Version of this build (= workspace version = installer version).
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Setting: log level (`error`, `warn`, `info`, `debug`).
pub const LOG_LEVEL: &str = "maintenance.log_level";
/// Setting: size (KiB) at which the log file rotates.
pub const LOG_MAX_KB: &str = "maintenance.log_max_kb";
/// Setting: rotated log files kept.
pub const LOG_FILES: &str = "maintenance.log_files";
/// Setting: run the quick integrity check at start after an unclean shutdown.
pub const CHECK_AFTER_CRASH: &str = "maintenance.check_after_crash";

/// Sub-folder of the data folder holding the logs.
pub const LOG_DIR: &str = "logs";
/// Sub-folder of the data folder holding crash reports.
pub const CRASH_DIR: &str = "crash";

fn setting(key: &str, kind: SettingKind, default: serde_json::Value, advanced: bool) -> SettingDef {
    SettingDef {
        key: key.to_owned(),
        kind,
        default,
        label_key: format!("settings.{key}.label"),
        description_key: Some(format!("settings.{key}.description")),
        advanced,
        scopes: vec![SettingScopeKind::App],
    }
}

/// Descriptor.
pub fn descriptor() -> ModuleDescriptor {
    ModuleDescriptor {
        id: ID,
        name_key: "modules.maintenance.name",
        description_key: "modules.maintenance.description",
        enabled_by_default: true,
        can_disable: false,
        settings: vec![
            setting(
                LOG_LEVEL,
                SettingKind::Choice {
                    options: ["error", "warn", "info", "debug"].map(String::from).to_vec(),
                },
                json!("info"),
                true,
            ),
            setting(
                LOG_MAX_KB,
                SettingKind::Integer {
                    min: Some(64),
                    max: Some(20_480),
                },
                json!(1024),
                true,
            ),
            setting(
                LOG_FILES,
                SettingKind::Integer {
                    min: Some(1),
                    max: Some(20),
                },
                json!(4),
                true,
            ),
            setting(CHECK_AFTER_CRASH, SettingKind::Bool, json!(true), true),
        ],
        ui: vec![main_menu(
            "maintenance.screen",
            "nav.app_maintenance",
            "lifebuoy",
            "/app-maintenance",
            95,
        )],
    }
}

/// Registers the module and its methods.
pub fn register(reg: &mut Registry) {
    reg.module(descriptor());
    reg.provide(Arc::new(MaintenanceServices::default()));
    reg.query::<MaintenanceStatusGet>(recovery::status);
    reg.query::<MaintenanceRecovery>(recovery::recovery);
    reg.command::<MaintenanceRecoveryDismiss>(recovery::dismiss);
    reg.query::<MaintenanceIntegrityCheck>(recovery::integrity_check);
    reg.command::<MaintenanceLogsExport>(export::export_logs);
    reg.command::<MaintenanceCrashReportsClear>(recovery::clear_crash_reports);
    reg.command::<MaintenanceDraftsSave>(drafts::save);
    reg.query::<MaintenanceDraftsGet>(drafts::get);
    reg.query::<MaintenanceDraftsList>(drafts::list);
    reg.command::<MaintenanceDraftsDiscard>(drafts::discard);
}

/// What the desktop app tells the module at start (folders, log, previous run) and what the
/// module learns during the run (integrity verdict, dismissal).
#[derive(Default)]
pub struct MaintenanceServices {
    state: RwLock<State>,
    dismissed: AtomicBool,
}

#[derive(Default, Clone)]
struct State {
    data_dir: Option<PathBuf>,
    log: Option<Arc<FileLog>>,
    run: Option<RunStart>,
    /// Crash reports written during the previous run (newest first).
    crashes: Vec<CrashReportDto>,
    integrity: Option<IntegrityDto>,
}

impl MaintenanceServices {
    fn read(&self) -> State {
        self.state.read().map(|s| s.clone()).unwrap_or_default()
    }

    fn update(&self, f: impl FnOnce(&mut State)) {
        if let Ok(mut s) = self.state.write() {
            f(&mut s);
        }
    }

    /// The collection's folder, when known.
    pub fn data_dir(&self) -> Option<PathBuf> {
        self.read().data_dir
    }

    /// Folder of crash reports (`<data dir>/crash`).
    pub fn crash_dir(&self) -> Option<PathBuf> {
        self.data_dir().map(|d| d.join(CRASH_DIR))
    }

    /// The log file, when the app writes one.
    pub fn log(&self) -> Option<Arc<FileLog>> {
        self.read().log
    }

    fn previous_run(&self) -> Option<RunInfoDto> {
        self.read().run.and_then(|r| r.previous).map(|p| RunInfoDto {
            started_at: p.started_at,
            version: p.version,
            clean: p.clean,
        })
    }

    pub(crate) fn set_integrity(&self, dto: IntegrityDto) {
        self.update(|s| s.integrity = Some(dto));
    }
}

/// Outcome of a command that is not an undo step (no inverse, no events).
pub(crate) fn done<M: fc_api::ApiMethod, R>(response: R) -> crate::handler::CommandOutcome<R> {
    crate::handler::CommandOutcome {
        response,
        inverse: Vec::new(),
        summary: crate::support::summary::<M>(),
        events: Vec::new(),
    }
}

/// Folder of the log files inside a data folder.
pub fn log_dir(data_dir: &Path) -> PathBuf {
    data_dir.join(LOG_DIR)
}

impl App {
    /// The maintenance module's services.
    pub fn maintenance(&self) -> Option<Arc<MaintenanceServices>> {
        self.registry().extension::<MaintenanceServices>()
    }

    /// Desktop start (after [`run::RunGuard::begin`] and opening the collection): records the
    /// folders, the log and the previous run, applies the log settings, collects the crash
    /// reports of the previous run and — when it did not end cleanly and
    /// `maintenance.check_after_crash` is on — runs the quick integrity check, logging the
    /// verdict. Never fails the start: problems are logged and offered as recovery.
    pub fn start_maintenance(&self, data_dir: &Path, log: Option<Arc<FileLog>>, run: RunStart) {
        let Some(svc) = self.maintenance() else {
            return;
        };
        let crashes = match &run.previous {
            Some(p) if !p.clean => crash::reports_since(&data_dir.join(CRASH_DIR), p.started_at)
                .into_iter()
                .map(recovery::crash_dto)
                .collect(),
            _ => Vec::new(),
        };
        let unclean = run.previous_unclean();
        svc.update(|s| {
            s.data_dir = Some(data_dir.to_path_buf());
            s.log = log;
            s.run = Some(run);
            s.crashes = crashes;
        });
        self.apply_log_settings();
        log::info!(target: "fc_app::maintenance", "started {APP_VERSION} ({})", crash::os_label());
        if unclean {
            log::warn!(target: "fc_app::maintenance", "the previous run did not end cleanly");
            if recovery::setting_bool(self, CHECK_AFTER_CRASH, true) {
                match recovery::run_integrity_check(self, true) {
                    Ok(dto) if dto.ok => {
                        log::info!(target: "fc_app::maintenance", "integrity check ok ({} ms)", dto.duration_ms);
                    }
                    Ok(dto) => {
                        log::error!(target: "fc_app::maintenance", "integrity check found problems: {:?}", dto.problems);
                    }
                    Err(e) => {
                        log::error!(target: "fc_app::maintenance", "integrity check failed: {}", e.code);
                    }
                }
            }
        }
    }

    /// Applies `maintenance.log_*` settings to the log file (start, and when they change).
    pub fn apply_log_settings(&self) {
        let Some(log) = self.maintenance().and_then(|s| s.log()) else {
            return;
        };
        let level = recovery::setting_str(self, LOG_LEVEL)
            .and_then(|l| logging::parse_level(&l))
            .unwrap_or(logging::LevelFilter::Info);
        let kb = recovery::setting_u64(self, LOG_MAX_KB).unwrap_or(1024);
        let files = recovery::setting_u64(self, LOG_FILES).unwrap_or(4);
        log.set_level(level);
        log.set_limits(kb.saturating_mul(1024), usize::try_from(files).unwrap_or(4));
        // Exact keys the app holds are masked too (defence in depth).
        log.add_secrets(export::known_secrets(self));
    }
}
