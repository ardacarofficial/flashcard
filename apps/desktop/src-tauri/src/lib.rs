//! Flashcard desktop shell. Window + IPC bridge only; all logic lives in the
//! Rust crates behind `fc-api` (docs/03 §1).

pub mod ipc;

use tauri::Manager;

/// Start the desktop app.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let result = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![ipc::invoke])
        .setup(|app| {
            // TODO(integration): open the collection via fc-app, store it with
            // `app.manage(...)`, and forward its event bus to `ipc::emit_event`.
            let handle = app.app_handle().clone();
            ipc::emit_event(
                &handle,
                &ipc::FcEvent {
                    kind: "app.ready".into(),
                    payload: serde_json::Value::Null,
                },
            )?;
            Ok(())
        })
        .run(tauri::generate_context!());
    if let Err(e) = result {
        eprintln!("fc-desktop failed to start: {e}");
        std::process::exit(1);
    }
}
