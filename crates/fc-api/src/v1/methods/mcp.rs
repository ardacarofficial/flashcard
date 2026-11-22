//! MCP server tool catalog and external MCP server registrations (v0.8).

use crate::v1::common::Empty;
use crate::v1::mcp::*;

methods! {
    // ---- mcp ----
    /// Tools the MCP server exposes (and whether writes are allowed).
    McpToolsList = "mcp.tools.list", Query, "mcp", "mcp:read", Public, undo: false, Empty => McpToolList;
    /// Paths an MCP client configuration needs (collection file, `fc` program, OS).
    McpSetupGet = "mcp.setup", Query, "mcp", "mcp:read", Public, undo: false, Empty => McpSetup;
    /// Registered external MCP servers.
    McpServersList = "mcp.servers.list", Query, "mcp", "mcp:read", Public, undo: false, Empty => McpServerList;
    /// Registers an external MCP server.
    McpServersAdd = "mcp.servers.add", Command, "mcp", "mcp:write", Public, undo: true, AddMcpServer => McpServerDto;
    /// Edits an external MCP server.
    McpServersUpdate = "mcp.servers.update", Command, "mcp", "mcp:write", Public, undo: true, UpdateMcpServer => McpServerDto;
    /// Removes an external MCP server.
    McpServersRemove = "mcp.servers.remove", Command, "mcp", "mcp:write", Public, undo: true, McpServerRef => McpServerDto;
    /// Internal inverse of remove: re-inserts the row unchanged.
    McpServersRestore = "mcp.servers.restore", Command, "mcp", "mcp:write", Internal, undo: true, RestoreMcpServer => McpServerDto;
}
