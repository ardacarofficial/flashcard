//! The dev bridge in process: same dispatch as the desktop IPC (envelope, errors, refusals),
//! the guards (token, Host, Origin, CORS) and post-commit events over SSE.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::bridge::Bridge;
use fc_app::testing::TestApp;
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::Arc;
use std::time::Duration;

const TOKEN: &str = "0123456789abcdef-test-token";

fn bridge() -> Bridge {
    let t = TestApp::mem();
    Bridge::start(Arc::new(t.app), 0, TOKEN.to_owned()).unwrap()
}

struct Reply {
    status: u16,
    head: String,
    body: Value,
}

fn http(addr: SocketAddr, method: &str, path: &str, headers: &[(&str, &str)], body: &str) -> Reply {
    let mut s = TcpStream::connect(addr).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
    let mut req = format!(
        "{method} {path} HTTP/1.1\r\nConnection: close\r\nContent-Length: {}\r\n",
        body.len()
    );
    if !headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("host")) {
        req.push_str(&format!("Host: 127.0.0.1:{}\r\n", addr.port()));
    }
    for (k, v) in headers {
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    req.push_str("\r\n");
    req.push_str(body);
    s.write_all(req.as_bytes()).unwrap();
    let mut raw = String::new();
    s.read_to_string(&mut raw).unwrap();
    let (head, rest) = raw.split_once("\r\n\r\n").unwrap();
    let status = head.split(' ').nth(1).unwrap().parse().unwrap();
    Reply {
        status,
        head: head.to_ascii_lowercase(),
        body: serde_json::from_str(rest.trim()).unwrap_or(Value::Null),
    }
}

fn rpc(addr: SocketAddr, method: &str, params: &Value) -> Value {
    let auth = format!("Bearer {TOKEN}");
    let r = http(
        addr,
        "POST",
        "/rpc",
        &[
            ("Authorization", auth.as_str()),
            ("Content-Type", "application/json"),
        ],
        &json!({ "method": method, "params": params }).to_string(),
    );
    assert_eq!(r.status, 200, "{}", r.body);
    r.body
}

#[test]
fn dispatches_public_methods_like_the_desktop_ipc() {
    let b = bridge();
    let created = rpc(
        b.local_addr(),
        "v1.decks.create",
        &json!({ "path": "Dil::İngilizce" }),
    );
    assert_eq!(created["ok"], true);
    assert_eq!(created["result"]["path"], "Dil::İngilizce");
    // The `v1.` prefix may be left out; `null` params mean none.
    let tree = rpc(b.local_addr(), "decks.tree", &Value::Null);
    assert_eq!(tree["result"]["roots"][0]["deck"]["name"], "Dil");
}

#[test]
fn refusals_have_the_contract_error_shape() {
    let b = bridge();
    let bad = rpc(b.local_addr(), "v1.decks.create", &json!({ "path": "" }));
    assert_eq!(bad["ok"], false);
    for k in ["code", "message_key", "message", "retryable"] {
        assert!(bad["error"].get(k).is_some(), "missing {k}: {bad}");
    }
    let internal = rpc(
        b.local_addr(),
        "v1.decks.restore_affected",
        &json!({ "id": "01900000-0000-7000-8000-0000000000aa", "decks": [], "notes": [], "cards": [] }),
    );
    assert_eq!(internal["error"]["code"], "PERMISSION_DENIED");
    let unknown = rpc(b.local_addr(), "v1.nope", &Value::Null);
    assert_eq!(unknown["error"]["code"], "NOT_FOUND_METHOD");
}

#[test]
fn the_token_is_required_except_for_health() {
    let b = bridge();
    let a = b.local_addr();
    let body = json!({ "method": "decks.tree" }).to_string();
    let none = http(a, "POST", "/rpc", &[], &body);
    assert_eq!(none.status, 401);
    assert_eq!(none.body["error"]["code"], "PERMISSION_UNAUTHENTICATED");
    let wrong = http(
        a,
        "POST",
        "/rpc",
        &[("Authorization", "Bearer nope-nope-nope-nope")],
        &body,
    );
    assert_eq!(wrong.status, 401);
    let query = http(a, "POST", &format!("/rpc?token={TOKEN}"), &[], &body);
    assert_eq!(query.status, 200);
    assert_eq!(query.body["ok"], true);
    assert_eq!(http(a, "GET", "/health", &[], "").body["ok"], true);
    assert_eq!(
        http(a, "GET", &format!("/nope?token={TOKEN}"), &[], "").status,
        404
    );
    let garbage = http(a, "POST", &format!("/rpc?token={TOKEN}"), &[], "{");
    assert_eq!(garbage.status, 400);
}

#[test]
fn only_loopback_hosts_and_origins_get_in() {
    let b = bridge();
    let a = b.local_addr();
    let auth = format!("Bearer {TOKEN}");
    let body = json!({ "method": "decks.tree" }).to_string();
    // DNS rebinding: a page on evil.test resolving to 127.0.0.1.
    let rebound = http(
        a,
        "POST",
        "/rpc",
        &[("Host", "evil.test"), ("Authorization", auth.as_str())],
        &body,
    );
    assert_eq!(rebound.status, 403);
    let health = http(a, "GET", "/health", &[("Host", "evil.test")], "");
    assert_eq!(health.status, 403);
    let page = http(
        a,
        "POST",
        "/rpc",
        &[
            ("Origin", "https://example.com"),
            ("Authorization", auth.as_str()),
        ],
        &body,
    );
    assert_eq!(page.status, 403);
    assert_eq!(page.body["error"]["code"], "PERMISSION_ORIGIN_NOT_ALLOWED");
    // The vite dev server: CORS answered for it.
    let vite = "http://127.0.0.1:24680";
    let pre = http(
        a,
        "OPTIONS",
        "/rpc",
        &[
            ("Origin", vite),
            ("Access-Control-Request-Method", "POST"),
            (
                "Access-Control-Request-Headers",
                "authorization,content-type",
            ),
        ],
        "",
    );
    assert_eq!(pre.status, 204);
    assert!(
        pre.head
            .contains(&format!("access-control-allow-origin: {vite}")),
        "{}",
        pre.head
    );
    assert!(
        pre.head
            .contains("access-control-allow-headers: authorization"),
        "{}",
        pre.head
    );
    let ok = http(
        a,
        "POST",
        "/rpc",
        &[("Origin", vite), ("Authorization", auth.as_str())],
        &body,
    );
    assert_eq!(ok.status, 200);
    assert!(ok
        .head
        .contains(&format!("access-control-allow-origin: {vite}")));
}

#[test]
fn post_commit_events_stream_over_sse() {
    let b = bridge();
    let a = b.local_addr();
    let mut s = TcpStream::connect(a).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(20))).unwrap();
    write!(
        s,
        "GET /events?token={TOKEN} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nAccept: text/event-stream\r\n\r\n",
        a.port()
    )
    .unwrap();
    let mut reader = BufReader::new(s);
    let mut status = String::new();
    reader.read_line(&mut status).unwrap();
    assert!(status.contains(" 200 "), "{status}");
    // Headers end with an empty line; the stream is subscribed once they are out.
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        if line.trim().is_empty() {
            break;
        }
    }
    let created = rpc(a, "decks.create", &json!({ "path": "Olay" }));
    assert_eq!(created["ok"], true);
    let mut seen = Vec::new();
    loop {
        let mut line = String::new();
        assert!(
            reader.read_line(&mut line).unwrap() > 0,
            "stream ended: {seen:?}"
        );
        if let Some(data) = line.trim().strip_prefix("data:") {
            let env: Value = serde_json::from_str(data.trim()).unwrap();
            let kind = env["event"]["type"].as_str().unwrap_or_default().to_owned();
            seen.push(kind.clone());
            if kind == "deck_created" {
                break;
            }
        }
    }
}
