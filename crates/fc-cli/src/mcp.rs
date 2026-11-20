//! `fc mcp …`: the MCP server for Claude Desktop / Claude Code (`fc mcp serve`) and a probe
//! for external MCP servers (`fc mcp tools`). See docs/06 §2 for the client configuration.

use crate::commands::Ctx;
use crate::lookup::call;
use clap::Subcommand;
use fc_api::v1::common::Empty;
use fc_api::v1::methods as m;
use fc_api::{ApiError, ErrorCode};
use fc_app::App;
use fc_mcp::McpClient;
use std::path::Path;
use std::sync::Arc;

/// `fc mcp` subcommands.
#[derive(Debug, Subcommand)]
pub enum McpCommand {
    /// Serve the collection to an MCP client over stdin/stdout (launched by Claude Desktop /
    /// Claude Code; stdout is the protocol channel). The database must already exist.
    Serve {
        /// Only list and run the read tools, whatever the app setting says.
        #[arg(long)]
        read_only: bool,
    },
    /// Connect to an external MCP server and list its tools: a registered server by name
    /// (`mcp.servers.add`), or a command given after `--`.
    Tools {
        /// Registered server name.
        #[arg(conflicts_with = "command")]
        server: Option<String>,
        /// Program to launch, followed by its arguments (`fc mcp tools -- npx -y pkg`).
        #[arg(last = true, num_args = 1..)]
        command: Vec<String>,
    },
}

fn io(e: impl std::fmt::Display) -> ApiError {
    ApiError::new(ErrorCode::IoFile, e.to_string())
}

fn runtime() -> Result<tokio::runtime::Runtime, ApiError> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(io)
}

/// Opens the collection and serves it until the client disconnects. Nothing is printed to
/// stdout besides the protocol. The collection is never migrated here: an older schema is
/// refused (open the app once to upgrade it), so a stray MCP launch cannot rewrite a database
/// the app owns.
pub fn serve(db: &Path, read_only: bool) -> Result<(), ApiError> {
    if !db.is_file() {
        return Err(ApiError::new(
            ErrorCode::IoFile,
            format!(
                "no collection at {} (pass --db with the path of an existing collection.db)",
                db.display()
            ),
        ));
    }
    let app = Arc::new(App::open_sqlite_current(db).map_err(fc_app::AppError::into_api)?);
    runtime()?
        .block_on(fc_mcp::serve_stdio(app, read_only))
        .map_err(io)
}

/// `fc mcp tools`.
pub fn tools(ctx: &Ctx<'_>, server: Option<&str>, command: &[String]) -> Result<String, ApiError> {
    let registered = match server {
        Some(name) => Some(
            call::<m::McpServersList>(ctx.app, &Empty {})?
                .servers
                .into_iter()
                .find(|s| s.name.to_lowercase() == name.trim().to_lowercase())
                .ok_or_else(|| {
                    ApiError::new(
                        ErrorCode::NotFoundMcpServer,
                        format!("no MCP server {name:?}"),
                    )
                    .with_detail("name", name)
                })?,
        ),
        None => None,
    };
    let tools = runtime()?.block_on(async {
        let client = match (&registered, command.split_first()) {
            (Some(s), _) => McpClient::connect_registered(s).await,
            (None, Some((program, args))) => McpClient::connect_stdio(program, args).await,
            (None, None) => {
                return Err(ApiError::new(
                    ErrorCode::ValidationInvalidParams,
                    "give a registered server name or a command after --",
                ))
            }
        }
        .map_err(io)?;
        let tools = client.list_tools().await.map_err(io)?;
        client.close().await.map_err(io)?;
        Ok(tools)
    })?;
    if ctx.json {
        let v: Vec<_> = tools
            .iter()
            .map(|t| {
                serde_json::json!({
                    "name": t.name,
                    "description": t.description,
                    "read_only": t.read_only,
                    "needs_confirmation": t.needs_confirmation(),
                    "input_schema": t.input_schema,
                })
            })
            .collect();
        return serde_json::to_string_pretty(&v).map_err(io);
    }
    Ok(tools
        .iter()
        .map(|t| {
            let mark = if t.needs_confirmation() { "!" } else { " " };
            format!("{mark} {}", t.name)
        })
        .collect::<Vec<_>>()
        .join("\n"))
}
