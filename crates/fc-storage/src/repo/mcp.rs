//! External MCP server registrations (v0.8, migration `0750_mcp_servers`).

use super::*;
use fc_core::McpServerId;

/// One `mcp_servers` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct McpServerRecord {
    /// Id.
    pub id: McpServerId,
    /// Display name (unique, case-insensitive: the app checks, the index enforces ASCII case).
    pub name: String,
    /// `stdio` or `http`.
    pub transport: String,
    /// Program (stdio).
    pub command: String,
    /// Program arguments (stdio).
    pub args: Vec<String>,
    /// Endpoint (http).
    pub url: Option<String>,
    /// Enabled.
    pub enabled: bool,
    /// Creation time.
    pub created_at: Timestamp,
    /// Last modification time.
    pub updated_at: Timestamp,
}

/// MCP reads.
pub trait McpRead {
    /// Every registered server, by name (case-insensitive), then id.
    fn list_mcp_servers(&self) -> StorageResult<Vec<McpServerRecord>>;
    /// One server.
    fn get_mcp_server(&self, id: McpServerId) -> StorageResult<Option<McpServerRecord>>;
}

/// MCP writes.
pub trait McpWrite: McpRead {
    /// Inserts or replaces a server row (all columns).
    fn upsert_mcp_server(&mut self, server: &McpServerRecord) -> StorageResult<()>;
    /// Deletes a server row. Returns whether it existed.
    fn delete_mcp_server(&mut self, id: McpServerId) -> StorageResult<bool>;
}
