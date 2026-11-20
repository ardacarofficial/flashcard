//! The MCP server driven the way Claude Desktop / Claude Code drive it: newline-delimited
//! JSON-RPC (`initialize` → `notifications/initialized` → `tools/list` → `tools/call`), here
//! over an in-memory pipe; `fc-cli/tests/mcp_stdio.rs` does the same against the real `fc`
//! binary. The client skeleton is checked against our own server.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use fc_api::v1::methods as m;
use fc_api::v1::oplog::{Actor, ListOplog};
use fc_app::App;
use fc_core::paging::PageRequest;
use fc_mcp::{FlashcardServer, McpClient};
use rmcp::ServiceExt;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::io::{
    AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, Lines, ReadHalf, WriteHalf,
};

struct Fx {
    app: Arc<App>,
    _dir: tempfile::TempDir,
}

fn collection() -> Fx {
    let dir = tempfile::tempdir().unwrap();
    let app = Arc::new(App::open_sqlite(&dir.path().join("collection.db")).unwrap());
    for path in ["İngilizce", "İngilizce::Fiiller"] {
        app.invoke(
            "v1.decks.create",
            json!({ "path": path, "description": null, "icon": null, "color": null }),
            &Actor::User,
        )
        .unwrap();
    }
    Fx { app, _dir: dir }
}

/// A raw JSON-RPC peer, like an MCP host process.
struct Raw {
    lines: Lines<BufReader<ReadHalf<DuplexStream>>>,
    write: WriteHalf<DuplexStream>,
}

impl Raw {
    fn start(app: Arc<App>, read_only: bool) -> Self {
        let (client, server) = tokio::io::duplex(1 << 16);
        tokio::spawn(async move {
            let running = FlashcardServer::new(app, read_only)
                .serve(server)
                .await
                .unwrap();
            let _ = running.waiting().await;
        });
        let (read, write) = tokio::io::split(client);
        Self {
            lines: BufReader::new(read).lines(),
            write,
        }
    }

    async fn send(&mut self, msg: Value) {
        let line = format!("{msg}\n");
        self.write.write_all(line.as_bytes()).await.unwrap();
    }

    /// Sends a request and returns its response (skipping notifications).
    async fn request(&mut self, id: u64, method: &str, params: Value) -> Value {
        self.send(json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }))
            .await;
        loop {
            let line = self
                .lines
                .next_line()
                .await
                .unwrap()
                .expect("server closed");
            let v: Value = serde_json::from_str(&line).unwrap();
            if v["id"] == json!(id) {
                return v;
            }
        }
    }

    async fn initialize(&mut self, client: &str) -> Value {
        let r = self
            .request(
                1,
                "initialize",
                json!({
                    "protocolVersion": "2025-06-18",
                    "capabilities": {},
                    "clientInfo": { "name": client, "version": "1.0.0" }
                }),
            )
            .await;
        self.send(json!({ "jsonrpc": "2.0", "method": "notifications/initialized" }))
            .await;
        r
    }

    async fn call(&mut self, id: u64, name: &str, arguments: Value) -> Value {
        let r = self
            .request(
                id,
                "tools/call",
                json!({ "name": name, "arguments": arguments }),
            )
            .await;
        r["result"].clone()
    }
}

fn names(list: &Value) -> Vec<String> {
    list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_owned())
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn claude_lists_decks_and_adds_a_note_over_json_rpc() {
    let fx = collection();
    let mut peer = Raw::start(fx.app.clone(), false);

    let init = peer.initialize("claude-ai").await;
    assert_eq!(init["result"]["serverInfo"]["name"], "flashcard");
    assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
    assert!(init["result"]["capabilities"]["tools"].is_object());
    assert!(init["result"]["instructions"]
        .as_str()
        .unwrap()
        .contains("list_decks"));

    let list = peer.request(2, "tools/list", json!({})).await;
    let tools = names(&list);
    for t in [
        "list_decks",
        "search_cards",
        "add_notes",
        "set_classification",
    ] {
        assert!(tools.contains(&t.to_owned()), "{t} missing from {tools:?}");
    }
    let add = list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "add_notes")
        .unwrap();
    assert_eq!(add["inputSchema"]["required"], json!(["deck", "notes"]));
    assert_eq!(add["annotations"]["readOnlyHint"], false);
    let ld = list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["name"] == "list_decks")
        .unwrap();
    assert_eq!(ld["annotations"]["readOnlyHint"], true);

    let decks = peer.call(3, "list_decks", json!({})).await;
    assert_eq!(decks["isError"], false);
    assert_eq!(
        decks["structuredContent"]["decks"][1]["path"],
        "İngilizce::Fiiller"
    );
    // The same JSON as text for clients that ignore structured content.
    let text: Value = serde_json::from_str(decks["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(text, decks["structuredContent"]);

    let added = peer
        .call(
            4,
            "add_notes",
            json!({ "deck": "İngilizce::Fiiller",
                    "notes": [{ "fields": { "front": "to go", "back": "gitmek" } }] }),
        )
        .await;
    assert_eq!(added["isError"], false, "{added}");
    assert_eq!(added["structuredContent"]["added"], 1);

    let found = peer
        .call(5, "search_cards", json!({ "query": "gitmek" }))
        .await;
    assert_eq!(found["structuredContent"]["total"], 1);

    // A tool failure is a readable tool error, not a protocol error.
    let bad = peer
        .call(
            6,
            "add_notes",
            json!({ "deck": "Yok", "notes": [{ "fields": { "front": "x" } }] }),
        )
        .await;
    assert_eq!(bad["isError"], true);
    assert_eq!(bad["structuredContent"]["code"], "NOT_FOUND_DECK");
    // An unknown tool is a protocol error.
    let unknown = peer
        .request(
            7,
            "tools/call",
            json!({ "name": "drop_everything", "arguments": {} }),
        )
        .await;
    assert!(unknown["error"].is_object(), "{unknown}");

    // The write is an ordinary command recorded with actor mcp:<clientInfo.name>.
    let log = fx
        .app
        .call::<m::OplogList>(
            &ListOplog {
                page: PageRequest {
                    cursor: None,
                    limit: Some(1),
                },
            },
            &Actor::User,
        )
        .unwrap();
    assert_eq!(log.items[0].command, "v1.notes.create");
    assert_eq!(log.items[0].actor, Actor::Mcp("claude-ai".into()));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn read_only_server_hides_and_refuses_write_tools() {
    let fx = collection();
    let mut peer = Raw::start(fx.app.clone(), true);
    peer.initialize("claude-code").await;
    let tools = names(&peer.request(2, "tools/list", json!({})).await);
    assert!(tools.contains(&"list_decks".to_owned()));
    assert!(!tools.iter().any(|t| t == "add_notes" || t == "create_deck"));
    let r = peer.call(3, "create_deck", json!({ "path": "X" })).await;
    assert_eq!(r["isError"], true);
    assert_eq!(r["structuredContent"]["code"], "PERMISSION_MCP_READ_ONLY");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn client_skeleton_talks_to_our_server() {
    let fx = collection();
    let (client_io, server_io) = tokio::io::duplex(1 << 16);
    let app = fx.app.clone();
    tokio::spawn(async move {
        let running = FlashcardServer::new(app, false)
            .serve(server_io)
            .await
            .unwrap();
        let _ = running.waiting().await;
    });
    let client = McpClient::connect(client_io).await.unwrap();
    assert_eq!(client.server_name().as_deref(), Some("flashcard"));

    let tools = client.list_tools().await.unwrap();
    let get = |n: &str| tools.iter().find(|t| t.name == n).unwrap();
    assert!(!get("list_decks").needs_confirmation());
    assert!(get("add_notes").needs_confirmation());
    assert!(get("set_classification").destructive);

    let r = client.call_tool("list_decks", json!({})).await.unwrap();
    assert!(!r.is_error);
    assert_eq!(r.structured.unwrap()["decks"][0]["path"], "İngilizce");
    let r = client
        .call_tool("get_note", json!({ "note_id": "nope" }))
        .await
        .unwrap();
    assert!(r.is_error);
    assert!(r.text.contains("VALIDATION_INVALID_PARAMS"), "{}", r.text);
    client.close().await.unwrap();
}
