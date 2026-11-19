//! Transport envelopes. Every transport (Tauri IPC, local HTTP, MCP, plugin ABI, CLI) carries
//! the same shapes: one generic `invoke(method, params)` plus an event stream.

use crate::error::ApiError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

/// A call to a method. Also the serialized form of an undo inverse in the oplog.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct InvokeRequest {
    /// Qualified method name, e.g. `v1.decks.create`.
    pub method: String,
    /// Request params (shape = the method's request type).
    #[ts(type = "unknown")]
    pub params: Value,
}

/// Result of an invoke.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum InvokeResponse {
    /// Success; `value` has the method's response shape.
    Ok {
        /// Response value.
        #[ts(type = "unknown")]
        value: Value,
    },
    /// Failure.
    Error {
        /// Error.
        error: ApiError,
    },
}

impl From<Result<Value, ApiError>> for InvokeResponse {
    fn from(r: Result<Value, ApiError>) -> Self {
        match r {
            Ok(value) => Self::Ok { value },
            Err(error) => Self::Error { error },
        }
    }
}

impl From<InvokeResponse> for Result<Value, ApiError> {
    fn from(r: InvokeResponse) -> Self {
        match r {
            InvokeResponse::Ok { value } => Ok(value),
            InvokeResponse::Error { error } => Err(error),
        }
    }
}
