//! External MCP server registrations (docs/06 §2.2): `mcp.servers.*`. Rows only: connecting
//! is the client's job (`fc_mcp::client`), never a handler's.

use crate::error::{AppError, AppResult};
use crate::handler::{inverse_call, CommandCtx, CommandOutcome, QueryCtx};
use crate::support::{err_id, summary};
use fc_api::v1::common::Empty;
use fc_api::v1::mcp::*;
use fc_api::v1::methods::*;
use fc_api::{ApiError, ErrorCode};
use fc_core::query::text::fold;
use fc_core::McpServerId;
use fc_storage::repo::{McpServerRecord, Reads};

const MAX_NAME: usize = 64;
const MAX_ARGS: usize = 64;
const MAX_TEXT: usize = 2048;

fn bad(field: &str, message: impl Into<String>) -> AppError {
    AppError::Api(
        ApiError::new(ErrorCode::ValidationMcpServer, message).with_detail("field", field),
    )
}

fn transport_str(t: McpTransport) -> &'static str {
    match t {
        McpTransport::Stdio => "stdio",
        McpTransport::Http => "http",
    }
}

/// What the user approves when they register a server: the launch command (stdio) or the
/// endpoint (http), as one canonical string. Stored with the row; the row only counts as
/// approved while its current summary still equals it.
pub fn approval_summary(
    transport: McpTransport,
    command: &str,
    args: &[String],
    url: Option<&str>,
) -> String {
    let mut parts = vec![transport_str(transport).to_owned()];
    match transport {
        McpTransport::Stdio => {
            parts.push(command.to_owned());
            parts.extend(args.iter().cloned());
        }
        McpTransport::Http => parts.push(url.unwrap_or_default().to_owned()),
    }
    serde_json::Value::from(parts).to_string()
}

fn dto(r: McpServerRecord) -> McpServerDto {
    let transport = if r.transport == "http" {
        McpTransport::Http
    } else {
        McpTransport::Stdio
    };
    let approved = r.approved.as_deref()
        == Some(approval_summary(transport, &r.command, &r.args, r.url.as_deref()).as_str());
    McpServerDto {
        id: r.id,
        name: r.name,
        transport,
        command: r.command,
        args: r.args,
        url: r.url,
        enabled: r.enabled,
        approved,
        created_at: r.created_at,
        updated_at: r.updated_at,
    }
}

fn record(d: &McpServerDto) -> McpServerRecord {
    McpServerRecord {
        id: d.id,
        name: d.name.clone(),
        transport: transport_str(d.transport).to_owned(),
        command: d.command.clone(),
        args: d.args.clone(),
        url: d.url.clone(),
        enabled: d.enabled,
        approved: d.approved.then(|| {
            approval_summary(d.transport, &d.command, &d.args, d.url.as_deref())
        }),
        created_at: d.created_at,
        updated_at: d.updated_at,
    }
}

/// The validated, editable part of a registration.
struct Fields {
    name: String,
    transport: McpTransport,
    command: String,
    args: Vec<String>,
    url: Option<String>,
}

fn validate(
    reads: &dyn Reads,
    id: Option<McpServerId>,
    name: &str,
    transport: McpTransport,
    command: Option<&str>,
    args: &[String],
    url: Option<&str>,
) -> AppResult<Fields> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > MAX_NAME {
        return Err(bad("name", format!("name must be 1–{MAX_NAME} characters")));
    }
    let taken = reads
        .list_mcp_servers()?
        .into_iter()
        .any(|s| Some(s.id) != id && fold(&s.name) == fold(name));
    if taken {
        return Err(AppError::Api(
            ApiError::new(
                ErrorCode::ConflictMcpServerName,
                format!("an MCP server named {name:?} already exists"),
            )
            .with_detail("name", name),
        ));
    }
    let (command, args, url) = match transport {
        McpTransport::Stdio => {
            let command = command.map(str::trim).unwrap_or_default();
            if command.is_empty() || command.len() > MAX_TEXT {
                return Err(bad("command", "a stdio server needs a command"));
            }
            if args.len() > MAX_ARGS || args.iter().any(|a| a.len() > MAX_TEXT) {
                return Err(bad("args", "too many or too long arguments"));
            }
            (command.to_owned(), args.to_vec(), None)
        }
        McpTransport::Http => {
            let url = url.map(str::trim).unwrap_or_default();
            let scheme_ok = url.starts_with("http://") || url.starts_with("https://");
            if !scheme_ok || url.len() > MAX_TEXT || url.chars().any(char::is_whitespace) {
                return Err(bad("url", "an http server needs an http(s):// URL"));
            }
            (String::new(), Vec::new(), Some(url.to_owned()))
        }
    };
    Ok(Fields {
        name: name.to_owned(),
        transport,
        command,
        args,
        url,
    })
}

fn require(reads: &dyn Reads, id: McpServerId) -> AppResult<McpServerRecord> {
    reads.get_mcp_server(id)?.ok_or_else(|| {
        err_id(
            ErrorCode::NotFoundMcpServer,
            format!("MCP server {id} not found"),
            id,
        )
    })
}

pub(super) fn list(_: Empty, ctx: &QueryCtx<'_>) -> AppResult<McpServerList> {
    Ok(McpServerList {
        servers: ctx.tx.list_mcp_servers()?.into_iter().map(dto).collect(),
    })
}

pub(super) fn add(
    req: AddMcpServer,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<McpServerDto>> {
    let f = validate(
        ctx.reads(),
        None,
        &req.name,
        req.transport,
        req.command.as_deref(),
        &req.args,
        req.url.as_deref(),
    )?;
    let server = McpServerDto {
        id: McpServerId::generate(ctx.ids),
        name: f.name,
        transport: f.transport,
        command: f.command,
        args: f.args,
        url: f.url,
        enabled: req.enabled.unwrap_or(true),
        // Only the user (and the CLI) hold `mcp:write`: registering is the approval.
        approved: true,
        created_at: ctx.now,
        updated_at: ctx.now,
    };
    ctx.tx.upsert_mcp_server(&record(&server))?;
    Ok(CommandOutcome {
        inverse: vec![inverse_call::<McpServersRemove>(&McpServerRef { id: server.id })?],
        summary: summary::<McpServersAdd>().param("name", &server.name),
        events: Vec::new(),
        response: server,
    })
}

pub(super) fn update(
    req: UpdateMcpServer,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<McpServerDto>> {
    let old = dto(require(ctx.reads(), req.id)?);
    let f = validate(
        ctx.reads(),
        Some(req.id),
        &req.name,
        req.transport,
        Some(&req.command),
        &req.args,
        req.url.as_deref(),
    )?;
    let server = McpServerDto {
        name: f.name,
        transport: f.transport,
        command: f.command,
        args: f.args,
        url: f.url,
        enabled: req.enabled,
        approved: true,
        updated_at: ctx.now,
        ..old.clone()
    };
    ctx.tx.upsert_mcp_server(&record(&server))?;
    Ok(CommandOutcome {
        // The exact previous row (approval included) goes back, not a fresh approval.
        inverse: vec![inverse_call::<McpServersRestore>(&RestoreMcpServer {
            server: old,
        })?],
        summary: summary::<McpServersUpdate>().param("name", &server.name),
        events: Vec::new(),
        response: server,
    })
}

pub(super) fn remove(
    req: McpServerRef,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<McpServerDto>> {
    let old = dto(require(ctx.reads(), req.id)?);
    ctx.tx.delete_mcp_server(old.id)?;
    Ok(CommandOutcome {
        inverse: vec![inverse_call::<McpServersRestore>(&RestoreMcpServer {
            server: old.clone(),
        })?],
        summary: summary::<McpServersRemove>().param("name", &old.name),
        events: Vec::new(),
        response: old,
    })
}

/// Puts `server` back exactly (an upsert): the inverse of `remove` and of `update`.
pub(super) fn restore(
    req: RestoreMcpServer,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<McpServerDto>> {
    let server = req.server;
    let previous = ctx.reads().get_mcp_server(server.id)?.map(dto);
    ctx.tx.upsert_mcp_server(&record(&server))?;
    let inverse = match previous {
        Some(old) => inverse_call::<McpServersRestore>(&RestoreMcpServer { server: old })?,
        None => inverse_call::<McpServersRemove>(&McpServerRef { id: server.id })?,
    };
    Ok(CommandOutcome {
        inverse: vec![inverse],
        summary: summary::<McpServersRestore>().param("name", &server.name),
        events: Vec::new(),
        response: server,
    })
}
