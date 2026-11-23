//! `/subtitle/v1/*`: the Subtitle browser extension's contract ("Subtitle Kartları API v1",
//! docs/06 §5.1). The user enters `http://127.0.0.1:<port>/subtitle` as the server address in
//! the extension; every path below is relative to it. Every decision is the `subtitle`
//! module's ([`fc_app::modules::subtitle`]); this file speaks the contract's HTTP:
//!
//! * the module off → `503` + `Retry-After` for every route, before anything else is looked
//!   at (no key check, no data: only "unavailable"). Not `404`: the extension takes a `404` on
//!   `/v1/media` for "this server has no media" and stops sending pictures and sounds for
//!   good, and any other `4xx` as a permanent failure; a `503` makes its outbox wait;
//! * `Authorization: Bearer <access key>` on every request (`401` without a working one), then
//!   a per-key rate limit ([`KEY_BURST`] at once, [`KEY_RATE_PER_SEC`] sustained; `429` +
//!   `Retry-After`) sized so that flushing a backlog never hits it;
//! * errors as `{ "error": { "code", "message" } }`, statuses as the contract lists them
//!   (`422` invalid, `413` too large, `415` wrong media type, `503` + `Retry-After` while the
//!   app is locked, so the extension retries instead of giving up).
//!
//! The checks `routes::handle` makes for every path still come first: `Host` must name the
//! loopback address, an `Origin` must be a browser extension's (web pages get `403`). Unlike
//! `/v1/*` no `X-Flashcard-Client` header is required: the extension cannot send one, and a
//! web page can neither read nor send the Bearer key (a cross-origin `Authorization` header
//! needs a preflight, which only extension origins pass), so every request that reaches the
//! collection carries a key only the extension has.

use crate::routes::{bearer, blocking, json, lock, token_hash, Bucket, Shared};
use crate::MAX_BODY_BYTES;
use axum::body::Body;
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode};
use axum::response::Response;
use fc_api::v1::subtitle::{SubtitleCaptureResult, SubtitleCaptureStatus, SubtitleError};
use fc_api::{ApiError, ErrorCategory, ErrorCode};
use fc_app::modules::subtitle::{self, MAX_AUDIO_BYTES};
use serde_json::Value;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Headers the extension may send cross-origin (CORS preflight answer).
pub(crate) const ALLOW_HEADERS: &str =
    "authorization, content-type, accept, idempotency-key, x-capture-id, x-media-kind";

/// Seconds the extension is told to wait while the app is locked.
const LOCKED_RETRY_SECS: u64 = 30;
/// Seconds the extension is told to wait while the `subtitle` module is off.
pub(crate) const MODULE_OFF_RETRY_SECS: u64 = 300;

/// Requests one access key may make at once. The extension's outbox flushes a backlog in
/// batches of 20 captures, each after up to two media uploads (~41 requests a round, one
/// round after another until the queue is empty): a few hundred queued words fit without a
/// `429`. A runaway client is still bounded by [`KEY_RATE_PER_SEC`].
pub const KEY_BURST: f64 = 1_000.0;
/// Sustained requests per second per access key.
pub const KEY_RATE_PER_SEC: f64 = 100.0;

fn api(code: ErrorCode, message: &str) -> ApiError {
    ApiError::new(code, message)
}

/// Status of an error on `/subtitle/v1/*`.
fn status_of(e: &ApiError) -> StatusCode {
    match e.code {
        ErrorCode::NotFoundMethod => StatusCode::NOT_FOUND,
        ErrorCode::ConflictModuleDisabled => StatusCode::SERVICE_UNAVAILABLE,
        ErrorCode::PermissionUnauthenticated => StatusCode::UNAUTHORIZED,
        ErrorCode::ValidationPayloadTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
        ErrorCode::ConflictRateLimited => StatusCode::TOO_MANY_REQUESTS,
        ErrorCode::PermissionAppLocked => StatusCode::SERVICE_UNAVAILABLE,
        ErrorCode::ValidationSubtitleMedia
            if e.details.get("field").and_then(Value::as_str) == Some("content-type") =>
        {
            StatusCode::UNSUPPORTED_MEDIA_TYPE
        }
        code => match code.category() {
            ErrorCategory::Validation => StatusCode::UNPROCESSABLE_ENTITY,
            ErrorCategory::Permission => StatusCode::FORBIDDEN,
            ErrorCategory::NotFound => StatusCode::NOT_FOUND,
            // Never 409: the extension takes a 409 for "the server has it".
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        },
    }
}

/// An error in the contract's shape.
pub(crate) fn error(e: &ApiError) -> Response {
    let status = status_of(e);
    let mut body = subtitle::error_body(e);
    body.retryable = None;
    let mut r = json(status, &SubtitleError { error: body });
    let retry = match e.code {
        ErrorCode::PermissionAppLocked => Some(LOCKED_RETRY_SECS),
        ErrorCode::ConflictModuleDisabled => Some(MODULE_OFF_RETRY_SECS),
        _ => e.details.get("retry_after_secs").and_then(Value::as_u64),
    };
    if let Some(secs) = retry {
        if let Ok(v) = HeaderValue::from_str(&secs.to_string()) {
            r.headers_mut().insert(header::RETRY_AFTER, v);
        }
    }
    r.headers_mut().insert(
        header::ACCESS_CONTROL_EXPOSE_HEADERS,
        HeaderValue::from_static("retry-after"),
    );
    r
}

/// The module is off: the same answer for everyone, key or not.
fn module_off() -> ApiError {
    api(
        ErrorCode::ConflictModuleDisabled,
        "the Subtitle connection is turned off in the app",
    )
}

fn not_found(method: &Method, path: &str) -> ApiError {
    api(
        ErrorCode::NotFoundMethod,
        &format!("no endpoint {method} {path}"),
    )
}

fn header_text<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|v| !v.is_empty())
}

async fn read_body(mut body: Body, headers: &HeaderMap, max: usize) -> Result<Vec<u8>, ApiError> {
    use axum::body::HttpBody;
    let too_large = || {
        api(
            ErrorCode::ValidationPayloadTooLarge,
            "request body too large",
        )
        .with_detail("max_bytes", max)
    };
    let declared =
        header_text(headers, header::CONTENT_LENGTH.as_str()).and_then(|v| v.parse::<usize>().ok());
    if declared.is_some_and(|n| n > max) {
        return Err(too_large());
    }
    // v1.4.1 (fuzz): read frame by frame so that only a body past `max` is a `413`; a body
    // that ends before its `Content-Length` or breaks its chunked encoding is a malformed
    // request (`422`), not "too large" (which the extension takes as final for the capture).
    let mut out = Vec::new();
    while let Some(frame) =
        std::future::poll_fn(|cx| std::pin::Pin::new(&mut body).poll_frame(cx)).await
    {
        let frame = frame.map_err(|_| {
            api(
                ErrorCode::ValidationInvalidParams,
                "the request body ended early or is not validly encoded",
            )
            .with_detail("field", "body")
        })?;
        if let Ok(data) = frame.into_data() {
            if out.len() + data.len() > max {
                return Err(too_large());
            }
            out.extend_from_slice(&data);
        }
    }
    Ok(out)
}

/// Largest `captures:batch` body: every capture of a full batch may be as large as a capture
/// sent on its own (a `413` would fail every capture of the batch for good).
pub(crate) const MAX_BATCH_BODY_BYTES: usize = subtitle::MAX_BATCH * MAX_BODY_BYTES;

async fn read_json(body: Body, headers: &HeaderMap, max: usize) -> Result<Value, ApiError> {
    let is_json = header_text(headers, header::CONTENT_TYPE.as_str())
        .and_then(|v| v.split(';').next())
        .is_some_and(|m| m.trim().eq_ignore_ascii_case("application/json"));
    if !is_json {
        return Err(api(
            ErrorCode::ValidationSubtitleMedia,
            "Content-Type must be application/json",
        )
        .with_detail("field", "content-type"));
    }
    let bytes = read_body(body, headers, max).await?;
    serde_json::from_slice(&bytes).map_err(|e| {
        api(
            ErrorCode::ValidationSubtitleCapture,
            &format!("body is not JSON: {e}"),
        )
        .with_detail("field", "body")
    })
}

/// `%XX` and `+` decoding of one query component.
fn decode(s: &str) -> String {
    let hex = |c: u8| {
        char::from(c)
            .to_digit(16)
            .and_then(|d| u8::try_from(d).ok())
    };
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() => match (hex(b[i + 1]), hex(b[i + 2])) {
                (Some(h), Some(l)) => {
                    out.push(h * 16 + l);
                    i += 2;
                }
                _ => out.push(b'%'),
            },
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn query_param(query: Option<&str>, name: &str) -> Option<String> {
    query?.split('&').find_map(|pair| {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        (decode(k) == name).then(|| decode(v))
    })
}

/// A key's requests: its rate limit, and its last use noted at most once a minute.
async fn authenticate(
    s: &Arc<Shared>,
    headers: &HeaderMap,
) -> Result<fc_api::v1::subtitle::SubtitleKeyDto, ApiError> {
    let hash = token_hash(&bearer(headers)?);
    let app = s.app.clone();
    let key = blocking(move || subtitle::authenticate(&app, &hash)).await?;
    lock(&s.key_buckets)
        .entry(key.id)
        .or_insert_with(|| Bucket::full(KEY_BURST))
        .take(KEY_RATE_PER_SEC, KEY_BURST)
        .map_err(crate::routes::rate_limited)?;
    let due = {
        let mut t = lock(&s.key_touched);
        let now = Instant::now();
        let due = t
            .get(&key.id)
            .is_none_or(|last| now.duration_since(*last) >= Duration::from_secs(60));
        if due {
            t.insert(key.id, now);
        }
        due
    };
    if due {
        let app = s.app.clone();
        let k = key.clone();
        // Bookkeeping only: a failure here never fails the request.
        let _ = blocking(move || subtitle::touch_key(&app, &k)).await;
    }
    Ok(key)
}

/// Routes `/subtitle/v1/<rest>`.
pub(crate) async fn route(
    s: &Arc<Shared>,
    method: &Method,
    path: &str,
    query: Option<&str>,
    headers: &HeaderMap,
    body: Body,
) -> Result<Response, ApiError> {
    // Off: unavailable for now (the extension waits and retries); nothing else is told.
    subtitle::ensure_enabled(&s.app).map_err(|_| module_off())?;
    let rest = path
        .strip_prefix(subtitle::PATH_PREFIX)
        .and_then(|p| p.strip_prefix("/v1/"))
        .ok_or_else(|| not_found(method, path))?;
    let allowed = match rest {
        "me" | "decks" | "known-words" => Method::GET,
        "media" | "captures" | "captures:batch" => Method::POST,
        _ => return Err(not_found(method, path)),
    };
    if *method != allowed {
        return Ok(error_with(
            StatusCode::METHOD_NOT_ALLOWED,
            &api(ErrorCode::NotFoundMethod, &format!("use {allowed} {path}")),
        ));
    }
    if s.app.background_blocked() {
        return Err(fc_app::modules::profiles::locked_error(path));
    }
    let key = authenticate(s, headers).await?;
    let app = s.app.clone();
    match rest {
        "me" => {
            let me = blocking(move || subtitle::me(&app, &key)).await?;
            Ok(json(StatusCode::OK, &me))
        }
        "decks" => {
            let decks = blocking(move || subtitle::decks(&app)).await?;
            Ok(json(StatusCode::OK, &decks))
        }
        // The gate (`subtitle.share_known_words`, off by default) is the service's: off →
        // `{"words": [], "next_cursor": null, "shared": false}`, still a 200 (a 4xx/5xx would
        // read as an error or a reason to retry).
        "known-words" => {
            let lang = query_param(query, "lang");
            let cursor = query_param(query, "cursor");
            let words =
                blocking(move || subtitle::known_words(&app, lang.as_deref(), cursor.as_deref()))
                    .await?;
            Ok(json(StatusCode::OK, &words))
        }
        "captures" => {
            let value = read_json(body, headers, MAX_BODY_BYTES).await?;
            let o = blocking(move || subtitle::capture(&app, &key, &value)).await?;
            let status = match o.status {
                SubtitleCaptureStatus::Created => StatusCode::CREATED,
                SubtitleCaptureStatus::Duplicate => StatusCode::OK,
            };
            Ok(json(
                status,
                &SubtitleCaptureResult {
                    status: o.status,
                    card_id: o.card_id.to_string(),
                },
            ))
        }
        "captures:batch" => {
            let value = read_json(body, headers, MAX_BATCH_BODY_BYTES).await?;
            let r = blocking(move || subtitle::batch(&app, &key, &value)).await?;
            Ok(json(StatusCode::OK, &r))
        }
        // "media"
        _ => {
            let missing = |h: &str| {
                api(
                    ErrorCode::ValidationSubtitleMedia,
                    &format!("the {h} header is required"),
                )
                .with_detail("field", h.to_ascii_lowercase())
            };
            let mime = header_text(headers, header::CONTENT_TYPE.as_str())
                .ok_or_else(|| missing("Content-Type"))?
                .to_owned();
            let capture_id = header_text(headers, "x-capture-id")
                .ok_or_else(|| missing("X-Capture-Id"))?
                .to_owned();
            let kind = header_text(headers, "x-media-kind")
                .ok_or_else(|| missing("X-Media-Kind"))?
                .to_owned();
            let bytes = read_body(body, headers, MAX_AUDIO_BYTES).await?;
            let r =
                blocking(move || subtitle::upload(&app, &key, &capture_id, &kind, &mime, &bytes))
                    .await?;
            Ok(json(StatusCode::CREATED, &r))
        }
    }
}

fn error_with(status: StatusCode, e: &ApiError) -> Response {
    let mut r = error(e);
    *r.status_mut() = status;
    r
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuses_follow_the_contract() {
        let s = |c| status_of(&ApiError::new(c, ""));
        assert_eq!(
            s(ErrorCode::ConflictModuleDisabled),
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert_eq!(s(ErrorCode::NotFoundMethod), StatusCode::NOT_FOUND);
        // Off is "try again later" with a wait, never a 4xx the extension gives up on.
        let off = error(&module_off());
        assert_eq!(off.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(
            off.headers().get(header::RETRY_AFTER).unwrap(),
            &MODULE_OFF_RETRY_SECS.to_string()
        );
        assert_eq!(
            s(ErrorCode::PermissionUnauthenticated),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            s(ErrorCode::PermissionOriginNotAllowed),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            s(ErrorCode::ValidationSubtitleCapture),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            s(ErrorCode::ValidationPayloadTooLarge),
            StatusCode::PAYLOAD_TOO_LARGE
        );
        assert_eq!(
            s(ErrorCode::PermissionAppLocked),
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert_eq!(
            s(ErrorCode::ConflictSubtitleNoteType),
            StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(
            status_of(
                &ApiError::new(ErrorCode::ValidationSubtitleMedia, "")
                    .with_detail("field", "content-type")
            ),
            StatusCode::UNSUPPORTED_MEDIA_TYPE
        );
    }

    #[test]
    fn a_backlog_flush_fits_the_key_rate_limit_and_a_runaway_client_does_not() {
        // A few hundred queued words: 300 captures × (2 uploads + their share of a batch).
        let backlog = 300.0 * 2.0 + 300.0 / 20.0;
        assert!(KEY_BURST >= backlog, "{KEY_BURST} < {backlog}");
        let mut b = Bucket::full(KEY_BURST);
        for i in 0..615 {
            assert!(b.take(KEY_RATE_PER_SEC, KEY_BURST).is_ok(), "request {i}");
        }
        // Still bounded: thousands at once run out (refilling 100/s cannot keep up).
        let refused = (0..10_000).any(|_| b.take(KEY_RATE_PER_SEC, KEY_BURST).is_err());
        assert!(refused);
    }

    #[test]
    fn query_params_are_decoded() {
        let q = Some("lang=en&cursor=ZW4fa2V5&x=a%20b+c%2");
        assert_eq!(query_param(q, "lang").as_deref(), Some("en"));
        assert_eq!(query_param(q, "cursor").as_deref(), Some("ZW4fa2V5"));
        assert_eq!(query_param(q, "x").as_deref(), Some("a b c%2"));
        assert_eq!(query_param(q, "y"), None);
        assert_eq!(query_param(None, "lang"), None);
    }
}
