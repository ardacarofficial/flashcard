//! MCP (docs/06 §2, v0.8): the curated tool catalog the MCP server exposes, and the external
//! MCP servers the app may connect to as a client.
//!
//! The server itself is a transport (`fc-mcp`, launched as `fc mcp serve`); the tools it
//! offers are thin wrappers over **public** v1 methods (`McpToolInfo::methods`), called with
//! actor `mcp` so every write is a normal, undoable command in the oplog.

use fc_core::{McpServerId, Timestamp};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Whether an MCP tool only reads or also writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum McpToolAccess {
    /// Read-only (`readOnlyHint`).
    Read,
    /// Writes through undoable commands; hidden when writes are off.
    Write,
}

/// One tool of the MCP server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct McpToolInfo {
    /// Tool name as MCP clients see it (`list_decks`, `add_notes`, …). UI label:
    /// `mcp.tools.<name>`.
    pub name: String,
    /// Read or write.
    pub access: McpToolAccess,
    /// Changes or overwrites existing content (`destructiveHint`). Bulk ones only preview
    /// what they would change until called again with `confirm: true`.
    pub destructive: bool,
    /// Public API methods the tool calls (unqualified names).
    pub methods: Vec<String>,
}

/// Response of `mcp.tools.list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct McpToolList {
    /// Every tool, in catalog order (write tools included even when writes are off).
    pub tools: Vec<McpToolInfo>,
    /// Setting `mcp.server.allow_writes`: when `false` the server lists and runs read tools
    /// only.
    pub writes_allowed: bool,
}

/// Response of `mcp.setup`: what an MCP client's configuration needs to start this
/// collection's server (`<fc_path> --db <db_path> mcp serve`), so the UI can show a snippet
/// with real paths instead of placeholders.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct McpSetup {
    /// The open collection file (`<profile folder>/collection.db`) when the desktop app
    /// started this process; `null` when unknown (CLI, tests).
    pub db_path: Option<String>,
    /// The `fc` command-line program: next to the running executable (the Windows installer
    /// ships it there, v1.3), else the first one on `PATH`; `null` when neither exists (a
    /// development build: the UI then says so and asks the user for it).
    pub fc_path: Option<String>,
    /// Operating system the app runs on (`windows`, `macos`, `linux`, …), for per-OS hints.
    pub platform: String,
}

/// How the app talks to an external MCP server.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum McpTransport {
    /// Launches `command args…` and speaks JSON-RPC over its stdin/stdout.
    Stdio,
    /// Streamable HTTP at `url` (stored; connecting is not implemented yet).
    Http,
}

/// A registered external MCP server.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct McpServerDto {
    /// Id.
    pub id: McpServerId,
    /// Display name (unique, case-insensitive).
    pub name: String,
    /// Transport.
    pub transport: McpTransport,
    /// Program to launch (`stdio`); empty for `http`.
    pub command: String,
    /// Program arguments (`stdio`).
    pub args: Vec<String>,
    /// Endpoint (`http`); `null` for `stdio`.
    pub url: Option<String>,
    /// Whether AI features may use it.
    pub enabled: bool,
    /// The user approved exactly this launch command (set by `mcp.servers.add`/`update`;
    /// `false` for a row that came from anywhere else, e.g. a backup). Unapproved servers are
    /// never launched.
    pub approved: bool,
    /// Creation time.
    pub created_at: Timestamp,
    /// Last modification time.
    pub updated_at: Timestamp,
}

/// Response of `mcp.servers.list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct McpServerList {
    /// Servers by name.
    pub servers: Vec<McpServerDto>,
}

/// `mcp.servers.add`. Environment variables are deliberately not stored (secrets belong in the
/// OS keychain, docs/06 §1.2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct AddMcpServer {
    /// Display name.
    pub name: String,
    /// Transport.
    pub transport: McpTransport,
    /// Program (`stdio`, required there).
    pub command: Option<String>,
    /// Arguments (`stdio`).
    #[serde(default)]
    pub args: Vec<String>,
    /// Endpoint (`http`, required there; `http://` or `https://`).
    pub url: Option<String>,
    /// Default `true`.
    pub enabled: Option<bool>,
}

/// `mcp.servers.update`: replaces every editable field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct UpdateMcpServer {
    /// Id.
    pub id: McpServerId,
    /// Display name.
    pub name: String,
    /// Transport.
    pub transport: McpTransport,
    /// Program (`stdio`).
    pub command: String,
    /// Arguments (`stdio`).
    pub args: Vec<String>,
    /// Endpoint (`http`).
    pub url: Option<String>,
    /// Enabled.
    pub enabled: bool,
}

/// A server by id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct McpServerRef {
    /// Id.
    pub id: McpServerId,
}

/// Internal inverse of `mcp.servers.remove`: puts the exact row back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct RestoreMcpServer {
    /// The removed row.
    pub server: McpServerDto,
}
