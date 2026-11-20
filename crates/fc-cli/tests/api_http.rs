//! v0.9 acceptance over the real binaries — "a word added from the browser extension lands in
//! the inbox": `fc api serve` (the same server the desktop runs) on a free loopback port; the
//! extension's calls as raw HTTP from a `chrome-extension://` origin (pair → poll (POST) → token →
//! `POST /v1/inbox`); the user's approval and the check through separate `fc call` processes
//! (`local_api.clients.approve`, `inbox.list`). Also: the server refuses to start while the
//! module is off, and refuses token-less requests.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};

const EXT: &str = "chrome-extension://kfbdpdaobnofkbopebjglnaadopfikhh";

fn fc(db: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fc"))
        .arg("--db")
        .arg(db)
        .args(args)
        .env_remove("FC_DB")
        .output()
        .expect("run fc")
}

fn call(db: &Path, method: &str, params: &Value) -> Value {
    let o = fc(db, &["--json", "call", method, &params.to_string()]);
    assert!(
        o.status.success(),
        "fc call {method} failed: {}",
        String::from_utf8_lossy(&o.stderr)
    );
    serde_json::from_slice(&o.stdout).unwrap()
}

struct Serve(Child, SocketAddr);

impl Drop for Serve {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn serve(db: &Path) -> Serve {
    let mut child = Command::new(env!("CARGO_BIN_EXE_fc"))
        .arg("--db")
        .arg(db)
        .args(["api", "serve", "--port", "0"])
        .env_remove("FC_DB")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut line = String::new();
    BufReader::new(child.stderr.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    let addr = line
        .trim()
        .rsplit("http://")
        .next()
        .unwrap()
        .parse()
        .unwrap_or_else(|_| panic!("no address in {line:?}"));
    Serve(child, addr)
}

fn http(
    addr: SocketAddr,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: Option<&Value>,
) -> (u16, Value) {
    let mut s = TcpStream::connect(addr).unwrap();
    let body = body.map(Value::to_string).unwrap_or_default();
    let mut req = format!(
        "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\nX-Flashcard-Client: acceptance-test\r\nContent-Type: application/json\r\nContent-Length: {}\r\n",
        addr.port(),
        body.len()
    );
    for (k, v) in headers {
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    req.push_str("\r\n");
    req.push_str(&body);
    s.write_all(req.as_bytes()).unwrap();
    let mut raw = String::new();
    s.read_to_string(&mut raw).unwrap();
    let (head, rest) = raw.split_once("\r\n\r\n").unwrap();
    let status = head.split(' ').nth(1).unwrap().parse().unwrap();
    let text = if head
        .to_ascii_lowercase()
        .contains("transfer-encoding: chunked")
    {
        let mut out = String::new();
        let mut r = rest;
        while let Some((len, tail)) = r.split_once("\r\n") {
            let n = usize::from_str_radix(len.trim(), 16).unwrap_or(0);
            if n == 0 {
                break;
            }
            out.push_str(&tail[..n]);
            r = &tail[n + 2..];
        }
        out
    } else {
        rest.to_owned()
    };
    (status, serde_json::from_str(&text).unwrap_or(Value::Null))
}

#[test]
fn extension_word_lands_in_the_inbox_through_fc_api_serve() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("collection.db");
    let deck = call(&db, "decks.create", &json!({"path": "İngilizce"}));

    // Off by default: the server does not start.
    let o = fc(&db, &["api", "serve", "--port", "0"]);
    assert!(!o.status.success());
    assert!(String::from_utf8_lossy(&o.stderr).contains("CONFLICT_MODULE_DISABLED"));
    call(
        &db,
        "modules.set_enabled",
        &json!({"id": "local_api", "enabled": true}),
    );
    let server = serve(&db);
    let addr = server.1;
    assert!(addr.ip().is_loopback());

    let (status, _) = http(addr, "GET", "/v1/decks", &[], None);
    assert_eq!(status, 401, "token-less request refused");

    let origin = [("Origin", EXT)];
    let (status, pair) = http(
        addr,
        "POST",
        "/v1/pair",
        &origin,
        Some(
            &json!({"client_name": "Flashcard for Chrome", "client_kind": "browser_extension",
                     "scopes": ["inbox:write", "decks:read"]}),
        ),
    );
    assert_eq!(status, 201, "{pair}");
    let poll = format!("/v1/pair/{}", pair["request_id"].as_str().unwrap());
    assert_eq!(
        http(addr, "POST", &poll, &origin, Some(&json!({}))).1["status"],
        "pending"
    );

    // The user approves (here headless, in another process).
    let clients = call(&db, "local_api.clients.list", &json!({}));
    let client = &clients["clients"][0];
    assert_eq!(client["status"], "pending");
    assert_eq!(client["origin"], EXT);
    call(
        &db,
        "local_api.clients.approve",
        &json!({"id": client["id"], "scopes": null}),
    );

    let (_, approved) = http(addr, "POST", &poll, &origin, Some(&json!({})));
    assert_eq!(approved["status"], "approved");
    let token = approved["token"].as_str().unwrap().to_owned();
    let auth = format!("Bearer {token}");
    let h = [("Authorization", auth.as_str()), ("Origin", EXT)];

    let (status, decks) = http(addr, "GET", "/v1/decks", &h, None);
    assert_eq!(status, 200);
    assert_eq!(decks["decks"][0]["path"], "İngilizce");
    let (status, item) = http(
        addr,
        "POST",
        "/v1/inbox",
        &h,
        Some(
            &json!({"front": "badger", "back": "porsuk", "deck_id": deck["id"],
                     "context": {"url": "https://example.com", "sentence": "The badger dug."}}),
        ),
    );
    assert_eq!(status, 201, "{item}");

    // The word is in the inbox, as seen by another process.
    let inbox = call(&db, "inbox.list", &json!({}));
    assert_eq!(inbox["pending"], 1);
    let got = &inbox["items"][0];
    assert_eq!(got["item"]["front"], "badger");
    assert_eq!(got["source"]["kind"], "local_api");
    assert_eq!(got["source"]["client_name"], "Flashcard for Chrome");
    assert_eq!(got["item"]["context"]["sentence"], "The badger dug.");
    // Accepted in the app, it is a note in that deck.
    call(&db, "inbox.accept", &json!({"items": [{"id": got["id"]}]}));
    let hits = call(
        &db,
        "search.cards",
        &json!({"query": "deck:İngilizce badger"}),
    );
    assert_eq!(hits["total"], 1);
}
