//! Request handling: one fallback handler routes by hand so the checks run in a fixed order
//! for every path — `Host`, `Origin` (+ CORS preflight), rate limits, body size, token — and
//! every failure is an `ApiError` JSON body with a status from its code.

use crate::{
    CLIENT_BURST, CLIENT_HEADER, CLIENT_RATE_PER_SEC, MAX_BODY_BYTES, PAIR_NAME_WINDOW_SECS,
    PAIR_PER_MINUTE, PAIR_PER_NAME,
};
use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode};
use axum::response::Response;
use axum::Router;
use base64::Engine;
use fc_api::v1::local_api::{
    ApiClientDto, ApiClientStatus, LocalApiMethodList, PairRequest, PairResponse, PairStatus,
    PairStatusResponse, PingResponse,
};
use fc_api::{ApiError, ErrorCategory, ErrorCode};
use fc_app::modules::local_api;
use fc_app::App;
use fc_core::{ApiClientId, Timestamp};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// A pairing request waiting for the user, kept in memory only: the token never touches disk.
struct Pairing {
    client_id: ApiClientId,
    /// Taken by the first approved poll.
    token: Option<String>,
    origin: Option<String>,
    expires: Instant,
}

struct Bucket {
    tokens: f64,
    last: Instant,
}

impl Bucket {
    fn full(burst: f64) -> Self {
        Self {
            tokens: burst,
            last: Instant::now(),
        }
    }

    /// Takes one token, or the seconds until one is back.
    fn take(&mut self, per_sec: f64, burst: f64) -> Result<(), u64> {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last).as_secs_f64();
        self.tokens = (self.tokens + elapsed * per_sec).min(burst);
        self.last = now;
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            Ok(())
        } else {
            // Whole seconds, rounded up; the f64 → u64 cast saturates.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let wait = ((1.0 - self.tokens) / per_sec).ceil().max(1.0) as u64;
            Err(wait)
        }
    }
}

struct Shared {
    app: Arc<App>,
    port: u16,
    pairings: Mutex<HashMap<String, Pairing>>,
    buckets: Mutex<HashMap<ApiClientId, Bucket>>,
    pair_bucket: Mutex<Bucket>,
    /// Pairing allowance per `origin|client name`.
    pair_names: Mutex<HashMap<String, Bucket>>,
    touched: Mutex<HashMap<ApiClientId, Instant>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub(crate) fn router(app: Arc<App>, port: u16) -> Router {
    let shared = Arc::new(Shared {
        app,
        port,
        pairings: Mutex::new(HashMap::new()),
        buckets: Mutex::new(HashMap::new()),
        pair_bucket: Mutex::new(Bucket::full(PAIR_PER_MINUTE)),
        pair_names: Mutex::new(HashMap::new()),
        touched: Mutex::new(HashMap::new()),
    });
    // Tokens handed out by an earlier run lived in its memory only: nothing can finish those
    // pairings, so they must not wait to be approved. (Failing to tidy up is not fatal.)
    let _ = local_api::expire_pending(&shared.app);
    Router::new().fallback(handle).with_state(shared)
}

// ------------------------------------------------------------------ responses

fn status_of(e: &ApiError) -> StatusCode {
    match e.code {
        ErrorCode::PermissionUnauthenticated => StatusCode::UNAUTHORIZED,
        ErrorCode::ValidationPayloadTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
        ErrorCode::ConflictRateLimited => StatusCode::TOO_MANY_REQUESTS,
        ErrorCode::ConflictModuleDisabled => StatusCode::SERVICE_UNAVAILABLE,
        ErrorCode::PermissionAppLocked => StatusCode::LOCKED,
        code => match code.category() {
            ErrorCategory::Validation => StatusCode::BAD_REQUEST,
            ErrorCategory::Permission => StatusCode::FORBIDDEN,
            ErrorCategory::NotFound => StatusCode::NOT_FOUND,
            ErrorCategory::Conflict => StatusCode::CONFLICT,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        },
    }
}

fn json<T: Serialize>(status: StatusCode, body: &T) -> Response {
    let bytes = serde_json::to_vec(body).unwrap_or_else(|_| b"{}".to_vec());
    let mut r = Response::new(Body::from(bytes));
    *r.status_mut() = status;
    let h = r.headers_mut();
    h.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    h.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    h.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    r
}

fn error(e: &ApiError) -> Response {
    let mut r = json(status_of(e), e);
    if let Some(secs) = e.details.get("retry_after_secs").and_then(Value::as_u64) {
        if let Ok(v) = HeaderValue::from_str(&secs.to_string()) {
            r.headers_mut().insert(header::RETRY_AFTER, v);
        }
    }
    r
}

fn api(code: ErrorCode, message: &str) -> ApiError {
    ApiError::new(code, message)
}

fn rate_limited(secs: u64) -> ApiError {
    api(ErrorCode::ConflictRateLimited, "too many requests").with_detail("retry_after_secs", secs)
}

fn internal(e: impl std::fmt::Display) -> ApiError {
    api(ErrorCode::Internal, &e.to_string())
}

/// Runs a blocking app call on the blocking pool.
async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, ApiError> + Send + 'static,
) -> Result<T, ApiError> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(internal)
        .and_then(|r| r)
}

// ------------------------------------------------------------------ checks

fn host_ok(headers: &HeaderMap, port: u16) -> bool {
    let Some(host) = headers.get(header::HOST).and_then(|h| h.to_str().ok()) else {
        return false;
    };
    let host = host.to_ascii_lowercase();
    [
        format!("127.0.0.1:{port}"),
        format!("localhost:{port}"),
        format!("[::1]:{port}"),
    ]
    .contains(&host)
}

/// The request's `Origin`: `Ok(None)` without one, `Ok(Some)` for an extension, else 403.
fn origin(headers: &HeaderMap) -> Result<Option<String>, ApiError> {
    match headers.get(header::ORIGIN) {
        None => Ok(None),
        Some(v) => {
            let o = v.to_str().unwrap_or_default();
            if local_api::is_extension_origin(o) {
                Ok(Some(o.to_owned()))
            } else {
                Err(api(
                    ErrorCode::PermissionOriginNotAllowed,
                    "web pages may not call the local API",
                ))
            }
        }
    }
}

fn cors(mut r: Response, origin: Option<&str>, preflight: bool) -> Response {
    let Some(o) = origin.and_then(|o| HeaderValue::from_str(o).ok()) else {
        return r;
    };
    let h = r.headers_mut();
    h.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, o);
    h.insert(header::VARY, HeaderValue::from_static("Origin"));
    if preflight {
        h.insert(
            header::ACCESS_CONTROL_ALLOW_METHODS,
            HeaderValue::from_static("GET, POST, OPTIONS"),
        );
        h.insert(
            header::ACCESS_CONTROL_ALLOW_HEADERS,
            HeaderValue::from_static("authorization, content-type, x-flashcard-client"),
        );
        h.insert(
            header::ACCESS_CONTROL_MAX_AGE,
            HeaderValue::from_static("600"),
        );
    }
    r
}

fn random_bytes<const N: usize>() -> Result<[u8; N], ApiError> {
    let mut b = [0u8; N];
    getrandom::fill(&mut b).map_err(internal)?;
    Ok(b)
}

/// Hex SHA-256 of a token (what the app stores).
pub(crate) fn token_hash(token: &str) -> String {
    Sha256::digest(token.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn bearer(headers: &HeaderMap) -> Result<String, ApiError> {
    let missing = || {
        api(
            ErrorCode::PermissionUnauthenticated,
            "an Authorization: Bearer token is required",
        )
        .with_detail("reason", "missing")
    };
    let v = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(missing)?;
    let (scheme, token) = v.trim().split_once(' ').ok_or_else(missing)?;
    if !scheme.eq_ignore_ascii_case("bearer") || token.trim().is_empty() {
        return Err(missing());
    }
    let token = token.trim();
    if token.len() > 200 {
        return Err(api(ErrorCode::PermissionUnauthenticated, "no usable token")
            .with_detail("reason", "invalid"));
    }
    Ok(token.to_owned())
}

/// `X-Flashcard-Client: <who>` (1–100 printable characters) and, for a `POST`,
/// `Content-Type: application/json`: what makes every browser request a preflighted one.
fn csrf_guard(headers: &HeaderMap, method: &Method) -> Result<(), ApiError> {
    let missing = |what: &str, reason: &str| {
        api(
            ErrorCode::ValidationInvalidParams,
            &format!("{what} is required"),
        )
        .with_detail("reason", reason)
    };
    let client = headers
        .get(CLIENT_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|v| (1..=100).contains(&v.len()) && v.chars().all(|c| !c.is_control()));
    if client.is_none() {
        return Err(missing("the X-Flashcard-Client header", "client_header"));
    }
    if method == Method::POST {
        let json = headers
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(';').next())
            .is_some_and(|m| m.trim().eq_ignore_ascii_case("application/json"));
        if !json {
            return Err(missing("Content-Type: application/json", "content_type"));
        }
    }
    Ok(())
}

async fn read_json(body: Body, headers: &HeaderMap) -> Result<Value, ApiError> {
    let too_large = || {
        api(
            ErrorCode::ValidationPayloadTooLarge,
            "request body too large",
        )
        .with_detail("max_bytes", MAX_BODY_BYTES)
    };
    let declared = headers
        .get(header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<usize>().ok());
    if declared.is_some_and(|n| n > MAX_BODY_BYTES) {
        return Err(too_large());
    }
    let bytes = axum::body::to_bytes(body, MAX_BODY_BYTES)
        .await
        .map_err(|_| too_large())?;
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return Ok(Value::Object(serde_json::Map::new()));
    }
    serde_json::from_slice(&bytes).map_err(|e| {
        api(
            ErrorCode::ValidationInvalidParams,
            &format!("body is not JSON: {e}"),
        )
        .with_detail("reason", e.to_string())
    })
}

fn parse<T: serde::de::DeserializeOwned>(v: Value) -> Result<T, ApiError> {
    serde_json::from_value(v).map_err(|e| {
        api(ErrorCode::ValidationInvalidParams, &e.to_string()).with_detail("reason", e.to_string())
    })
}

// ------------------------------------------------------------------ handler

async fn handle(State(s): State<Arc<Shared>>, req: Request) -> Response {
    let headers = req.headers().clone();
    if !host_ok(&headers, s.port) {
        return error(&api(
            ErrorCode::PermissionOriginNotAllowed,
            "the Host header must name the loopback address",
        ));
    }
    let origin = match origin(&headers) {
        Ok(o) => o,
        Err(e) => return error(&e),
    };
    let method = req.method().clone();
    if method == Method::OPTIONS {
        let mut r = Response::new(Body::empty());
        *r.status_mut() = StatusCode::NO_CONTENT;
        return cors(r, origin.as_deref(), true);
    }
    let path = req.uri().path().to_owned();
    let body = req.into_body();
    let response = match route(&s, &method, &path, &headers, origin.as_deref(), body).await {
        Ok(r) => r,
        Err(e) => error(&e),
    };
    cors(response, origin.as_deref(), false)
}

async fn route(
    s: &Arc<Shared>,
    method: &Method,
    path: &str,
    headers: &HeaderMap,
    origin: Option<&str>,
    body: Body,
) -> Result<Response, ApiError> {
    let not_found = || {
        api(
            ErrorCode::NotFoundMethod,
            &format!("no endpoint {method} {path}"),
        )
        .with_detail("method", path)
    };
    let rest = path.strip_prefix("/v1/").ok_or_else(not_found)?;
    if !(method == Method::GET && rest == "ping") {
        csrf_guard(headers, method)?;
        // v1.0: a locked app (or one switching profile) answers nothing but `ping`.
        if s.app.background_blocked() {
            return Err(fc_app::modules::profiles::locked_error(path));
        }
    }
    match (method.as_str(), rest) {
        ("GET", "ping") => Ok(json(
            StatusCode::OK,
            &PingResponse {
                app: "flashcard".into(),
                api_version: fc_api::API_VERSION.into(),
                version: env!("CARGO_PKG_VERSION").into(),
            },
        )),
        ("POST", "pair") => pair(s, headers, origin, body).await,
        ("POST", id) if id.starts_with("pair/") => {
            poll(s, id.trim_start_matches("pair/"), origin).await
        }
        ("GET", "auth" | "methods" | "decks") | ("POST", _) => {
            let client = authenticate(s, headers, origin).await?;
            match (method.as_str(), rest) {
                ("GET", "auth") => Ok(json(StatusCode::OK, &client)),
                ("GET", "methods") => {
                    let all = local_api::method_list(s.app.registry());
                    let methods = all
                        .methods
                        .into_iter()
                        .filter(|m| client.scopes.contains(&m.permission))
                        .collect();
                    Ok(json(StatusCode::OK, &LocalApiMethodList { methods }))
                }
                ("GET", "decks") => invoke(s, client, "decks.list", Value::Null).await,
                ("POST", "inbox") => {
                    let item = parse(read_json(body, headers).await?)?;
                    let app = s.app.clone();
                    let (dto, created) =
                        blocking(move || local_api::add_inbox_item(&app, &client, item)).await?;
                    let status = if created {
                        StatusCode::CREATED
                    } else {
                        StatusCode::OK
                    };
                    Ok(json(status, &dto))
                }
                ("POST", name) => {
                    let ok = !name.is_empty()
                        && name.len() <= 64
                        && name
                            .chars()
                            .all(|c| c.is_ascii_lowercase() || c == '.' || c == '_');
                    if !ok {
                        return Err(not_found());
                    }
                    let params = read_json(body, headers).await?;
                    invoke(s, client, name, params).await
                }
                _ => Err(not_found()),
            }
        }
        _ => Err(not_found()),
    }
}

async fn invoke(
    s: &Arc<Shared>,
    client: ApiClientDto,
    method: &str,
    params: Value,
) -> Result<Response, ApiError> {
    let app = s.app.clone();
    let method = method.to_owned();
    let params = if params.is_null() {
        Value::Object(serde_json::Map::new())
    } else {
        params
    };
    let v = blocking(move || local_api::call(&app, &client, &method, params)).await?;
    Ok(json(StatusCode::OK, &v))
}

/// Token → approved client (+ per-client rate limit, last-use note).
async fn authenticate(
    s: &Arc<Shared>,
    headers: &HeaderMap,
    origin: Option<&str>,
) -> Result<ApiClientDto, ApiError> {
    let hash = token_hash(&bearer(headers)?);
    let app = s.app.clone();
    let origin = origin.map(str::to_owned);
    let client = blocking(move || local_api::authenticate(&app, &hash, origin.as_deref())).await?;
    lock(&s.buckets)
        .entry(client.id)
        .or_insert_with(|| Bucket::full(CLIENT_BURST))
        .take(CLIENT_RATE_PER_SEC, CLIENT_BURST)
        .map_err(rate_limited)?;
    let due = {
        let mut t = lock(&s.touched);
        let now = Instant::now();
        let due = t
            .get(&client.id)
            .is_none_or(|last| now.duration_since(*last) >= Duration::from_secs(60));
        if due {
            t.insert(client.id, now);
        }
        due
    };
    if due {
        let app = s.app.clone();
        let id = client.id;
        // Bookkeeping only: a failure here never fails the request.
        let _ = blocking(move || local_api::touch_client(&app, id)).await;
    }
    Ok(client)
}

/// One client name (per origin) may ask only a few times in a while, whatever else is going on.
fn pair_name_allowed(s: &Shared, origin: Option<&str>, name: &str) -> Result<(), ApiError> {
    let key = format!(
        "{}|{}",
        origin.unwrap_or_default(),
        name.trim().to_lowercase()
    );
    let mut map = lock(&s.pair_names);
    if map.len() >= 256 {
        // A bucket idle for the whole window is full again: forget it.
        let window = Duration::from_secs_f64(PAIR_NAME_WINDOW_SECS);
        map.retain(|_, b| b.last.elapsed() < window);
    }
    map.entry(key)
        .or_insert_with(|| Bucket::full(PAIR_PER_NAME))
        .take(PAIR_PER_NAME / PAIR_NAME_WINDOW_SECS, PAIR_PER_NAME)
        .map_err(rate_limited)
}

async fn pair(
    s: &Arc<Shared>,
    headers: &HeaderMap,
    origin: Option<&str>,
    body: Body,
) -> Result<Response, ApiError> {
    lock(&s.pair_bucket)
        .take(PAIR_PER_MINUTE / 60.0, PAIR_PER_MINUTE)
        .map_err(rate_limited)?;
    let req: PairRequest = parse(read_json(body, headers).await?)?;
    pair_name_allowed(s, origin, &req.client_name)?;
    let token = format!(
        "fc1_{}",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(random_bytes::<32>()?)
    );
    let request_id: String = random_bytes::<24>()?
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    let hash = token_hash(&token);
    let app = s.app.clone();
    let o = origin.map(str::to_owned);
    let client =
        blocking(move || local_api::record_pairing(&app, &req, o.as_deref(), &hash)).await?;
    let ttl = Duration::from_millis(u64::try_from(local_api::PAIRING_TTL_MS).unwrap_or(600_000));
    {
        let mut map = lock(&s.pairings);
        let now = Instant::now();
        map.retain(|_, p| p.expires > now);
        map.insert(
            request_id.clone(),
            Pairing {
                client_id: client.id,
                token: Some(token),
                origin: origin.map(str::to_owned),
                expires: now + ttl,
            },
        );
    }
    Ok(json(
        StatusCode::CREATED,
        &PairResponse {
            request_id,
            status: PairStatus::Pending,
            expires_at: Timestamp(client.created_at.0 + local_api::PAIRING_TTL_MS),
        },
    ))
}

async fn poll(
    s: &Arc<Shared>,
    request_id: &str,
    origin: Option<&str>,
) -> Result<Response, ApiError> {
    let unknown = || {
        api(ErrorCode::NotFoundPairing, "no such pairing request")
            .with_detail("request_id", request_id)
    };
    let (client_id, paired_origin, expired) = {
        let map = lock(&s.pairings);
        let p = map.get(request_id).ok_or_else(unknown)?;
        (p.client_id, p.origin.clone(), p.expires <= Instant::now())
    };
    local_api::check_origin(paired_origin.as_deref(), origin)?;
    let answer = |status, client_id, token, scopes| {
        json(
            StatusCode::OK,
            &PairStatusResponse {
                status,
                client_id,
                token,
                scopes,
            },
        )
    };
    if expired {
        lock(&s.pairings).remove(request_id);
        return Ok(answer(PairStatus::Expired, None, None, vec![]));
    }
    let app = s.app.clone();
    let client = blocking(move || local_api::client(&app, client_id)).await?;
    Ok(match client {
        None => {
            lock(&s.pairings).remove(request_id);
            answer(PairStatus::Expired, None, None, vec![])
        }
        Some(c) => match c.status {
            ApiClientStatus::Pending => answer(PairStatus::Pending, None, None, vec![]),
            ApiClientStatus::Approved => {
                let token = lock(&s.pairings)
                    .get_mut(request_id)
                    .and_then(|p| p.token.take());
                answer(PairStatus::Approved, Some(c.id), token, c.scopes)
            }
            ApiClientStatus::Denied | ApiClientStatus::Revoked => {
                lock(&s.pairings).remove(request_id);
                answer(PairStatus::Denied, None, None, vec![])
            }
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuses_follow_error_categories() {
        let s = |c| status_of(&ApiError::new(c, ""));
        assert_eq!(
            s(ErrorCode::PermissionUnauthenticated),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(s(ErrorCode::PermissionDenied), StatusCode::FORBIDDEN);
        assert_eq!(
            s(ErrorCode::PermissionOriginNotAllowed),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            s(ErrorCode::ValidationInvalidParams),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            s(ErrorCode::ValidationPayloadTooLarge),
            StatusCode::PAYLOAD_TOO_LARGE
        );
        assert_eq!(s(ErrorCode::NotFoundMethod), StatusCode::NOT_FOUND);
        assert_eq!(
            s(ErrorCode::ConflictRateLimited),
            StatusCode::TOO_MANY_REQUESTS
        );
        assert_eq!(
            s(ErrorCode::ConflictModuleDisabled),
            StatusCode::SERVICE_UNAVAILABLE
        );
        assert_eq!(s(ErrorCode::ConflictInboxItemState), StatusCode::CONFLICT);
        assert_eq!(s(ErrorCode::Internal), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn token_hashes_are_sha256_hex() {
        assert_eq!(
            token_hash("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn buckets_refill() {
        let mut b = Bucket::full(2.0);
        assert!(b.take(1.0, 2.0).is_ok());
        assert!(b.take(1.0, 2.0).is_ok());
        assert_eq!(b.take(1.0, 2.0), Err(1));
    }
}
