//! # fc-mcp
//!
//! MCP transport for Flashcard (docs/06 §2), on the official Rust SDK (`rmcp`).
//!
//! * [`server`]: an MCP server over stdio (`fc mcp serve --db <collection.db>`). It lists the
//!   curated tool catalog of `fc_app::modules::mcp::tools` and forwards every `tools/call`
//!   there; tools call public v1 methods as actor `mcp:<client name>`, so writes are normal,
//!   undoable commands. No business logic lives here.
//! * [`client`]: a skeleton for connecting to external MCP servers (registered with
//!   `mcp.servers.*`) so AI features can use their tools: connect over stdio, list tools, call
//!   one, and decide which calls need the user's confirmation.
#![forbid(unsafe_code)]

pub mod client;
pub mod server;

pub use client::{ExternalTool, McpClient};
pub use server::{serve_stdio, FlashcardServer};

/// Errors of the MCP transport (protocol/IO level; tool failures are tool results).
#[derive(Debug, thiserror::Error)]
pub enum McpError {
    /// The peer did not complete the MCP handshake.
    #[error("MCP handshake failed: {0}")]
    Handshake(String),
    /// A request failed at the protocol level.
    #[error("MCP request failed: {0}")]
    Service(String),
    /// Launching or talking to a process failed.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// The server task stopped abnormally.
    #[error("MCP server task failed: {0}")]
    Join(String),
    /// The external server is not usable over the requested transport.
    #[error("unsupported MCP transport: {0}")]
    Unsupported(String),
    /// The registration may not be launched (disabled, or the user never approved it).
    #[error("MCP server not allowed to run: {0}")]
    NotAllowed(String),
}
