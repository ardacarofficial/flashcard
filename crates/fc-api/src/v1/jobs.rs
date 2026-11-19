//! Background job DTOs (docs/03 §12).

use super::common::I18nMessage;
use crate::error::ApiError;
use fc_core::{JobId, Timestamp};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Job lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum JobStatus {
    /// Waiting for a worker.
    Queued,
    /// Running.
    Running,
    /// Finished successfully.
    Succeeded,
    /// Finished with an error.
    Failed,
    /// Cancelled by the user.
    Cancelled,
    /// App closed while running; user decides to resume or cancel.
    Interrupted,
}

/// A job.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct JobDto {
    /// Id.
    pub id: JobId,
    /// Job kind (`import.csv`, …).
    pub kind: String,
    /// Status.
    pub status: JobStatus,
    /// Progress 0.0–1.0 (`null` = indeterminate).
    pub progress: Option<f32>,
    /// Current step description.
    pub message: Option<I18nMessage>,
    /// Error if failed.
    pub error: Option<ApiError>,
    /// Created.
    pub created_at: Timestamp,
    /// Last update.
    pub updated_at: Timestamp,
}

/// `jobs.get`, `jobs.cancel`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct JobRef {
    /// Job id.
    pub id: JobId,
}

/// Response of `jobs.list`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct JobList {
    /// Jobs, newest first (finished jobs are pruned over time).
    pub jobs: Vec<JobDto>,
}
