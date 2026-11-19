//! Flashcard desktop shell. Window + IPC bridge only; all logic lives in the Rust crates
//! behind `fc-api` (docs/03 §1).

pub mod ipc;

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
    let result = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![ipc::invoke])
        .setup(|app| {
            // Handled here rather than returned: depending on the platform Tauri runs setup
            // inside the event loop and would panic on an error instead of returning it.
            match open_collection(app) {
                Ok(collection) => {
                    ipc::forward_events(&collection, app.handle().clone());
                    app.manage(collection);
                    Ok(())
                }
                Err(e) => fail(&e),
            }
        })
        .run(tauri::generate_context!());
    if let Err(e) = result {
        fail(&e.to_string());
    }
}

fn open_collection(app: &tauri::App) -> Result<ipc::SharedApp, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = dir.join(COLLECTION_FILE);
    let collection = fc_app::App::open_sqlite(&path).map_err(|e| {
        let e = e.into_api();
        format!("{}\n{}: {}", path.display(), e.code, e.message)
    })?;
    Ok(Arc::new(collection))
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
    #[test]
    fn startup_error_text_is_bilingual_and_keeps_the_detail() {
        let t = super::startup_error_text("IO_DATABASE: disk full");
        assert!(t.contains("başlatılamadı"));
        assert!(t.contains("could not start"));
        assert!(t.ends_with("IO_DATABASE: disk full"));
    }
}
