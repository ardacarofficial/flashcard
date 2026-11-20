//! MCP client skeleton (docs/06 §2.2): the app connects to an **external** MCP server
//! (registered with `mcp.servers.add`) so AI features can use its tools.
//!
//! Scope of v0.8: stdio servers (launch `command args…`), list tools, call a tool, and the
//! confirmation policy ([`ExternalTool::needs_confirmation`]): every call is shown to the
//! user, and tools that are not declared read-only (or are declared destructive) need an
//! explicit approval before AI features may run them. Streamable HTTP is stored in the
//! registration but not connected yet.

use crate::McpError;
use fc_api::v1::mcp::{McpServerDto, McpTransport};
use rmcp::model::{CallToolRequestParams, ClientCapabilities, ClientConfig, Implementation};
use rmcp::service::RunningService;
use rmcp::transport::TokioChildProcess;
use rmcp::{RoleClient, ServiceExt};
use serde_json::Value;

/// Client name the app announces to external servers.
pub const CLIENT_NAME: &str = "flashcard";

/// A tool offered by an external server.
#[derive(Debug, Clone, PartialEq)]
pub struct ExternalTool {
    /// Tool name.
    pub name: String,
    /// Description for the model.
    pub description: Option<String>,
    /// JSON Schema of the arguments.
    pub input_schema: Value,
    /// `readOnlyHint` (absent = `false`, per the MCP spec).
    pub read_only: bool,
    /// `destructiveHint` (absent = `true` for non-read-only tools, per the MCP spec).
    pub destructive: bool,
}

impl ExternalTool {
    /// Whether the user must approve a call before it runs. Hints come from an external,
    /// untrusted server, so they can only make the policy stricter: anything not explicitly
    /// read-only needs approval.
    pub fn needs_confirmation(&self) -> bool {
        !self.read_only || self.destructive
    }
}

/// Result of calling an external tool.
#[derive(Debug, Clone, PartialEq)]
pub struct ExternalToolResult {
    /// The tool reported an error.
    pub is_error: bool,
    /// Concatenated text content.
    pub text: String,
    /// Structured content, if any.
    pub structured: Option<Value>,
}

/// A live connection to one external MCP server.
pub struct McpClient {
    service: RunningService<RoleClient, ClientConfig>,
}

impl std::fmt::Debug for McpClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("McpClient").finish_non_exhaustive()
    }
}

fn client_config() -> ClientConfig {
    ClientConfig::new(
        ClientCapabilities::default(),
        Implementation::new(CLIENT_NAME, env!("CARGO_PKG_VERSION")),
    )
}

impl McpClient {
    /// Launches `command args…` and completes the MCP handshake over its stdio.
    pub async fn connect_stdio(command: &str, args: &[String]) -> Result<Self, McpError> {
        let mut cmd = tokio::process::Command::new(command);
        cmd.args(args);
        let transport = TokioChildProcess::new(cmd)?;
        Self::connect(transport).await
    }

    /// Connects to a registered server (stdio only for now). Refuses a disabled registration
    /// and one the user never approved (`McpServerDto::approved`: a row that did not come
    /// from `mcp.servers.add`/`update` — e.g. from a restored backup — must not launch
    /// programs).
    pub async fn connect_registered(server: &McpServerDto) -> Result<Self, McpError> {
        if !server.enabled {
            return Err(McpError::NotAllowed(format!("{} is disabled", server.name)));
        }
        if !server.approved {
            return Err(McpError::NotAllowed(format!(
                "{} was not approved by the user (re-add or save it in the MCP screen)",
                server.name
            )));
        }
        match server.transport {
            McpTransport::Stdio => Self::connect_stdio(&server.command, &server.args).await,
            McpTransport::Http => Err(McpError::Unsupported(format!(
                "{}: streamable HTTP servers are not connected yet",
                server.name
            ))),
        }
    }

    /// Completes the handshake over any rmcp transport (tests use an in-memory duplex).
    pub async fn connect<T, E, A>(transport: T) -> Result<Self, McpError>
    where
        T: rmcp::transport::IntoTransport<RoleClient, E, A>,
        E: std::error::Error + Send + Sync + 'static,
    {
        let service = client_config()
            .serve(transport)
            .await
            .map_err(|e| McpError::Handshake(e.to_string()))?;
        Ok(Self { service })
    }

    /// `serverInfo.name` from the handshake.
    pub fn server_name(&self) -> Option<String> {
        self.service
            .peer_info()
            .and_then(|i| i.server_info.as_ref().map(|s| s.name.clone()))
    }

    /// Every tool of the server (all pages).
    pub async fn list_tools(&self) -> Result<Vec<ExternalTool>, McpError> {
        let tools = self
            .service
            .list_all_tools()
            .await
            .map_err(|e| McpError::Service(e.to_string()))?;
        Ok(tools
            .into_iter()
            .map(|t| {
                let read_only = t
                    .annotations
                    .as_ref()
                    .and_then(|a| a.read_only_hint)
                    .unwrap_or(false);
                let destructive = t
                    .annotations
                    .as_ref()
                    .and_then(|a| a.destructive_hint)
                    .unwrap_or(!read_only);
                ExternalTool {
                    name: t.name.to_string(),
                    description: t.description.map(|d| d.to_string()),
                    input_schema: Value::Object((*t.input_schema).clone()),
                    read_only,
                    destructive,
                }
            })
            .collect())
    }

    /// Calls a tool with JSON object arguments.
    pub async fn call_tool(&self, name: &str, args: Value) -> Result<ExternalToolResult, McpError> {
        let mut params = CallToolRequestParams::new(name.to_owned());
        if let Value::Object(map) = args {
            params.arguments = Some(map);
        }
        let r = self
            .service
            .call_tool(params)
            .await
            .map_err(|e| McpError::Service(e.to_string()))?;
        let text = r
            .content
            .iter()
            .filter_map(|c| c.as_text().map(|t| t.text.clone()))
            .collect::<Vec<_>>()
            .join("\n");
        Ok(ExternalToolResult {
            is_error: r.is_error.unwrap_or(false),
            text,
            structured: r.structured_content,
        })
    }

    /// Closes the connection (and stops a launched process).
    pub async fn close(self) -> Result<(), McpError> {
        self.service
            .cancel()
            .await
            .map(|_| ())
            .map_err(|e| McpError::Join(e.to_string()))
    }
}
