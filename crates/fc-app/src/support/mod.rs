//! Building blocks shared by the module handlers: lookups with the right error codes, the deck
//! index (paths, ancestry, subtrees), DTO conversion, attribute applicability and the undo
//! history walk. Handlers compose these; none of them writes on its own.

pub(crate) mod attrs;
pub(crate) mod decks;
pub(crate) mod dto;
pub(crate) mod history;
pub(crate) mod notes;

use crate::error::{AppError, AppResult};
use crate::registry::Registry;
use fc_api::v1::common::I18nMessage;
use fc_api::{ApiError, ApiMethod, ErrorCode};
use fc_storage::Reads;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::collections::HashMap;

/// An API error with an `id` detail.
pub(crate) fn err_id(code: ErrorCode, message: impl Into<String>, id: impl ToString) -> AppError {
    AppError::Api(ApiError::new(code, message).with_detail("id", id.to_string()))
}

/// `CONFLICT_DELETED` for an entity in the trash.
pub(crate) fn deleted(entity: &str, id: impl ToString) -> AppError {
    AppError::Api(
        ApiError::new(
            ErrorCode::ConflictDeleted,
            format!("{entity} is in the trash"),
        )
        .with_detail("entity", entity)
        .with_detail("id", id.to_string()),
    )
}

/// `VALIDATION_INVALID_PARAMS` with a reason.
pub(crate) fn invalid(reason: impl Into<String>) -> AppError {
    let reason = reason.into();
    AppError::Api(
        ApiError::new(ErrorCode::ValidationInvalidParams, reason.clone())
            .with_detail("reason", reason),
    )
}

/// Undo-UI summary of method `M`: key `oplog.summary.<method>`.
pub(crate) fn summary<M: ApiMethod>() -> I18nMessage {
    I18nMessage::key(format!("oplog.summary.{}", M::NAME))
}

/// Deserializes stored JSON (internal error on failure: the app owns these shapes).
pub(crate) fn from_json<T: DeserializeOwned>(v: &Value) -> AppResult<T> {
    serde_json::from_value(v.clone()).map_err(|e| AppError::api(ErrorCode::Internal, e.to_string()))
}

/// Effective on/off state of every module: stored switch, else the module default. Modules
/// that cannot be disabled are always on.
pub(crate) fn module_states(
    reads: &dyn Reads,
    registry: &Registry,
) -> AppResult<HashMap<&'static str, bool>> {
    let stored: HashMap<String, bool> = reads.list_module_states()?.into_iter().collect();
    Ok(registry
        .modules()
        .iter()
        .map(|m| {
            let on = !m.can_disable || stored.get(m.id).copied().unwrap_or(m.enabled_by_default);
            (m.id, on)
        })
        .collect())
}
