//! v0.8 acceptance — "Claude Desktop/Claude Code can list decks and add cards through the MCP
//! server": launches the real `fc --db <file> mcp serve` exactly as their configuration does
//! (docs/06 §2.1) and speaks newline-delimited JSON-RPC over its stdin/stdout: `initialize`,
//! `notifications/initialized`, `tools/list`, `tools/call list_decks`, `tools/call add_notes`
//! (with a classification), `tools/call search_cards seviye>=B1`. Then, in separate `fc`
//! processes: the note is in the collection, the oplog actor is the MCP client and `fc undo`
//! removes it. A second session with `--read-only` cannot write. Finally `fc mcp tools` (the
//! client skeleton) connects to `fc mcp serve` as an external server.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Output, Stdio};

fn fc(db: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_fc"))
        .arg("--db")
        .arg(db)
        .args(args)
        .env_remove("FC_DB")
        .output()
        .expect("run fc")
}

fn ok(db: &Path, args: &[&str]) -> String {
    let o = fc(db, args);
    assert!(
        o.status.success(),
        "fc {args:?} failed: {}",
        String::from_utf8_lossy(&o.stderr)
    );
    String::from_utf8(o.stdout).unwrap()
}

fn json_out(db: &Path, args: &[&str]) -> Value {
    let mut all = vec!["--json"];
    all.extend_from_slice(args);
    serde_json::from_str(&ok(db, &all)).unwrap()
}

/// `fc mcp serve` as an MCP host launches it.
struct Server {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl Server {
    fn launch(db: &Path, extra: &[&str]) -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_fc"))
            .arg("--db")
            .arg(db)
            .args(["mcp", "serve"])
            .args(extra)
            .env_remove("FC_DB")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("launch fc mcp serve");
        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        Self {
            child,
            stdin,
            stdout,
        }
    }

    fn send(&mut self, msg: &Value) {
        writeln!(self.stdin, "{msg}").unwrap();
        self.stdin.flush().unwrap();
    }

    fn request(&mut self, id: u64, method: &str, params: Value) -> Value {
        self.send(&json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }));
        loop {
            let mut line = String::new();
            assert!(
                self.stdout.read_line(&mut line).unwrap() > 0,
                "server exited"
            );
            let v: Value = serde_json::from_str(&line)
                .unwrap_or_else(|e| panic!("stdout must carry only JSON-RPC ({e}): {line}"));
            if v["id"] == json!(id) {
                return v;
            }
        }
    }

    fn initialize(&mut self, client: &str) -> Value {
        let r = self.request(
            1,
            "initialize",
            json!({
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": { "name": client, "version": "0.0.1" }
            }),
        );
        self.send(&json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }));
        r
    }

    fn call(&mut self, id: u64, tool: &str, arguments: Value) -> Value {
        self.request(
            id,
            "tools/call",
            json!({ "name": tool, "arguments": arguments }),
        )["result"]
            .clone()
    }

    /// Closing stdin ends the session; the process must exit cleanly.
    fn close(self) {
        let Self {
            mut child, stdin, ..
        } = self;
        drop(stdin);
        let status = child.wait().unwrap();
        assert!(status.success(), "fc mcp serve exited with {status}");
    }
}

fn tool_names(list: &Value) -> Vec<String> {
    list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn claude_lists_decks_and_adds_cards_over_stdio() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("collection.db");
    ok(&db, &["deck", "create", "İngilizce::Fiiller"]);
    let mut define = vec![
        "attribute",
        "define",
        "--name",
        "Seviye",
        "--kind",
        "ordered-enum",
        "--deck",
        "İngilizce",
    ];
    for l in ["A1", "A2", "B1", "B2", "C1", "C2"] {
        define.extend_from_slice(&["--option", l]);
    }
    ok(&db, &define);

    let mut s = Server::launch(&db, &[]);
    let init = s.initialize("claude-ai");
    assert_eq!(init["result"]["serverInfo"]["name"], "flashcard");
    assert!(init["result"]["capabilities"]["tools"].is_object());

    let tools = tool_names(&s.request(2, "tools/list", json!({})));
    assert!(
        tools.contains(&"list_decks".into()) && tools.contains(&"add_notes".into()),
        "{tools:?}"
    );

    let decks = s.call(3, "list_decks", json!({}));
    assert_eq!(decks["isError"], false, "{decks}");
    let paths: Vec<&str> = decks["structuredContent"]["decks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["path"].as_str().unwrap())
        .collect();
    assert_eq!(paths, ["İngilizce", "İngilizce::Fiiller"]);

    let cls = s.call(
        4,
        "list_classifications",
        json!({ "deck": "İngilizce::Fiiller" }),
    );
    assert_eq!(
        cls["structuredContent"]["classifications"][0]["name"],
        "Seviye"
    );

    let added = s.call(
        5,
        "add_notes",
        json!({
            "deck": "İngilizce::Fiiller",
            "notes": [
                { "fields": { "front": "to go", "back": "gitmek" },
                  "tags": ["mcp"], "classifications": { "Seviye": "B1" } },
                { "fields": { "front": "to be", "back": "olmak" },
                  "classifications": { "Seviye": "A1" } }
            ]
        }),
    );
    assert_eq!(added["isError"], false, "{added}");
    assert_eq!(added["structuredContent"]["added"], 2);

    let found = s.call(6, "search_cards", json!({ "query": "seviye>=B1" }));
    assert_eq!(found["structuredContent"]["total"], 1);
    assert_eq!(found["structuredContent"]["cards"][0]["text"], "to go");
    s.close();

    // Another process sees the cards; the oplog names the MCP client; undo is one step.
    let notes = json_out(&db, &["note", "list"]);
    assert_eq!(notes["items"].as_array().unwrap().len(), 2);
    let log = json_out(&db, &["call", "oplog.list", r#"{"page":{"limit":2}}"#]);
    for e in log["items"].as_array().unwrap() {
        assert_eq!(e["command"], "v1.notes.create");
        assert_eq!(e["actor"], json!({ "kind": "mcp", "id": "claude-ai" }));
    }
    ok(&db, &["undo"]);
    assert!(json_out(&db, &["note", "list"])["items"]
        .as_array()
        .unwrap()
        .is_empty());

    // A read-only session lists no write tools and refuses them.
    let mut ro = Server::launch(&db, &["--read-only"]);
    ro.initialize("claude-code");
    let tools = tool_names(&ro.request(2, "tools/list", json!({})));
    assert!(!tools.contains(&"add_notes".into()), "{tools:?}");
    let r = ro.call(3, "create_deck", json!({ "path": "X" }));
    assert_eq!(r["isError"], true);
    assert_eq!(r["structuredContent"]["code"], "PERMISSION_MCP_READ_ONLY");
    ro.close();
}

#[test]
fn serve_refuses_a_missing_collection_without_touching_stdout() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("typo.db");
    let o = fc(&db, &["mcp", "serve"]);
    assert!(!o.status.success());
    assert!(o.stdout.is_empty());
    assert!(String::from_utf8_lossy(&o.stderr).contains("no collection"));
    assert!(!db.exists(), "serve must not create a stray database");
}

#[test]
fn mcp_tools_connects_to_an_external_server() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("collection.db");
    let other = dir.path().join("other.db");
    ok(&db, &["deck", "list"]);
    ok(&other, &["deck", "list"]);
    // Register `fc --db other.db mcp serve --read-only` as an external stdio server.
    let exe = env!("CARGO_BIN_EXE_fc");
    let add = json!({
        "name": "Other collection",
        "transport": "stdio",
        "command": exe,
        "args": ["--db", other.to_str().unwrap(), "mcp", "serve", "--read-only"],
    });
    ok(&db, &["call", "mcp.servers.add", &add.to_string()]);

    let tools = json_out(&db, &["mcp", "tools", "other collection"]);
    let names: Vec<&str> = tools
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(
        names.contains(&"list_decks") && !names.contains(&"add_notes"),
        "{names:?}"
    );
    assert!(tools
        .as_array()
        .unwrap()
        .iter()
        .all(|t| t["read_only"] == true));

    // Ad-hoc command after `--`; write tools are flagged for confirmation.
    let text = ok(
        &db,
        &[
            "mcp",
            "tools",
            "--",
            exe,
            "--db",
            other.to_str().unwrap(),
            "mcp",
            "serve",
        ],
    );
    assert!(text.lines().any(|l| l == "! add_notes"), "{text}");
    assert!(text.lines().any(|l| l == "  list_decks"), "{text}");
}
