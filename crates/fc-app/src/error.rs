//! App-layer error: wraps core, storage and API errors so handlers can use `?` on all of them,
//! and maps everything to the wire [`ApiError`] in exactly one place.

use fc_api::{ApiError, ErrorCode};
use fc_core::error::CoreError;
use fc_storage::StorageError;

/// Result alias for handlers and services.
pub type AppResult<T> = Result<T, AppError>;

/// Any error a handler can produce.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    /// Already a wire error (not-found checks, conflicts decided by the handler…).
    #[error(transparent)]
    Api(#[from] ApiError),
    /// A pure domain rule failed.
    #[error(transparent)]
    Core(#[from] CoreError),
    /// Storage failed.
    #[error(transparent)]
    Storage(#[from] StorageError),
}

impl AppError {
    /// Shortcut for `AppError::Api(ApiError::new(code, message))`.
    pub fn api(code: ErrorCode, message: impl Into<String>) -> Self {
        Self::Api(ApiError::new(code, message))
    }

    /// Maps to the wire error.
    pub fn into_api(self) -> ApiError {
        match self {
            Self::Api(e) => e,
            Self::Core(e) => core_to_api(e),
            Self::Storage(e) => storage_to_api(e),
        }
    }
}

impl From<AppError> for ApiError {
    fn from(e: AppError) -> Self {
        e.into_api()
    }
}

/// Maps a domain error to its documented code and details.
pub fn core_to_api(e: CoreError) -> ApiError {
    let msg = e.to_string();
    match e {
        CoreError::InvalidId { kind, value } => {
            ApiError::new(ErrorCode::ValidationInvalidParams, msg)
                .with_detail("id_kind", kind)
                .with_detail("value", value)
        }
        CoreError::InvalidDeckName { name, reason } => {
            ApiError::new(ErrorCode::ValidationDeckName, msg)
                .with_detail("name", name)
                .with_detail("reason", reason.to_string())
        }
        CoreError::InvalidTagName { name, reason } => {
            ApiError::new(ErrorCode::ValidationTagName, msg)
                .with_detail("name", name)
                .with_detail("reason", reason.to_string())
        }
        CoreError::UnknownField { field } => {
            ApiError::new(ErrorCode::ValidationUnknownField, msg).with_detail("field", field)
        }
        CoreError::RequiredFieldEmpty { field } => {
            ApiError::new(ErrorCode::ValidationFieldRequired, msg).with_detail("field", field)
        }
        CoreError::InvalidNoteType { reason } => {
            ApiError::new(ErrorCode::ValidationNoteType, msg).with_detail("reason", reason)
        }
        CoreError::InvalidAttributeDef { reason } => {
            ApiError::new(ErrorCode::ValidationAttributeDef, msg)
                .with_detail("reason", reason.to_string())
        }
        CoreError::InvalidAttributeValue { key, reason } => {
            ApiError::new(ErrorCode::ValidationAttributeValue, msg)
                .with_detail("key", key)
                .with_detail("reason", reason.to_string())
        }
        CoreError::AttributeNotApplicable { id } => {
            ApiError::new(ErrorCode::ValidationAttributeNotApplicable, msg).with_detail("id", id)
        }
        CoreError::AttributeRequired { key } => {
            ApiError::new(ErrorCode::ValidationAttributeRequired, msg).with_detail("key", key)
        }
        CoreError::UnknownAttributeOption { key, option } => {
            ApiError::new(ErrorCode::NotFoundAttributeOption, msg)
                .with_detail("key", key)
                .with_detail("id", option)
        }
        CoreError::InvalidSettingValue { key, reason } => {
            ApiError::new(ErrorCode::ValidationSettingValue, msg)
                .with_detail("key", key)
                .with_detail("reason", reason)
        }
        CoreError::DeckCycle => ApiError::new(ErrorCode::ConflictDeckCycle, msg),
        _ => ApiError::new(ErrorCode::Internal, msg),
    }
}

/// A paging cursor the store did not issue: `VALIDATION_INVALID_PARAMS` with
/// `details.field = "cursor"`.
fn invalid_cursor(msg: String) -> ApiError {
    ApiError::new(ErrorCode::ValidationInvalidParams, msg.clone())
        .with_detail("field", "cursor")
        .with_detail("reason", msg)
}

/// Maps a storage error. Handlers should check existence/uniqueness themselves and return the
/// specific `NOT_FOUND_*`/`CONFLICT_*` code; this is the fallback.
pub fn storage_to_api(e: StorageError) -> ApiError {
    let msg = e.to_string();
    match e {
        StorageError::NotFound { entity, id } => {
            let code = match entity {
                "deck" => ErrorCode::NotFoundDeck,
                "note" => ErrorCode::NotFoundNote,
                "card" => ErrorCode::NotFoundCard,
                "notetype" => ErrorCode::NotFoundNoteType,
                "tag" => ErrorCode::NotFoundTag,
                "attribute" => ErrorCode::NotFoundAttribute,
                "attribute_option" => ErrorCode::NotFoundAttributeOption,
                "job" => ErrorCode::NotFoundJob,
                _ => ErrorCode::Internal,
            };
            ApiError::new(code, msg).with_detail("id", id)
        }
        // Client input, not a database fault.
        StorageError::InvalidCursor(_) => invalid_cursor(msg),
        StorageError::Busy => ApiError::new(ErrorCode::IoDatabase, msg).retryable(true),
        // A plugin host call ran past the plugin's deadline (`fc_storage::interrupt`).
        StorageError::Interrupted => {
            ApiError::new(ErrorCode::IoDatabase, msg).with_detail("interrupted", true)
        }
        StorageError::File { path, .. } => {
            ApiError::new(ErrorCode::IoFile, msg).with_detail("path", path)
        }
        // v1.4.1: said in plain words; the command was rolled back, the app stays usable.
        StorageError::DiskFull { path } => {
            let e = ApiError::new(ErrorCode::IoDiskFull, msg);
            match path {
                Some(p) => e.with_detail("path", p),
                None => e,
            }
        }
        StorageError::NotImplemented(op) => {
            ApiError::new(ErrorCode::InternalNotImplemented, msg).with_detail("operation", op)
        }
        StorageError::Constraint { constraint } => {
            ApiError::new(ErrorCode::Internal, msg).with_detail("constraint", constraint)
        }
        _ => ApiError::new(ErrorCode::IoDatabase, msg).retryable(false),
    }
}
