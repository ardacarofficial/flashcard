//! [`MemTx`] repository traits: external MCP server registrations (v0.8).

use super::*;
use fc_core::McpServerId;
use fc_storage::repo::McpServerRecord;

/// `mcp_servers`.
#[derive(Debug, Clone, Default)]
pub struct McpState {
    servers: BTreeMap<McpServerId, McpServerRecord>,
}

impl McpRead for MemTx<'_> {
    fn list_mcp_servers(&self) -> StorageResult<Vec<McpServerRecord>> {
        let mut v: Vec<McpServerRecord> =
            self.s().domains.mcp.servers.values().cloned().collect();
        v.sort_by(|a, b| {
            a.name
                .to_lowercase()
                .cmp(&b.name.to_lowercase())
                .then_with(|| a.id.cmp(&b.id))
        });
        Ok(v)
    }

    fn get_mcp_server(&self, id: McpServerId) -> StorageResult<Option<McpServerRecord>> {
        Ok(self.s().domains.mcp.servers.get(&id).cloned())
    }
}

impl McpWrite for MemTx<'_> {
    fn upsert_mcp_server(&mut self, server: &McpServerRecord) -> StorageResult<()> {
        let clash = self.s().domains.mcp.servers.values().any(|s| {
            s.id != server.id && s.name.eq_ignore_ascii_case(&server.name)
        });
        if clash {
            return Err(StorageError::Constraint {
                constraint: "mcp_servers_name".into(),
            });
        }
        self.m()
            .domains
            .mcp
            .servers
            .insert(server.id, server.clone());
        Ok(())
    }

    fn delete_mcp_server(&mut self, id: McpServerId) -> StorageResult<bool> {
        Ok(self.m().domains.mcp.servers.remove(&id).is_some())
    }
}
