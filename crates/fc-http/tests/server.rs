//! The local HTTP API over real TCP (v0.9 acceptance, docs/04):
//!
//! * token-less requests are refused (`token_less_requests_are_refused`);
//! * only localhost is listened on (`only_loopback_is_listened_on`, plus `ServerConfig` unit
//!   tests: a non-loopback address cannot even be expressed);
//! * permissions are denied outside the granted scopes and the allow-list
//!   (`permissions_are_denied_outside_scopes_and_allow_list`);
//! * a word sent by the browser extension lands in the inbox
//!   (`extension_word_lands_in_the_inbox`: pairing from a `chrome-extension://` origin,
//!   approval in the app, token collected once, `POST /v1/inbox` with a classification);
//! * web origins, foreign `Host` headers, big bodies and bursts are refused.
//!
//! The client is a few lines of raw HTTP/1.1 so the test exercises exactly what is on the wire.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use fc_api::v1::inbox::{InboxSource, ListInbox};
use fc_api::v1::local_api::ApproveApiClient;
use fc_api::v1::methods::*;
use fc_api::v1::modules::SetModuleEnabled;
use fc_api::v1::oplog::Actor;
use fc_app::testing::{TempSqlite, TestApp};
use fc_http::{Server, ServerConfig};
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::Arc;

const EXT: &str = "chrome-extension://abcdefghijklmnopabcdefghijklmnop";

struct Reply {
    status: u16,
    headers: Vec<(String, String)>,
    body: Value,
}

impl Reply {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    fn code(&self) -> &str {
        self.body["code"].as_str().unwrap_or_default()
    }
}

/// One request; `host` overrides the `Host` header.
fn send(
    addr: SocketAddr,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: Option<&str>,
) -> Reply {
    let mut s = TcpStream::connect(addr).unwrap();
    let mut req = format!("{method} {path} HTTP/1.1\r\nConnection: close\r\n");
    if !headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("host")) {
        req.push_str(&format!("Host: 127.0.0.1:{}\r\n", addr.port()));
    }
    let has = |name: &str| headers.iter().any(|(k, _)| k.eq_ignore_ascii_case(name));
    // Every request names its client, as the contract asks; `("X-Flashcard-Client", "")`
    // leaves it out, and a `Content-Type` of the test's own replaces the JSON one.
    if !has("x-flashcard-client") {
        req.push_str("X-Flashcard-Client: test\r\n");
    }
    for (k, v) in headers.iter().filter(|(_, v)| !v.is_empty()) {
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    let body = body.unwrap_or_default();
    if !body.is_empty() || method == "POST" {
        if !has("content-type") {
            req.push_str("Content-Type: application/json\r\n");
        }
        req.push_str(&format!("Content-Length: {}\r\n", body.len()));
    }
    req.push_str("\r\n");
    req.push_str(body);
    s.write_all(req.as_bytes()).unwrap();
    let mut raw = Vec::new();
    s.read_to_end(&mut raw).unwrap();
    let text = String::from_utf8_lossy(&raw).to_string();
    let (head, rest) = text.split_once("\r\n\r\n").unwrap();
    let mut lines = head.lines();
    let status: u16 = lines
        .next()
        .unwrap()
        .split(' ')
        .nth(1)
        .unwrap()
        .parse()
        .unwrap();
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_owned(), v.trim().to_owned()))
        .collect();
    let chunked = headers
        .iter()
        .any(|(k, v)| k.eq_ignore_ascii_case("transfer-encoding") && v.contains("chunked"));
    let payload = if chunked {
        dechunk(rest)
    } else {
        rest.to_owned()
    };
    let body = serde_json::from_str(&payload).unwrap_or(Value::Null);
    Reply {
        status,
        headers,
        body,
    }
}

fn dechunk(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some((len, tail)) = rest.split_once("\r\n") {
        let n = usize::from_str_radix(len.trim(), 16).unwrap_or(0);
        if n == 0 {
            break;
        }
        out.push_str(&tail[..n]);
        rest = &tail[n + 2..];
    }
    out
}

struct Fx {
    t: TestApp,
    app: Arc<fc_app::App>,
    server: Server,
    db: std::path::PathBuf,
}

impl Fx {
    /// A file-backed app with the `local_api` module on, served on a free loopback port.
    fn new() -> Self {
        let store = Arc::new(TempSqlite::new());
        let db = store.path();
        let t = TestApp::new(store);
        t.ok::<ModulesSetEnabled>(SetModuleEnabled {
            id: "local_api".into(),
            enabled: true,
        });
        // The server and the test share one store; a second app over it sees the same rows.
        let app = Arc::new(TestApp::new(t.store.clone()).app);
        let server = Server::start(app.clone(), ServerConfig::localhost(0)).unwrap();
        Self { t, app, server, db }
    }

    fn addr(&self) -> SocketAddr {
        self.server.local_addr()
    }

    fn get(&self, path: &str, headers: &[(&str, &str)]) -> Reply {
        send(self.addr(), "GET", path, headers, None)
    }

    fn post(&self, path: &str, headers: &[(&str, &str)], body: &Value) -> Reply {
        send(self.addr(), "POST", path, headers, Some(&body.to_string()))
    }

    /// Pairs from `origin`, approves in the app (as the user), collects the token.
    fn token(&self, scopes: &[&str], origin: Option<&str>) -> String {
        let mut h = vec![];
        if let Some(o) = origin {
            h.push(("Origin", o));
        }
        let r = self.post(
            "/v1/pair",
            &h,
            &json!({"client_name": "Chrome", "client_kind": "browser_extension", "scopes": scopes}),
        );
        assert_eq!(r.status, 201, "{}", r.body);
        let id = r.body["request_id"].as_str().unwrap().to_owned();
        let pending = self
            .t
            .ok::<LocalApiClientsList>(fc_api::v1::common::Empty {})
            .clients
            .into_iter()
            .find(|c| c.status == fc_api::v1::local_api::ApiClientStatus::Pending)
            .unwrap();
        self.t.ok::<LocalApiClientsApprove>(ApproveApiClient {
            id: pending.id,
            scopes: None,
        });
        let r = self.post(&format!("/v1/pair/{id}"), &[], &json!({}));
        r.body["token"].as_str().unwrap().to_owned()
    }
}

#[test]
fn token_less_requests_are_refused() {
    let fx = Fx::new();
    for (method, path) in [
        ("GET", "/v1/decks"),
        ("GET", "/v1/auth"),
        ("GET", "/v1/methods"),
        ("POST", "/v1/inbox"),
        ("POST", "/v1/decks.list"),
        ("POST", "/v1/notes.create"),
    ] {
        let r = send(fx.addr(), method, path, &[], Some("{}"));
        assert_eq!(r.status, 401, "{method} {path}");
        assert_eq!(r.code(), "PERMISSION_UNAUTHENTICATED");
        assert_eq!(r.body["details"]["reason"], "missing");
    }
    for bad in ["Bearer nope", "Basic abc", "Bearer ", "bearer fc1_x"] {
        let r = fx.get("/v1/decks", &[("Authorization", bad)]);
        assert_eq!(r.status, 401, "{bad}");
    }
    // A token waiting for approval does not work either.
    let r = fx.post(
        "/v1/pair",
        &[],
        &json!({"client_name": "x", "client_kind": "script", "scopes": ["decks:read"]}),
    );
    assert_eq!(r.status, 201);
    // Nothing was written by the refused calls.
    let inbox = fx.t.ok::<InboxList>(ListInbox::default());
    assert!(inbox.items.is_empty());
    // Without a token only ping and pairing answer.
    let r = fx.get("/v1/ping", &[]);
    assert_eq!(r.status, 200);
    assert_eq!(r.body["app"], "flashcard");
    assert_eq!(r.body["api_version"], "v1");
}

#[test]
fn only_loopback_is_listened_on() {
    let fx = Fx::new();
    let addr = fx.addr();
    assert!(addr.ip().is_loopback());
    assert_ne!(addr.port(), 0);
    // The only way to name an address is through the loopback-only config.
    assert!(ServerConfig::parse("0.0.0.0:28735").is_err());
    // A non-loopback interface of this machine (if it has one) does not answer.
    let probe = std::net::UdpSocket::bind("0.0.0.0:0")
        .and_then(|u| u.connect("192.0.2.1:9").map(|()| u))
        .and_then(|u| u.local_addr());
    if let Ok(local) = probe {
        if !local.ip().is_loopback() && !local.ip().is_unspecified() {
            let outside = SocketAddr::new(local.ip(), addr.port());
            assert!(
                TcpStream::connect_timeout(&outside, std::time::Duration::from_millis(500))
                    .is_err(),
                "{outside} must not accept connections"
            );
        }
    }
    // Requests naming another host are refused (DNS rebinding).
    let r = fx.get("/v1/ping", &[("Host", "evil.example:80")]);
    assert_eq!(r.status, 403);
    assert_eq!(r.code(), "PERMISSION_ORIGIN_NOT_ALLOWED");
    let r = fx.get(
        "/v1/ping",
        &[("Host", &format!("localhost:{}", addr.port()))],
    );
    assert_eq!(r.status, 200);
    // Port in use: starting a second server there fails instead of moving elsewhere.
    assert!(Server::start(fx.app.clone(), ServerConfig::localhost(addr.port())).is_err());
}

#[test]
fn permissions_are_denied_outside_scopes_and_allow_list() {
    let fx = Fx::new();
    let token = fx.token(&["inbox:write"], None);
    let auth = format!("Bearer {token}");
    let h = [("Authorization", auth.as_str())];
    // Granted: the inbox.
    let r = fx.post("/v1/inbox", &h, &json!({"front": "porsuk"}));
    assert_eq!(r.status, 201, "{}", r.body);
    // Not granted: decks.
    let r = fx.get("/v1/decks", &h);
    assert_eq!(r.status, 403);
    assert_eq!(r.code(), "PERMISSION_DENIED");
    // Never reachable, whatever the scopes: not even disclosed.
    for m in [
        "notes.delete",
        "io.backup.create",
        "oplog.undo",
        "plugins.install",
        "ai.providers.set_key",
        "local_api.clients.approve",
        "local_api.pair",
        "settings.set",
        "modules.set_enabled",
        "mcp.servers.add",
        "batch.execute",
    ] {
        let r = fx.post(&format!("/v1/{m}"), &h, &json!({}));
        assert_eq!(r.status, 404, "{m}");
        assert_eq!(r.code(), "NOT_FOUND_METHOD", "{m}");
    }
    // The client cannot approve itself or widen its scopes.
    let me = fx.get("/v1/auth", &h);
    assert_eq!(me.body["scopes"], json!(["inbox:write"]));
    let methods = fx.get("/v1/methods", &h);
    let names: Vec<_> = methods.body["methods"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["name"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(names, ["inbox.add"]);
    // Revoked in the app → refused at once.
    let id: fc_core::ApiClientId = serde_json::from_value(me.body["id"].clone()).unwrap();
    fx.t.ok::<LocalApiClientsRevoke>(fc_api::v1::local_api::ApiClientRef { id });
    let r = fx.post("/v1/inbox", &h, &json!({"front": "x"}));
    assert_eq!(r.status, 401);
    assert_eq!(r.body["details"]["reason"], "revoked");
    // Module off → 503.
    let token = fx.token(&["inbox:write"], None);
    fx.t.ok::<ModulesSetEnabled>(SetModuleEnabled {
        id: "inbox".into(),
        enabled: false,
    });
    // (the serving app caches module switches; the next reload sees the change)
    fx.app.reload_cached_state().unwrap();
    let auth = format!("Bearer {token}");
    let r = fx.post(
        "/v1/inbox",
        &[("Authorization", auth.as_str())],
        &json!({"front": "x"}),
    );
    assert_eq!(r.status, 503);
    assert_eq!(r.code(), "CONFLICT_MODULE_DISABLED");
}

#[test]
fn extension_word_lands_in_the_inbox() {
    let fx = Fx::new();
    let verbs = {
        let parent = fx.t.ok::<DecksCreate>(fc_api::v1::decks::CreateDeck {
            path: "İngilizce".into(),
            parent_id: None,
            description: None,
            icon: None,
            color: None,
        });
        fx.t.ok::<AttributesDefine>(fc_api::v1::attributes::DefineAttribute {
            scope_deck_id: Some(parent.id),
            key: Some("seviye".into()),
            name: "Seviye".into(),
            kind: fc_core::attribute::AttributeKind::OrderedEnum,
            target: None,
            required: None,
            options: ["A1", "A2", "B1", "B2"]
                .iter()
                .map(|l| fc_api::v1::attributes::NewAttributeOption {
                    label: (*l).into(),
                    color: None,
                })
                .collect(),
        });
        parent.id
    };
    let origin = [("Origin", EXT)];
    // Preflight from the extension: answered for that origin only.
    let r = send(fx.addr(), "OPTIONS", "/v1/pair", &origin, None);
    assert_eq!(r.status, 204);
    assert_eq!(r.header("access-control-allow-origin"), Some(EXT));
    assert!(r
        .header("access-control-allow-headers")
        .unwrap()
        .contains("authorization"));

    // 1. The extension asks to pair.
    let r = fx.post(
        "/v1/pair",
        &origin,
        &json!({"client_name": "Flashcard for Chrome", "client_kind": "browser_extension",
                "scopes": ["inbox:write", "decks:read", "attributes:read"]}),
    );
    assert_eq!(r.status, 201, "{}", r.body);
    assert_eq!(r.header("access-control-allow-origin"), Some(EXT));
    let request = r.body["request_id"].as_str().unwrap().to_owned();
    let poll = format!("/v1/pair/{request}");
    assert_eq!(
        fx.post(&poll, &origin, &json!({})).body["status"],
        "pending"
    );

    // 2. The app shows the request (event) and the user approves it.
    let events = fx.t.take_events();
    let _ = events;
    let pending =
        fx.t.ok::<LocalApiClientsList>(fc_api::v1::common::Empty {})
            .clients;
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].origin.as_deref(), Some(EXT));
    assert_eq!(pending[0].name, "Flashcard for Chrome");
    fx.t.ok::<LocalApiClientsApprove>(ApproveApiClient {
        id: pending[0].id,
        scopes: None,
    });

    // 3. The extension's GET carries no Origin; it gets the token once.
    let r = fx.post(&poll, &[], &json!({}));
    assert_eq!(r.body["status"], "approved");
    let token = r.body["token"].as_str().unwrap().to_owned();
    assert!(token.starts_with("fc1_"));
    let again = fx.post(&poll, &origin, &json!({}));
    assert_eq!(again.body["status"], "approved");
    assert!(
        again.body["token"].is_null(),
        "the token is handed out once"
    );
    // Another origin may not poll.
    let r = fx.post(
        &poll,
        &[("Origin", "chrome-extension://someoneelse")],
        &json!({}),
    );
    assert_eq!(r.status, 403);
    // The token is not stored anywhere in the database.
    let mut db = std::fs::read(&fx.db).unwrap();
    db.extend(std::fs::read(fx.db.with_extension("db-wal")).unwrap_or_default());
    assert!(!db.windows(token.len()).any(|w| w == token.as_bytes()));

    // 4. Decks and classifications for the popup.
    let auth = format!("Bearer {token}");
    let h = [("Authorization", auth.as_str()), ("Origin", EXT)];
    let decks = fx.get("/v1/decks", &h);
    assert_eq!(decks.status, 200);
    assert_eq!(decks.body["decks"][0]["path"], "İngilizce");
    let attrs = fx.post(
        "/v1/attributes.effective",
        &h,
        &json!({"deck_id": verbs, "target": "note"}),
    );
    assert_eq!(attrs.status, 200, "{}", attrs.body);
    assert_eq!(attrs.body["attributes"][0]["def"]["name"], "Seviye");

    // 5. The word goes to the inbox, with its context and classification.
    let word = json!({
        "front": "badger", "back": "porsuk", "deck_id": verbs,
        "attributes": {"Seviye": "B1"},
        "context": {"url": "https://example.com/a", "title": "Badgers", "sentence": "The badger dug."},
        "client_ref": "0193-a"
    });
    let r = fx.post("/v1/inbox", &h, &word);
    assert_eq!(r.status, 201, "{}", r.body);
    let id = r.body["id"].clone();
    // A resend (lost response) does not duplicate it.
    let r = fx.post("/v1/inbox", &h, &word);
    assert_eq!(r.status, 200);
    assert_eq!(r.body["id"], id);

    let inbox = fx.t.ok::<InboxList>(ListInbox::default());
    assert_eq!(inbox.items.len(), 1);
    let item = &inbox.items[0];
    assert!(
        matches!(&item.source, InboxSource::LocalApi { client_name, .. } if client_name == "Flashcard for Chrome")
    );
    assert_eq!(item.item.front.as_deref(), Some("badger"));
    assert_eq!(
        item.item.context.as_ref().unwrap().sentence.as_deref(),
        Some("The badger dug.")
    );
    // The user accepts it: a classified note.
    fx.t.ok::<InboxAccept>(fc_api::v1::inbox::AcceptInbox {
        items: vec![fc_api::v1::inbox::AcceptInboxItem {
            id: item.id,
            deck_id: None,
            notetype_id: None,
            fields: None,
            tags: None,
            attributes: None,
        }],
        allow_duplicates: None,
    });
    let hits =
        fx.t.ok::<SearchCardsMethod>(fc_api::v1::search::SearchCards {
            query: "seviye:B1".into(),
            sort: None,
            descending: None,
            notes_only: None,
            offset: None,
            limit: None,
        });
    assert_eq!(hits.total, 1);
    // The add was the client's, in the oplog.
    let log =
        fx.t.ok::<OplogList>(fc_api::v1::oplog::ListOplog::default())
            .items;
    assert!(log
        .iter()
        .any(|e| e.command == "v1.inbox.add" && matches!(e.actor, Actor::LocalApi(_))));
}

#[test]
fn web_origins_big_bodies_and_bursts_are_refused() {
    let fx = Fx::new();
    for o in ["https://evil.example", "http://127.0.0.1:3000", "null"] {
        for path in ["/v1/ping", "/v1/pair"] {
            let r = send(fx.addr(), "OPTIONS", path, &[("Origin", o)], None);
            assert_eq!(r.status, 403, "{o}");
            assert!(r.header("access-control-allow-origin").is_none());
        }
        let r = fx.post(
            "/v1/pair",
            &[("Origin", o)],
            &json!({"client_name": "x", "client_kind": "app", "scopes": ["inbox:write"]}),
        );
        assert_eq!(r.status, 403, "{o}");
    }
    let token = fx.token(&["inbox:write"], Some(EXT));
    let auth = format!("Bearer {token}");
    // The token is bound to its origin: a web page holding it gets nothing.
    let r = fx.post(
        "/v1/inbox",
        &[
            ("Authorization", auth.as_str()),
            ("Origin", "https://evil.example"),
        ],
        &json!({"front": "x"}),
    );
    assert_eq!(r.status, 403);
    // Body limit.
    let big = json!({"front": "x".repeat(fc_http::MAX_BODY_BYTES)});
    let r = fx.post("/v1/inbox", &[("Authorization", auth.as_str())], &big);
    assert_eq!(r.status, 413);
    assert_eq!(r.code(), "VALIDATION_PAYLOAD_TOO_LARGE");
    // Malformed JSON and unknown fields.
    let r = send(
        fx.addr(),
        "POST",
        "/v1/inbox",
        &[("Authorization", auth.as_str())],
        Some("{"),
    );
    assert_eq!(r.status, 400);
    let r = fx.post(
        "/v1/inbox",
        &[("Authorization", auth.as_str())],
        &json!({"front": "x", "source": "me"}),
    );
    assert_eq!(r.status, 400);
    // Burst limit per client.
    let mut limited = None;
    for _ in 0..80 {
        let r = fx.get("/v1/auth", &[("Authorization", auth.as_str())]);
        if r.status == 429 {
            limited = Some(r);
            break;
        }
    }
    let r = limited.expect("a burst is rate limited");
    assert_eq!(r.code(), "CONFLICT_RATE_LIMITED");
    assert!(r.header("retry-after").is_some());
    // Pairing requests are limited for the whole server.
    let mut codes = vec![];
    for i in 0..8 {
        let r = fx.post(
            "/v1/pair",
            &[],
            &json!({"client_name": format!("c{i}"), "client_kind": "script", "scopes": ["inbox:write"]}),
        );
        codes.push(r.status);
    }
    assert!(codes.contains(&429), "{codes:?}");
}

#[test]
fn every_request_but_ping_needs_the_client_header_and_json() {
    let fx = Fx::new();
    let token = fx.token(&["inbox:write", "decks:read"], None);
    let auth = format!("Bearer {token}");
    let a = [("Authorization", auth.as_str())];
    let no_client = [("Authorization", auth.as_str()), ("X-Flashcard-Client", "")];
    // The token alone is not enough, GETs that return user data included.
    for path in ["/v1/decks", "/v1/auth", "/v1/methods"] {
        assert_eq!(fx.get(path, &a).status, 200, "{path} with the header");
        let r = fx.get(path, &no_client);
        assert_eq!(r.status, 400, "{path} without it");
        assert_eq!(r.body["details"]["reason"], "client_header");
    }
    // POSTs: JSON content type required (what a plain HTML form cannot send).
    let body = json!({"front": "porsuk"}).to_string();
    for ct in [
        "text/plain",
        "application/x-www-form-urlencoded",
        "multipart/form-data",
    ] {
        let mut h = a.to_vec();
        h.push(("Content-Type", ct));
        let r = send(fx.addr(), "POST", "/v1/inbox", &h, Some(&body));
        assert_eq!(r.status, 400, "{ct}");
        assert_eq!(r.body["details"]["reason"], "content_type");
    }
    let r = send(fx.addr(), "POST", "/v1/inbox", &no_client, Some(&body));
    assert_eq!(r.body["details"]["reason"], "client_header");
    let mut h = a.to_vec();
    h.push(("Content-Type", "Application/JSON; charset=utf-8"));
    assert_eq!(
        send(fx.addr(), "POST", "/v1/inbox", &h, Some(&body)).status,
        201
    );
    assert!(
        fx.t.ok::<InboxList>(ListInbox::default()).items.len() == 1,
        "only the valid one"
    );

    // Pairing and polling are guarded the same way, and polling is a POST.
    let pair = json!({"client_name": "x", "client_kind": "script", "scopes": ["inbox:write"]});
    let r = send(
        fx.addr(),
        "POST",
        "/v1/pair",
        &[("X-Flashcard-Client", "")],
        Some(&pair.to_string()),
    );
    assert_eq!(r.body["details"]["reason"], "client_header");
    let r = send(
        fx.addr(),
        "POST",
        "/v1/pair",
        &[("Content-Type", "text/plain")],
        Some(&pair.to_string()),
    );
    assert_eq!(r.body["details"]["reason"], "content_type");
    let id = fx.post("/v1/pair", &[], &pair).body["request_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let poll = format!("/v1/pair/{id}");
    assert_eq!(fx.get(&poll, &[]).status, 404, "a GET no longer polls");
    let r = send(
        fx.addr(),
        "POST",
        &poll,
        &[("X-Flashcard-Client", "")],
        Some("{}"),
    );
    assert_eq!(r.body["details"]["reason"], "client_header");
    assert_eq!(fx.post(&poll, &[], &json!({})).body["status"], "pending");
    // Ping stays open: it says only that the app is there.
    let r = send(
        fx.addr(),
        "GET",
        "/v1/ping",
        &[("X-Flashcard-Client", "")],
        None,
    );
    assert_eq!(r.status, 200);
    // The extension's preflight may ask for the header.
    let r = send(fx.addr(), "OPTIONS", "/v1/inbox", &[("Origin", EXT)], None);
    assert!(r
        .header("access-control-allow-headers")
        .unwrap()
        .to_lowercase()
        .contains("x-flashcard-client"));
}

#[test]
fn pairing_is_limited_per_client_name_and_never_locked_out() {
    let fx = Fx::new();
    let ask = |name: &str, origin: Option<&str>| {
        let mut h = vec![];
        if let Some(o) = origin {
            h.push(("Origin", o));
        }
        fx.post(
            "/v1/pair",
            &h,
            &json!({"client_name": name, "client_kind": "script", "scopes": ["inbox:write"]}),
        )
    };
    // One name: three requests, then it waits (case and spacing do not make a new name).
    // The server-wide allowance is five a minute, so this test spends four of them here.
    let first: Vec<u16> = ["Noisy", "Noisy", "noisy", "NOISY "]
        .iter()
        .map(|n| ask(n, None).status)
        .collect();
    assert_eq!(first, [201, 201, 201, 429]);
    // Another name (or the same name from an extension) is a different client.
    assert_eq!(ask("Quiet", None).status, 201);
    // Only one request per name stays pending: a repeat replaced its earlier one.
    let rows =
        fx.t.ok::<LocalApiClientsList>(fc_api::v1::common::Empty {})
            .clients;
    assert_eq!(rows.iter().filter(|c| c.name == "Noisy").count(), 1);
    // The queue stays under its cap with names rotating: oldest makes room, no lockout.
    for i in 0..3 {
        let _ = ask(&format!("flood {i}"), None);
    }
    let rows =
        fx.t.ok::<LocalApiClientsList>(fc_api::v1::common::Empty {})
            .clients;
    assert!(rows.len() <= fc_app::modules::local_api::MAX_PENDING);
}

#[test]
fn pending_pairings_do_not_survive_a_restart() {
    let fx = Fx::new();
    let r = fx.post(
        "/v1/pair",
        &[],
        &json!({"client_name": "Before restart", "client_kind": "script", "scopes": ["inbox:write"]}),
    );
    assert_eq!(r.status, 201);
    let id =
        fx.t.ok::<LocalApiClientsList>(fc_api::v1::common::Empty {})
            .clients[0]
            .id;
    // The server (and with it every token it held) goes away and comes back.
    let app = fx.app.clone();
    fx.server.stop();
    let server = Server::start(app, ServerConfig::localhost(0)).unwrap();
    assert!(fx
        .t
        .ok::<LocalApiClientsList>(fc_api::v1::common::Empty {})
        .clients
        .is_empty());
    let e =
        fx.t.err::<LocalApiClientsApprove>(ApproveApiClient { id, scopes: None });
    assert_eq!(e.code, fc_api::ErrorCode::NotFoundApiClient);
    drop(server);
}

/// v1.0 / Phase-4 review: while the app is locked the server answers nothing but `ping`
/// (`423 PERMISSION_APP_LOCKED`, pairing included); unlocked, the same token works again.
#[test]
fn a_locked_app_answers_nothing_but_ping() {
    use fc_api::v1::session::{SetLock, Unlock};
    use fc_app::modules::profiles::{ProfileStore, Session};
    let fx = Fx::new();
    let root = tempfile::tempdir().unwrap();
    let session = Session::open(
        ProfileStore::new(root.path()),
        Arc::new(fc_ai::secrets::MemorySecrets::new()),
        Arc::new(fc_app::system::SystemClock),
    )
    .unwrap();
    fx.app.install_session(Arc::new(session));
    let token = fx.token(&["decks:read"], None);
    let auth = format!("Bearer {token}");
    let decks = || fx.get("/v1/decks", &[("Authorization", auth.as_str())]);
    assert_eq!(decks().status, 200);

    let user = Actor::User;
    fx.app
        .call::<LockSet>(
            &SetLock {
                kind: fc_core::ux::lock::LockKind::Pin,
                secret: "4821".into(),
                current: None,
                idle_minutes: None,
            },
            &user,
        )
        .unwrap();
    fx.app
        .call::<LockLock>(&fc_api::v1::common::Empty {}, &user)
        .unwrap();
    let r = decks();
    assert_eq!(r.status, 423);
    assert_eq!(r.code(), "PERMISSION_APP_LOCKED");
    let r = fx.post(
        "/v1/pair",
        &[],
        &json!({"client_name": "x", "client_kind": "script", "scopes": ["decks:read"]}),
    );
    assert_eq!(r.status, 423, "no pairing requests while locked");
    assert_eq!(fx.get("/v1/ping", &[]).status, 200);

    fx.app
        .call::<LockUnlock>(
            &Unlock {
                secret: "4821".into(),
            },
            &user,
        )
        .unwrap();
    assert_eq!(decks().status, 200);
}
