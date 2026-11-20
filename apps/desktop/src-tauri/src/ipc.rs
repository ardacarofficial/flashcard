//! The IPC bridge: one generic command plus one event channel (docs/03 §7).
//!
//! A thin transport adapter with no business logic: it hands `(command, payload)` to
//! [`App::invoke`] as the user and returns whatever comes back. Internal methods (inverse
//! calls used by undo/redo) are refused by the app's registry for every transport actor, so
//! only public `fc-api` methods are reachable from the webview.

use fc_api::v1::events::EventEnvelope;
use fc_api::v1::oplog::Actor;
use fc_api::{ApiError, ErrorCode};
use fc_app::App;
use serde_json::Value;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Runtime, State};

/// Tauri event name carrying domain events ([`EventEnvelope`]) to the frontend.
pub const EVENT_CHANNEL: &str = "fc://event";

/// The opened collection, held in Tauri's managed state.
pub type SharedApp = Arc<App>;

/// Dispatches a qualified method (`v1.decks.create`) with its params. A missing payload
/// (`null`) means "no params" (`{}`). While the app is locked only the lock screen's methods
/// get through (`PERMISSION_APP_LOCKED`, v1.0).
pub fn dispatch(app: &App, command: &str, payload: Value) -> Result<Value, ApiError> {
    fc_app::modules::profiles::gate(app, command)?;
    let params = match payload {
        Value::Null => Value::Object(serde_json::Map::new()),
        other => other,
    };
    app.invoke(command, params, &Actor::User)
}

/// The single generic IPC entry point. Runs the (blocking, SQLite) call off the async
/// runtime's worker threads.
#[tauri::command]
pub async fn invoke(
    state: State<'_, SharedApp>,
    command: String,
    payload: Value,
) -> Result<Value, ApiError> {
    let app = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || dispatch(&app, &command, payload))
        .await
        .map_err(|e| ApiError::new(ErrorCode::Internal, e.to_string()))?
}

/// Forwards every event the app publishes (after commit) to all windows on
/// [`EVENT_CHANNEL`], unchanged.
pub fn forward_events<R: Runtime>(app: &App, handle: AppHandle<R>) {
    app.events().subscribe(Arc::new(move |e: &EventEnvelope| {
        // A closed window is not an error for the command that produced the event.
        let _ = handle.emit(EVENT_CHANNEL, e);
    }));
}

/// How often the app looks for commits other processes made to the collection.
pub const EXTERNAL_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(2000);

/// Background watcher: every [`EXTERNAL_POLL_INTERVAL`] asks the app whether another process
/// (CLI, `fc mcp serve`) committed something; if so the app publishes `data_changed` and the
/// forwarded event makes open screens refetch. One cheap query per tick.
pub fn watch_external_changes(app: SharedApp) {
    std::thread::spawn(move || loop {
        std::thread::sleep(EXTERNAL_POLL_INTERVAL);
        let _ = app.poll_external_changes();
    });
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use fc_app::testing::TestApp;
    use serde_json::json;

    #[test]
    fn dispatches_public_methods_to_the_app() {
        let t = TestApp::mem();
        let deck = dispatch(
            &t.app,
            "v1.decks.create",
            json!({ "path": "Dil::İngilizce" }),
        )
        .unwrap();
        assert_eq!(deck["path"], "Dil::İngilizce");
        let tree = dispatch(&t.app, "v1.decks.tree", Value::Null).unwrap();
        assert_eq!(tree["roots"][0]["deck"]["name"], "Dil");
        // Events reach subscribers after commit.
        assert!(!t.take_events().is_empty());
    }

    #[test]
    fn internal_and_unknown_methods_are_refused() {
        let t = TestApp::mem();
        let internal = dispatch(
            &t.app,
            "v1.decks.restore_affected",
            json!({ "id": "01900000-0000-7000-8000-0000000000aa", "decks": [], "notes": [], "cards": [] }),
        )
        .unwrap_err();
        assert_eq!(internal.code, ErrorCode::PermissionDenied);
        let unknown = dispatch(&t.app, "v1.nope", Value::Null).unwrap_err();
        assert_eq!(unknown.code, ErrorCode::NotFoundMethod);
    }

    #[test]
    fn errors_serialise_to_the_contract_shape() {
        let t = TestApp::mem();
        let e = dispatch(&t.app, "v1.decks.create", json!({ "path": "" })).unwrap_err();
        let v = serde_json::to_value(e).unwrap();
        for k in ["code", "message_key", "message", "retryable"] {
            assert!(v.get(k).is_some(), "missing {k}: {v}");
        }
    }
}
