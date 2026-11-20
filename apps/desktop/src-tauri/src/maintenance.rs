//! v1.0 shell side of the `maintenance` and `updates` modules (track O): the run marker, the
//! rotating file log, the panic hook, the `--smoke` self test and the platform updater over
//! `tauri-plugin-updater`. Decisions (what to offer after a crash, when to check, what the
//! update state is) stay in fc-app; this file only wires the platform in.

use fc_api::v1::events::{DomainEvent, EventEnvelope};
use fc_api::v1::maintenance::UpdateInfoDto;
use fc_app::modules::maintenance::logging::{self, FileLog};
use fc_app::modules::maintenance::run::{RunGuard, RunStart};
use fc_app::modules::maintenance::{crash, log_dir, smoke, APP_VERSION, CRASH_DIR};
use fc_app::modules::updates::{UpdateError, Updater};
use fc_core::Clock;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Manager, Runtime};

/// Command-line flag of the self test (CI runs it after a silent install, docs/07 §8).
pub const SMOKE_FLAG: &str = "--smoke";
/// `--smoke-report=<path>`: where the self test writes its result (release builds on Windows
/// have no console).
pub const SMOKE_REPORT_FLAG: &str = "--smoke-report=";

/// What `--smoke` was asked to do: `None` = a normal start.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmokeArgs {
    /// Report file, if given.
    pub report: Option<PathBuf>,
}

/// Parses the process arguments (first one = the program).
pub fn smoke_args<I: IntoIterator<Item = String>>(args: I) -> Option<SmokeArgs> {
    let args: Vec<String> = args.into_iter().skip(1).collect();
    if !args.iter().any(|a| a == SMOKE_FLAG) {
        return None;
    }
    let report = args
        .iter()
        .find_map(|a| a.strip_prefix(SMOKE_REPORT_FLAG))
        .filter(|p| !p.is_empty())
        .map(PathBuf::from);
    Some(SmokeArgs { report })
}

/// Runs the self test in a fresh temporary folder (never the user's collection), writes the
/// report and returns the exit code (0 = passed).
pub fn run_smoke(args: &SmokeArgs) -> i32 {
    let dir = std::env::temp_dir().join(format!("flashcard-smoke-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let (code, text) = match smoke::run(&dir) {
        Ok(steps) => (0, format!("SMOKE OK {APP_VERSION}\n{}\n", steps.join("\n"))),
        Err(e) => (1, format!("SMOKE FAILED {APP_VERSION}\n{e}\n")),
    };
    let _ = std::fs::remove_dir_all(&dir);
    eprintln!("{text}");
    if let Some(path) = &args.report {
        if let Err(e) = std::fs::write(path, &text) {
            eprintln!("fc-desktop: {}: {e}", path.display());
            return 2;
        }
    }
    code
}

/// This run's marker, held in Tauri state until the app exits cleanly.
#[derive(Default)]
pub struct RunState(Mutex<Option<RunGuard>>);

impl RunState {
    /// Ends the run cleanly (normal exit, or right before an update installs). Idempotent.
    pub fn finish(&self) {
        let guard = self.0.lock().ok().and_then(|mut g| g.take());
        if let Some(g) = guard {
            match g.finish() {
                Ok(()) => log::info!(target: "fc_desktop", "run ended cleanly"),
                Err(e) => log::warn!(target: "fc_desktop", "run marker not cleared: {e}"),
            }
        }
    }
}

/// What [`begin`] set up before the collection opens.
pub struct Boot {
    /// The marker (put into [`RunState`]).
    pub state: RunState,
    /// What the previous run left.
    pub start: RunStart,
    /// The log file (`None` if the folder is not writable: the app still starts).
    pub log: Option<Arc<FileLog>>,
}

/// Start of a desktop run in `data_dir`: run marker, file log as the `log` backend, panic
/// hook writing crash reports. Fails only when the marker cannot be written.
pub fn begin(data_dir: &Path) -> std::io::Result<Boot> {
    let clock: Arc<dyn Clock> = Arc::new(fc_app::system::SystemClock);
    let (guard, start) = RunGuard::begin(data_dir, APP_VERSION, clock.now())?;
    let log = match FileLog::open(&log_dir(data_dir), clock.clone()) {
        Ok(log) => {
            logging::install(log.clone());
            Some(log)
        }
        Err(e) => {
            eprintln!("fc-desktop: log folder: {e}");
            None
        }
    };
    crash::install_panic_hook(
        data_dir.join(CRASH_DIR),
        APP_VERSION.to_owned(),
        clock,
        log.clone(),
    );
    Ok(Boot {
        state: RunState(Mutex::new(Some(guard))),
        start,
        log,
    })
}

/// Settings whose change re-applies the log configuration (level, rotation, known keys).
pub fn affects_log(key: &str) -> bool {
    key.starts_with("maintenance.log_") || key.starts_with("ai.")
}

/// Keeps the log configuration in step with the settings (and new AI keys).
pub fn follow_log_settings(app: &crate::ipc::SharedApp) {
    let weak = Arc::downgrade(app);
    app.events().subscribe(Arc::new(move |e: &EventEnvelope| {
        let relevant = match &e.event {
            DomainEvent::SettingChanged { key, .. } => affects_log(key),
            DomainEvent::AiConfigChanged => true,
            _ => false,
        };
        if relevant {
            if let Some(app) = weak.upgrade() {
                std::thread::spawn(move || app.apply_log_settings());
            }
        }
    }));
}

/// `tauri-plugin-updater` behind fc-app's [`Updater`]: the manifest (`latest.json`) comes
/// from the endpoint in `tauri.conf.json`, and every download is verified against the
/// Ed25519 public key there before it is kept.
pub struct TauriUpdater<R: Runtime> {
    handle: AppHandle<R>,
    found: Mutex<Option<tauri_plugin_updater::Update>>,
    bytes: Mutex<Option<Vec<u8>>>,
}

impl<R: Runtime> TauriUpdater<R> {
    /// Over the app's updater plugin.
    pub fn new(handle: AppHandle<R>) -> Self {
        Self {
            handle,
            found: Mutex::new(None),
            bytes: Mutex::new(None),
        }
    }
}

/// Maps the plugin's errors onto the module's: signature problems are refusals, transport
/// problems are retryable network errors.
pub fn map_error(e: &tauri_plugin_updater::Error) -> UpdateError {
    use tauri_plugin_updater::Error as E;
    match e {
        E::Minisign(_)
        | E::Base64(_)
        | E::SignatureUtf8(_)
        | E::SignedVersionMismatch { .. }
        | E::MissingSignedVersion => UpdateError::Signature(e.to_string()),
        E::Reqwest(_) | E::Network(_) | E::ReleaseNotFound | E::InsecureTransportProtocol => {
            UpdateError::Network(e.to_string())
        }
        _ => UpdateError::Other(e.to_string()),
    }
}

impl<R: Runtime> Updater for TauriUpdater<R> {
    fn check(&self) -> Result<Option<UpdateInfoDto>, UpdateError> {
        use tauri_plugin_updater::UpdaterExt;
        let state = self.handle.state::<RunStateHandle>().inner().clone();
        let updater = self
            .handle
            .updater_builder()
            .on_before_exit({
                let handle = self.handle.clone();
                move || {
                    // The installer closes the app: end the run cleanly so no recovery
                    // offer follows the update.
                    state.finish();
                    log::logger().flush();
                    handle.cleanup_before_exit();
                }
            })
            .build()
            .map_err(|e| map_error(&e))?;
        let found = tauri::async_runtime::block_on(updater.check()).map_err(|e| map_error(&e))?;
        let info = found.as_ref().map(|u| UpdateInfoDto {
            version: u.version.clone(),
            notes: u.body.clone(),
            date: u.date.map(|d| d.to_string()),
        });
        if let Ok(mut f) = self.found.lock() {
            *f = found;
        }
        if let Ok(mut b) = self.bytes.lock() {
            *b = None;
        }
        Ok(info)
    }

    fn download(
        &self,
        progress: &dyn Fn(u64, Option<u64>),
        cancelled: &dyn Fn() -> bool,
    ) -> Result<(), UpdateError> {
        let update = self
            .found
            .lock()
            .ok()
            .and_then(|f| f.clone())
            .ok_or(UpdateError::NotAvailable)?;
        let received = Arc::new(Mutex::new((0u64, None::<u64>)));
        let sink = received.clone();
        // `download` verifies the signature before it returns the bytes.
        let task = tauri::async_runtime::spawn(async move {
            update
                .download(
                    move |chunk, total| {
                        if let Ok(mut r) = sink.lock() {
                            r.0 += chunk as u64;
                            r.1 = total;
                        }
                    },
                    || {},
                )
                .await
        });
        loop {
            if cancelled() {
                task.abort();
                return Err(UpdateError::Cancelled);
            }
            if let Ok(r) = received.lock() {
                progress(r.0, r.1);
            }
            if task.inner().is_finished() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
        let bytes = tauri::async_runtime::block_on(task)
            .map_err(|e| UpdateError::Other(e.to_string()))?
            .map_err(|e| map_error(&e))?;
        if let Ok(mut b) = self.bytes.lock() {
            *b = Some(bytes);
        }
        Ok(())
    }

    fn apply(&self) -> Result<(), UpdateError> {
        let update = self
            .found
            .lock()
            .ok()
            .and_then(|f| f.clone())
            .ok_or(UpdateError::NotAvailable)?;
        let bytes = self
            .bytes
            .lock()
            .ok()
            .and_then(|mut b| b.take())
            .ok_or(UpdateError::NotReady)?;
        // Off the command's thread: on Windows `install` starts the installer and exits.
        std::thread::spawn(move || {
            if let Err(e) = update.install(bytes) {
                log::error!(target: "fc_desktop", "update install failed: {e}");
            }
        });
        Ok(())
    }
}

/// [`RunState`] shared with the updater's exit hook (Tauri state).
pub type RunStateHandle = Arc<RunState>;

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        std::iter::once("Flashcard.exe")
            .chain(list.iter().copied())
            .map(String::from)
            .collect()
    }

    #[test]
    fn the_smoke_flag_is_recognised_only_when_given() {
        assert_eq!(smoke_args(args(&[])), None);
        assert_eq!(smoke_args(args(&["flashcard://study"])), None);
        assert_eq!(
            smoke_args(args(&["--smoke"])),
            Some(SmokeArgs { report: None })
        );
        assert_eq!(
            smoke_args(args(&["--smoke-report=C:\\t\\s.txt", "--smoke"])),
            Some(SmokeArgs {
                report: Some(PathBuf::from("C:\\t\\s.txt"))
            })
        );
        // The program name itself never counts.
        assert_eq!(smoke_args(vec!["--smoke".to_owned()]), None);
    }

    #[test]
    fn the_smoke_run_passes_and_writes_its_report() {
        let dir = tempfile::tempdir().unwrap();
        let report = dir.path().join("smoke.txt");
        let code = run_smoke(&SmokeArgs {
            report: Some(report.clone()),
        });
        let text = std::fs::read_to_string(&report).unwrap();
        assert_eq!(code, 0, "{text}");
        assert!(text.starts_with("SMOKE OK"), "{text}");
        assert!(!text.contains(smoke::PLANTED_SECRET));
    }

    #[test]
    fn a_run_without_finish_is_unclean_at_the_next_begin_and_finish_clears_it() {
        let dir = tempfile::tempdir().unwrap();
        let first = begin(dir.path()).unwrap();
        assert!(!first.start.previous_unclean());
        drop(first); // "crashed"
        let second = begin(dir.path()).unwrap();
        assert!(second.start.previous_unclean());
        second.state.finish();
        second.state.finish(); // idempotent
        let third = begin(dir.path()).unwrap();
        assert!(!third.start.previous_unclean());
    }

    #[test]
    fn log_settings_and_keys_reapply_the_log_configuration() {
        assert!(affects_log("maintenance.log_level"));
        assert!(affects_log("ai.default_provider"));
        assert!(!affects_log("study.new_per_day"));
    }

    #[test]
    fn signature_errors_are_refusals_not_retries() {
        let e = tauri_plugin_updater::Error::SignatureUtf8("x".into());
        assert!(matches!(map_error(&e), UpdateError::Signature(_)));
        let e = tauri_plugin_updater::Error::Network("down".into());
        assert!(matches!(map_error(&e), UpdateError::Network(_)));
    }
}
