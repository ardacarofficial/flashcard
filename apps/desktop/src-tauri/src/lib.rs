//! Flashcard desktop shell. Window + IPC bridge only; all logic lives in the Rust crates
//! behind `fc-api` (docs/03 §1).

pub mod ipc;

use std::sync::Arc;
use tauri::Manager;

/// File name of the collection inside the app data directory.
pub const COLLECTION_FILE: &str = "collection.db";

/// Starts the desktop app: opens (creating/migrating) the collection in the OS app data
/// directory, puts it in Tauri state and forwards its events to the webview.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let result = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![ipc::invoke])
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&dir)?;
            let collection = fc_app::App::open_sqlite(&dir.join(COLLECTION_FILE))
                .map_err(|e| e.into_api().message)?;
            let collection: ipc::SharedApp = Arc::new(collection);
            ipc::forward_events(&collection, app.handle().clone());
            app.manage(collection);
            Ok(())
        })
        .run(tauri::generate_context!());
    if let Err(e) = result {
        eprintln!("fc-desktop failed to start: {e}");
        std::process::exit(1);
    }
}
