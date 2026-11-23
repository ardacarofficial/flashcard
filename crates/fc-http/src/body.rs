//! Reading a request body, the same way on every path (v1.4.1): frame by frame so that only a
//! body past the limit is "too large"; a body that ends before its `Content-Length` or breaks
//! its chunked encoding is a malformed request; and a client that announces a body and then
//! stalls is cut off after [`crate::BODY_READ_TIMEOUT`] instead of holding the connection.

use axum::body::{Body, HttpBody};
use axum::http::{header, HeaderMap, StatusCode};
use fc_api::{ApiError, ErrorCode};
use serde_json::Value;
use std::time::Duration;

/// `details.reason` of the error a stalled body gets (answered with `408`).
pub(crate) const TIMEOUT_REASON: &str = "body_timeout";

fn too_large(max: usize) -> ApiError {
    ApiError::new(
        ErrorCode::ValidationPayloadTooLarge,
        "request body too large",
    )
    .with_detail("max_bytes", max)
}

/// The body, at most `max` bytes, read within `timeout`.
///
/// * declared or sent past `max` → `VALIDATION_PAYLOAD_TOO_LARGE` (`413`);
/// * ended early / broken encoding → `malformed` (each path picks its code);
/// * not complete within `timeout` → `VALIDATION_INVALID_PARAMS` with
///   `details.reason = "body_timeout"` (each path answers it with `408`, see [`status`]).
pub(crate) async fn read(
    mut body: Body,
    headers: &HeaderMap,
    max: usize,
    timeout: Duration,
    malformed: ErrorCode,
) -> Result<Vec<u8>, ApiError> {
    let declared = headers
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.trim().parse::<usize>().ok());
    if declared.is_some_and(|n| n > max) {
        return Err(too_large(max));
    }
    let read_all = async {
        let mut out = Vec::new();
        while let Some(frame) =
            std::future::poll_fn(|cx| std::pin::Pin::new(&mut body).poll_frame(cx)).await
        {
            let frame = frame.map_err(|_| {
                ApiError::new(
                    malformed,
                    "the request body ended early or is not validly encoded",
                )
                .with_detail("field", "body")
            })?;
            if let Ok(data) = frame.into_data() {
                if out.len() + data.len() > max {
                    return Err(too_large(max));
                }
                out.extend_from_slice(&data);
            }
        }
        Ok(out)
    };
    match tokio::time::timeout(timeout, read_all).await {
        Ok(r) => r,
        Err(_) => Err(ApiError::new(
            ErrorCode::ValidationInvalidParams,
            "the request body did not arrive in time",
        )
        .with_detail("field", "body")
        .with_detail("reason", TIMEOUT_REASON)
        .with_detail("timeout_secs", timeout.as_secs())),
    }
}

/// `408` for a body that did not arrive in time, else `None` (the path's own mapping).
pub(crate) fn status(e: &ApiError) -> Option<StatusCode> {
    (e.details.get("reason").and_then(Value::as_str) == Some(TIMEOUT_REASON))
        .then_some(StatusCode::REQUEST_TIMEOUT)
}
