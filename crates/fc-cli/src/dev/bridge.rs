//! The dev bridge's HTTP server (v1.3, development builds only; see the parent module).
//!
//! Wire format:
//!
//! * `POST /rpc` with `{"method": "v1.decks.create", "params": {…}}` (the `v1.` prefix may be
//!   left out; missing or `null` params mean none) → `200 {"ok": true, "result": …}`, or
//!   `200 {"ok": false, "error": ApiError}` when the app refuses — the same serialized error the
//!   desktop IPC command rejects with.
//! * `GET /events`: Server-Sent Events, one `data:` line per post-commit [`EventEnvelope`]
//!   (JSON, exactly what the desktop shell emits on `fc://event`).
//! * `GET /health` → `{"ok": true}` (no token: lets a harness wait for the server).
//!
//! Guards: the socket binds `127.0.0.1` only; the `Host` header must name the loopback address
//! and the bound port (DNS-rebinding guard); a browser `Origin` must be a loopback `http://`
//! origin (the vite dev server; CORS is answered for it only); every route but `/health` needs
//! the token, as `Authorization: Bearer <token>` or — for `EventSource`, which cannot send
//! headers — `?token=<token>`. Transport refusals use the same `{"ok": false, "error"}` body
//! with an HTTP status (`401`, `403`, `400`, `404`, `413`).

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Request, State};
use axum::http::{header, HeaderMap, HeaderValue, Method, StatusCode};
use axum::middleware::{self, Next};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::Router;
use fc_api::v1::events::EventEnvelope;
use fc_api::{ApiError, ErrorCode, API_VERSION};
use fc_app::events::SubscriptionId;
use fc_app::App;
use serde_json::{json, Value};
use std::convert::Infallible;
use std::net::{Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{broadcast, watch};

/// Shortest token `--token` accepts.
pub const MIN_TOKEN_CHARS: usize = 16;
/// Largest request body (pasted imports can be big; it is a local development tool).
pub const MAX_BODY_BYTES: usize = 16 * 1024 * 1024;
/// Events buffered per slow `/events` client before it skips ahead.
const EVENT_BUFFER: usize = 1024;

/// A fresh random token: 32 bytes from the OS generator, hex.
pub fn random_token() -> Result<String, String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| e.to_string())?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

#[derive(Clone)]
struct Shared {
    app: Arc<App>,
    token: Arc<str>,
    port: u16,
    events: broadcast::Sender<Arc<str>>,
    shutdown: watch::Receiver<bool>,
}

/// A running bridge (its own runtime thread). Dropping it stops the server.
pub struct Bridge {
    addr: SocketAddr,
    token: String,
    app: Arc<App>,
    subscription: Option<SubscriptionId>,
    shutdown: Option<watch::Sender<bool>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Bridge {
    /// Binds `127.0.0.1:<port>` (`0` = any free port) synchronously, so a busy port is reported
    /// here, subscribes to `app`'s events and serves on a background thread.
    pub fn start(app: Arc<App>, port: u16, token: String) -> std::io::Result<Self> {
        let listener = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, port))?;
        let addr = listener.local_addr()?;
        if !addr.ip().is_loopback() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!("{addr} is not a loopback address"),
            ));
        }
        listener.set_nonblocking(true)?;
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("fc-dev-bridge")
            .enable_all()
            .build()?;
        let (events, _) = broadcast::channel::<Arc<str>>(EVENT_BUFFER);
        let sender = events.clone();
        // Post-commit events, serialized once, fanned out to every `/events` stream.
        let subscription = app.events().subscribe(Arc::new(move |e: &EventEnvelope| {
            if let Ok(json) = serde_json::to_string(e) {
                let _ = sender.send(Arc::from(json));
            }
        }));
        let (stop_tx, stop_rx) = watch::channel(false);
        let shared = Shared {
            app: app.clone(),
            token: Arc::from(token.as_str()),
            port: addr.port(),
            events,
            shutdown: stop_rx.clone(),
        };
        let router = router(shared);
        let thread = std::thread::Builder::new()
            .name("fc-dev-bridge".into())
            .spawn(move || {
                runtime.block_on(async move {
                    let Ok(listener) = tokio::net::TcpListener::from_std(listener) else {
                        return;
                    };
                    let mut stop = stop_rx;
                    let _ = axum::serve(listener, router)
                        .with_graceful_shutdown(async move {
                            let _ = stop.wait_for(|s| *s).await;
                        })
                        .await;
                });
            })?;
        Ok(Self {
            addr,
            token,
            app,
            subscription: Some(subscription),
            shutdown: Some(stop_tx),
            thread: Some(thread),
        })
    }

    /// The bound address.
    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }

    /// `http://127.0.0.1:<port>`.
    pub fn url(&self) -> String {
        format!("http://{}", self.local_addr())
    }

    /// The token clients must send.
    pub fn token(&self) -> &str {
        &self.token
    }

    fn halt(&mut self) {
        if let Some(id) = self.subscription.take() {
            self.app.events().unsubscribe(id);
        }
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(true);
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for Bridge {
    fn drop(&mut self) {
        self.halt();
    }
}

fn router(shared: Shared) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/rpc", post(rpc))
        .route("/events", get(events))
        .fallback(not_found)
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
        .layer(middleware::from_fn_with_state(shared.clone(), guard))
        .with_state(shared)
}

// ------------------------------------------------------------------ responses

fn json_response(status: StatusCode, body: &Value) -> Response {
    (
        status,
        [(header::CONTENT_TYPE, "application/json")],
        body.to_string(),
    )
        .into_response()
}

fn refuse(status: StatusCode, code: ErrorCode, message: &str) -> Response {
    let error = serde_json::to_value(ApiError::new(code, message)).unwrap_or(Value::Null);
    json_response(status, &json!({ "ok": false, "error": error }))
}

fn outcome(result: Result<Value, ApiError>) -> Response {
    let body = match result {
        Ok(v) => json!({ "ok": true, "result": v }),
        Err(e) => json!({ "ok": false, "error": serde_json::to_value(e).unwrap_or(Value::Null) }),
    };
    json_response(StatusCode::OK, &body)
}

// ------------------------------------------------------------------ guards

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

/// A loopback `http://` origin (any port): the vite dev server, a Playwright page.
fn loopback_origin(origin: &str) -> bool {
    let Some(rest) = origin.strip_prefix("http://") else {
        return false;
    };
    let host = match rest.strip_prefix("[::1]") {
        Some(after) => return after.is_empty() || port_suffix(after),
        None => rest,
    };
    let (name, after) = match host.find(':') {
        Some(i) => (&host[..i], &host[i..]),
        None => (host, ""),
    };
    matches!(name, "127.0.0.1" | "localhost") && (after.is_empty() || port_suffix(after))
}

fn port_suffix(s: &str) -> bool {
    s.strip_prefix(':')
        .is_some_and(|p| !p.is_empty() && p.parse::<u16>().is_ok())
}

/// Constant-time comparison (no early exit on the first differing byte).
fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'%' {
            if let Some(Ok(v)) = s.get(i + 1..i + 3).map(|h| u8::from_str_radix(h, 16)) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(if b == b'+' { b' ' } else { b });
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn presented_token(req: &Request) -> Option<String> {
    if let Some(bearer) = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
    {
        return Some(bearer.trim().to_owned());
    }
    req.uri().query().and_then(|q| {
        q.split('&')
            .filter_map(|kv| kv.split_once('='))
            .find(|(k, _)| *k == "token")
            .map(|(_, v)| percent_decode(v))
    })
}

fn cors(mut res: Response, origin: Option<&HeaderValue>) -> Response {
    if let Some(o) = origin {
        let h = res.headers_mut();
        h.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, o.clone());
        h.insert(header::VARY, HeaderValue::from_static("Origin"));
    }
    res
}

/// Host, Origin (+ CORS preflight) and token checks for every route.
async fn guard(State(s): State<Shared>, req: Request, next: Next) -> Response {
    if !host_ok(req.headers(), s.port) {
        return refuse(
            StatusCode::FORBIDDEN,
            ErrorCode::PermissionOriginNotAllowed,
            "the Host header must name the loopback address of the dev bridge",
        );
    }
    let origin = req.headers().get(header::ORIGIN).cloned();
    if let Some(o) = &origin {
        if !loopback_origin(o.to_str().unwrap_or_default()) {
            return refuse(
                StatusCode::FORBIDDEN,
                ErrorCode::PermissionOriginNotAllowed,
                "only loopback http:// pages may call the dev bridge",
            );
        }
    }
    if req.method() == Method::OPTIONS {
        let mut res = StatusCode::NO_CONTENT.into_response();
        let h = res.headers_mut();
        h.insert(
            header::ACCESS_CONTROL_ALLOW_METHODS,
            HeaderValue::from_static("GET, POST, OPTIONS"),
        );
        h.insert(
            header::ACCESS_CONTROL_ALLOW_HEADERS,
            HeaderValue::from_static("authorization, content-type"),
        );
        h.insert(
            header::ACCESS_CONTROL_MAX_AGE,
            HeaderValue::from_static("600"),
        );
        return cors(res, origin.as_ref());
    }
    if req.uri().path() != "/health" {
        let ok = presented_token(&req).is_some_and(|t| same(t.as_bytes(), s.token.as_bytes()));
        if !ok {
            return cors(
                refuse(
                    StatusCode::UNAUTHORIZED,
                    ErrorCode::PermissionUnauthenticated,
                    "the dev bridge token is missing or wrong",
                ),
                origin.as_ref(),
            );
        }
    }
    let res = next.run(req).await;
    cors(res, origin.as_ref())
}

// ------------------------------------------------------------------ routes

async fn health() -> Response {
    json_response(StatusCode::OK, &json!({ "ok": true }))
}

async fn not_found() -> Response {
    refuse(
        StatusCode::NOT_FOUND,
        ErrorCode::NotFoundMethod,
        "dev bridge routes: POST /rpc, GET /events, GET /health",
    )
}

async fn rpc(State(s): State<Shared>, body: Bytes) -> Response {
    #[derive(serde::Deserialize)]
    struct Call {
        method: String,
        #[serde(default)]
        params: Value,
    }
    let call: Call = match serde_json::from_slice(&body) {
        Ok(c) => c,
        Err(e) => {
            return refuse(
                StatusCode::BAD_REQUEST,
                ErrorCode::ValidationInvalidParams,
                &format!("expected {{\"method\": …, \"params\": …}}: {e}"),
            )
        }
    };
    let prefix = format!("{API_VERSION}.");
    let method = if call.method.starts_with(&prefix) {
        call.method
    } else {
        format!("{prefix}{}", call.method)
    };
    let app = s.app.clone();
    let result = tokio::task::spawn_blocking(move || {
        fc_app::transport::dispatch(&app, &method, call.params)
    })
    .await
    .unwrap_or_else(|e| Err(ApiError::new(ErrorCode::Internal, e.to_string())));
    outcome(result)
}

async fn events(State(s): State<Shared>) -> Response {
    let rx = s.events.subscribe();
    let stop = s.shutdown.clone();
    let stream = futures_util::stream::unfold((rx, stop), |(mut rx, mut stop)| async move {
        loop {
            let got = tokio::select! {
                got = rx.recv() => got,
                _ = stop.wait_for(|s| *s) => return None,
            };
            match got {
                Ok(json) => {
                    let event = Event::default().data(&*json);
                    return Some((Ok::<_, Infallible>(event), (rx, stop)));
                }
                // A slow reader skipped events; screens refetch on the next one.
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => return None,
            }
        }
    });
    Sse::new(stream)
        .keep_alive(KeepAlive::new().interval(Duration::from_secs(15)))
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_loopback_origins_pass() {
        for ok in [
            "http://127.0.0.1:5173",
            "http://localhost:20001",
            "http://[::1]:1420",
            "http://localhost",
        ] {
            assert!(loopback_origin(ok), "{ok}");
        }
        for bad in [
            "https://127.0.0.1:5173",
            "http://127.0.0.1.evil.com",
            "http://localhost.evil.com:80",
            "http://192.168.1.2:5173",
            "http://127.0.0.1:x",
            "chrome-extension://abc",
            "null",
        ] {
            assert!(!loopback_origin(bad), "{bad}");
        }
    }

    #[test]
    fn tokens_compare_exactly() {
        assert!(same(b"abc", b"abc"));
        assert!(!same(b"abc", b"abd"));
        assert!(!same(b"abc", b"abcd"));
        assert_eq!(percent_decode("a%2Fb+c"), "a/b c");
        assert_eq!(percent_decode("100%"), "100%");
        let t = random_token().unwrap_or_default();
        assert_eq!(t.len(), 64);
    }
}
