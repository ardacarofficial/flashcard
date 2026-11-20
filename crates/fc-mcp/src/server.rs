//! The MCP server: `initialize` → `tools/list` → `tools/call`, over any rmcp transport
//! (stdio in production, an in-memory duplex in tests).
//!
//! Tool results carry the JSON answer both as `structuredContent` and as text. Failures of a
//! tool (unknown deck, read-only, invalid arguments…) are **tool errors** (`isError: true`)
//! whose JSON is the app's [`fc_api::ApiError`] (`code`, `message`, `details`), so the model
//! can read and correct them; only an unknown tool name is a protocol error.

use crate::McpError;
use fc_api::ApiError;
use fc_app::modules::mcp::tools::{self, ToolSpec};
use fc_app::App;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, Implementation, ListToolsResult,
    PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool, ToolAnnotations,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData, RoleServer, ServerHandler, ServiceExt};
use serde_json::Value;
use std::sync::Arc;

/// Server name in `initialize` (`serverInfo.name`).
pub const SERVER_NAME: &str = "flashcard";

/// The MCP server over one open collection.
#[derive(Clone)]
pub struct FlashcardServer {
    app: Arc<App>,
    read_only: bool,
}

impl std::fmt::Debug for FlashcardServer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FlashcardServer")
            .field("read_only", &self.read_only)
            .finish_non_exhaustive()
    }
}

/// MCP tool definition of a catalog entry.
pub fn tool_definition(spec: &ToolSpec) -> Tool {
    let schema = match (spec.input_schema)() {
        Value::Object(map) => map,
        _ => serde_json::Map::new(),
    };
    let read = spec.access == fc_api::v1::mcp::McpToolAccess::Read;
    Tool::new(spec.name, spec.description, schema)
        .with_title(spec.title)
        .with_annotations(
            ToolAnnotations::with_title(spec.title)
                .read_only(read)
                .destructive(spec.destructive)
                .idempotent(spec.idempotent)
                .open_world(false),
        )
}

fn error_result(e: &ApiError) -> CallToolResult {
    CallToolResult::structured_error(serde_json::to_value(e).unwrap_or_else(
        |_| serde_json::json!({ "code": e.code.to_string(), "message": e.message }),
    ))
}

impl FlashcardServer {
    /// Serves `app`; with `read_only` the write tools are neither listed nor callable
    /// (`fc mcp serve --read-only`), on top of the app's `mcp.server.allow_writes` setting.
    pub fn new(app: Arc<App>, read_only: bool) -> Self {
        Self { app, read_only }
    }

    async fn blocking<T: Send + 'static>(
        &self,
        f: impl FnOnce(&App) -> T + Send + 'static,
    ) -> Result<T, ErrorData> {
        let app = self.app.clone();
        tokio::task::spawn_blocking(move || f(&app))
            .await
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))
    }
}

impl ServerHandler for FlashcardServer {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(
                Implementation::new(SERVER_NAME, env!("CARGO_PKG_VERSION")).with_title("Flashcard"),
            )
            .with_instructions(tools::INSTRUCTIONS)
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, ErrorData> {
        let read_only = self.read_only;
        let listed = self
            .blocking(move |app| tools::list_tools(app, read_only))
            .await?;
        // Module off: no tools rather than a failing session.
        let tools = listed
            .map(|specs| specs.into_iter().map(tool_definition).collect())
            .unwrap_or_default();
        Ok(ListToolsResult {
            tools,
            ..Default::default()
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, ErrorData> {
        let name = request.name.to_string();
        if !tools::catalog().iter().any(|t| t.name == name) {
            return Err(ErrorData::invalid_params(
                format!("unknown tool {name:?}"),
                None,
            ));
        }
        let client = context
            .peer
            .peer_info()
            .map(|i| i.client_info.name.clone())
            .unwrap_or_default();
        let args = request.arguments.map_or(Value::Null, Value::Object);
        let read_only = self.read_only;
        let result = self
            .blocking(move |app| tools::call_tool(app, &client, read_only, &name, args))
            .await?;
        Ok(match result {
            Ok(value) => CallToolResult::structured(value),
            Err(e) => error_result(&e),
        }
        .into())
    }
}

/// Serves `app` on this process's stdin/stdout until the client disconnects (how Claude
/// Desktop / Claude Code launch `fc mcp serve`). Nothing else may write to stdout.
pub async fn serve_stdio(app: Arc<App>, read_only: bool) -> Result<(), McpError> {
    let running = FlashcardServer::new(app, read_only)
        .serve(rmcp::transport::stdio())
        .await
        .map_err(|e| McpError::Handshake(e.to_string()))?;
    running
        .waiting()
        .await
        .map_err(|e| McpError::Join(e.to_string()))?;
    Ok(())
}
