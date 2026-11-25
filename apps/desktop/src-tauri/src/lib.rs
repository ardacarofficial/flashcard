//! Flashcard desktop shell. Window + IPC bridge only; all logic lives in the Rust crates
//! behind `fc-api` (docs/03 §1).

pub mod ipc;
pub mod maintenance;
pub mod media;
pub mod outside;
pub mod session;

use std::sync::Arc;
use tauri::Manager;

/// File name of the collection inside the app data directory.
pub const COLLECTION_FILE: &str = "collection.db";

/// Starts the desktop app: opens (creating/migrating) the collection in the OS app data
/// directory, puts it in Tauri state and forwards its events to the webview.
///
/// A startup failure (e.g. a collection written by a newer version, or an edited migration)
/// is shown in a native error dialog, since release builds on Windows have no console, and
/// the process exits with status 1.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Before Tauri starts any thread: no shell variable changes behavior (FSRS optimizer).
    fc_app::system::pin_process_env();
    // `Flashcard.exe --smoke`: self test in a temporary collection, then exit (CI, v1.0).
    if let Some(args) = maintenance::smoke_args(std::env::args()) {
        std::process::exit(maintenance::run_smoke(&args));
    }
    let result = tauri::Builder::default()
        // First: a second launch hands its arguments (a flashcard:// link) to this instance
        // and exits (v0.9, docs/06 §6).
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            outside::on_second_instance(app, argv);
        }))
        .plugin(tauri_plugin_deep_link::init())
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        // Signed updates (v1.0): driven by the backend's `updates.*` methods only.
        .plugin(tauri_plugin_updater::Builder::new().build())
        // v1.0: study reminders as OS notifications.
        .plugin(tauri_plugin_notification::init())
        .invoke_handler(tauri::generate_handler![
            ipc::invoke,
            outside::take_deep_links,
            outside::held_deep_links,
            outside::discard_held_deep_links,
            outside::quick_add_prefill
        ])
        .register_uri_scheme_protocol(media::SCHEME, |ctx, request| {
            // The running profile's collection (v1.0); its media folder sits next to it.
            // Nothing while the app is locked (423).
            let (status, mime, body) = match session::collection_path(ctx.app_handle()) {
                Ok(db) => media::respond(&db, request.uri().path()),
                Err(status) => (status, "text/plain", Vec::new()),
            };
            tauri::http::Response::builder()
                .status(status)
                .header("Content-Type", mime)
                // Media bytes are user content: never let the webview sniff them into HTML/JS.
                .header("X-Content-Type-Options", "nosniff")
                .body(body)
                .unwrap_or_default()
        })
        .on_window_event(|window, event| {
            // "Keep running in the tray" (v1.0).
            session::on_window_event(window, event);
            // Coming back to the window: pick up what other processes (the CLI, an MCP
            // client such as Claude) changed meanwhile, right away instead of at the next tick.
            if matches!(event, tauri::WindowEvent::Focused(true)) {
                if let Some(state) = window.try_state::<ipc::SharedApp>() {
                    let app = state.inner().clone();
                    std::thread::spawn(move || {
                        let _ = app.poll_external_changes();
                    });
                }
            }
        })
        .setup(|app| {
            // Handled here rather than returned: depending on the platform Tauri runs setup
            // inside the event loop and would panic on an error instead of returning it.
            match open_collection(app) {
                Ok((collection, boot, dir)) => {
                    // Crash recovery, logs, integrity check after an unclean shutdown (v1.0).
                    collection.start_maintenance(&dir, boot.log.clone(), boot.start.clone());
                    maintenance::follow_log_settings(&collection);
                    app.manage(Arc::new(boot.state));
                    // Installed release builds update themselves (asking the user first).
                    if !cfg!(debug_assertions) {
                        collection.set_updater(Arc::new(maintenance::TauriUpdater::new(
                            app.handle().clone(),
                        )));
                    }
                    ipc::forward_events(&collection, app.handle().clone());
                    ipc::watch_external_changes(collection.clone());
                    collection.set_file_picker(Arc::new(NativePicker));
                    // Background jobs (imports, exports, backups) run on their own thread;
                    // the worker lives as long as the app state.
                    match fc_app::jobs::spawn_worker(collection.clone()) {
                        Ok(worker) => {
                            app.manage(worker);
                        }
                        Err(e) => fail(&e.to_string()),
                    }
                    // Scheduled automation rules (v0.9); the thread ends with the app.
                    if let Err(e) = fc_app::modules::automation::spawn_scheduler(&collection) {
                        fail(&e.to_string());
                    }
                    app.manage(collection.clone());
                    // v1.4.2: the main window only now (`"create": false` in
                    // tauri.conf.json). Tauri would create it before this setup runs, and a
                    // native dialog shown from here (the rescue offer, a startup error) pumps
                    // the event loop: the page loaded and called `invoke` before the state
                    // above existed ("state not managed").
                    if let Err(e) = open_main_window(app) {
                        fail(&e);
                    }
                    // App lock idle watch, reminders, tray, profile switch (v1.0).
                    if let Err(e) = session::start(app.handle(), &collection) {
                        fail(&e.to_string());
                    }
                    // Links, quick add, clipboard watcher, local API server (v0.9).
                    outside::start(app.handle(), collection);
                    Ok(())
                }
                Err(e) => fail(&e),
            }
        })
        .build(tauri::generate_context!());
    match result {
        Ok(app) => app.run(|handle, event| {
            // A normal exit ends the run cleanly: no recovery offer at the next start.
            if let tauri::RunEvent::Exit = event {
                if let Some(run) = handle.try_state::<maintenance::RunStateHandle>() {
                    run.finish();
                }
                log::logger().flush();
            }
        }),
        Err(e) => fail(&e.to_string()),
    }
}

/// Native dialogs behind plugins' `fs:pick`: the user chooses, the backend reads or writes
/// the file, the plugin only ever sees a file name and bytes.
struct NativePicker;

impl NativePicker {
    fn dialog(extensions: &[String]) -> rfd::FileDialog {
        let dialog = rfd::FileDialog::new();
        if extensions.is_empty() {
            dialog
        } else {
            dialog.add_filter(extensions.join(", "), extensions)
        }
    }
}

impl fc_app::FilePicker for NativePicker {
    fn pick_open(&self, extensions: &[String]) -> Option<std::path::PathBuf> {
        Self::dialog(extensions).pick_file()
    }

    fn pick_save(&self, suggested_name: &str, extensions: &[String]) -> Option<std::path::PathBuf> {
        Self::dialog(extensions)
            .set_file_name(suggested_name)
            .save_file()
    }
}

/// Opens the active profile's collection (v1.0: `profiles.json` in the app data folder picks
/// it; the default profile is `<app data>/collection.db`, as before v1.0).
///
/// The run marker, the log and the crash reports are **per profile** (v1.0): they live in the
/// active profile's folder (the app data folder itself for the default profile, as before
/// profiles; `profiles/<id>/` otherwise), next to the collection they describe. A profile
/// switch restarts the process, so one process only ever writes one profile's log, and
/// deleting a profile removes its logs and crash reports with its folder.
fn open_collection(
    app: &tauri::App,
) -> Result<(ipc::SharedApp, maintenance::Boot, std::path::PathBuf), String> {
    let root = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&root).map_err(|e| format!("{}: {e}", root.display()))?;
    let dir = session::active_profile_dir(&root)?;
    // Before opening: the run marker, the log and the panic hook (v1.0).
    let boot = maintenance::begin(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    // AI provider keys and the app lock's hash live only in the OS keychain (v0.8, v1.0).
    let secrets = Arc::new(fc_ai::secrets::OsKeychain::new(fc_ai::secrets::SERVICE));
    let (collection, session) =
        session::open_profile(&root, secrets, Arc::new(fc_app::system::SystemClock))?;
    let dir = session
        .collection_path()
        .parent()
        .map(std::path::Path::to_path_buf)
        .unwrap_or(dir);
    Ok((collection, boot, dir))
}

/// Creates the main window from its `tauri.conf.json` entry (which says `"create": false`,
/// so Tauri does not create it before the app state is managed).
fn open_main_window(app: &tauri::App) -> Result<(), String> {
    let config = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == "main")
        .ok_or_else(|| "tauri.conf.json: no window labelled main".to_owned())?;
    let builder = tauri::WebviewWindowBuilder::from_config(app.handle(), config)
        .map_err(|e| format!("main window: {e}"))?;
    builder.build().map_err(|e| format!("main window: {e}"))?;
    Ok(())
}

/// Text of the startup error dialog. The UI dictionaries are not loaded yet (and the
/// collection holding the language setting may be what failed), so it is bilingual.
pub fn startup_error_text(detail: &str) -> String {
    format!("Flashcard başlatılamadı.\nFlashcard could not start.\n\n{detail}")
}

/// Reports a startup failure on stderr and in a native dialog, then exits.
fn fail(detail: &str) -> ! {
    let text = startup_error_text(detail);
    eprintln!("fc-desktop: {text}");
    rfd::MessageDialog::new()
        .set_level(rfd::MessageLevel::Error)
        .set_title("Flashcard")
        .set_description(text)
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
    std::process::exit(1)
}

#[cfg(test)]
mod tests {
    /// The main window must not exist before the app state is managed (v1.4.2): Tauri
    /// creates every configured window before `setup` unless it says `"create": false`.
    #[test]
    fn the_main_window_is_created_by_setup_not_by_the_config() {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap_or_default();
        let windows = conf["app"]["windows"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0]["label"], "main");
        assert_eq!(windows[0]["create"], false);
    }

    #[test]
    fn startup_error_text_is_bilingual_and_keeps_the_detail() {
        let t = super::startup_error_text("IO_DATABASE: disk full");
        assert!(t.contains("başlatılamadı"));
        assert!(t.contains("could not start"));
        assert!(t.ends_with("IO_DATABASE: disk full"));
    }
}
