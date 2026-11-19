//! The IPC bridge: one generic command plus one event channel.
//!
//! This layer is a thin transport adapter (docs/03 §7). It must not contain
//! business logic: it hands `(command, payload)` to the API layer and returns
//! whatever comes back. Until `fc-app`/`fc-api` exist, every call answers
//! `NOT_IMPLEMENTED`.

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Runtime};

/// Tauri event name carrying domain events to the frontend.
pub const EVENT_CHANNEL: &str = "fc://event";

/// Error model mirrored from docs/03 §7.
///
/// TODO(integration): replace with `fc_api::ApiError` (same JSON shape).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ApiError {
    /// Stable, documented code. The frontend branches on this.
    pub code: String,
    /// i18n key for display.
    pub message_key: String,
    /// Developer-facing message (not shown verbatim to users).
    pub message: String,
    /// Optional structured details.
    pub details: Option<Value>,
    /// Whether retrying the same call may succeed.
    pub retryable: bool,
}

impl ApiError {
    /// The error every command returns until the backend is wired.
    pub fn not_implemented(command: &str) -> Self {
        Self {
            code: "NOT_IMPLEMENTED".into(),
            message_key: "errors.not_implemented".into(),
            message: format!("command `{command}` is not implemented yet"),
            details: Some(serde_json::json!({ "command": command })),
            retryable: false,
        }
    }
}

/// A domain event as sent on [`EVENT_CHANNEL`].
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct FcEvent {
    /// Event type, e.g. `deck.created`.
    #[serde(rename = "type")]
    pub kind: String,
    /// Event payload.
    pub payload: Value,
}

/// Dispatch a versioned command/query (e.g. `v1.deck.create`).
///
/// TODO(integration): `fc_api::dispatch(&state, &command, payload)`.
pub fn dispatch(command: &str, _payload: Value) -> Result<Value, ApiError> {
    Err(ApiError::not_implemented(command))
}

/// The single generic IPC entry point.
#[tauri::command]
pub async fn invoke(command: String, payload: Value) -> Result<Value, ApiError> {
    dispatch(&command, payload)
}

/// Push a domain event to all windows.
///
/// TODO(integration): subscribe to the fc-app event bus and forward each event here.
pub fn emit_event<R: Runtime>(app: &AppHandle<R>, event: &FcEvent) -> tauri::Result<()> {
    app.emit(EVENT_CHANNEL, event)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_command_is_not_implemented() {
        let err = dispatch("v1.deck.tree", Value::Null).unwrap_err();
        assert_eq!(err.code, "NOT_IMPLEMENTED");
        assert_eq!(err.message_key, "errors.not_implemented");
    }

    #[test]
    fn error_serialises_to_contract_shape() {
        let v = serde_json::to_value(ApiError::not_implemented("x")).unwrap();
        for k in ["code", "message_key", "message", "details", "retryable"] {
            assert!(v.get(k).is_some(), "missing {k}");
        }
    }

    #[test]
    fn event_uses_type_field() {
        let v = serde_json::to_value(FcEvent {
            kind: "app.ready".into(),
            payload: Value::Null,
        })
        .unwrap();
        assert_eq!(v["type"], "app.ready");
    }
}
