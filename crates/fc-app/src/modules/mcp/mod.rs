//! `mcp` module (v0.8, docs/06 §2): the MCP server's curated tool catalog ([`tools`]) and
//! the external MCP servers the app may use as a client ([`servers`]), and the paths a client
//! configuration needs ([`setup`]).
//!
//! The server transport lives in `fc-mcp` (`fc mcp serve`); it only lists [`tools::catalog`]
//! and forwards calls to [`tools::call_tool`]. Every tool is a thin wrapper over **public**
//! v1 methods invoked as [`Actor::Mcp`], so:
//!
//! * internal methods are unreachable (the registry refuses them for every actor),
//! * the MCP actor holds only [`MCP_PERMISSIONS`] (no settings/modules/undo/plugin/`mcp:write`
//!   changes: an MCP client cannot make the app launch new external servers) **and** reaches
//!   only the methods some tool calls ([`tools::method_reachable`]): `notes:write` would also
//!   cover `notes.delete`, but no tool deletes, so the registry refuses it,
//! * every write is an ordinary undoable command, recorded in the oplog with actor `mcp`.

pub mod servers;
pub mod setup;
pub mod tools;
#[cfg(test)]
mod tests;

use crate::module::{bool_setting, main_menu, ModuleDescriptor};
use crate::registry::Registry;
use fc_api::v1::methods::*;
use fc_core::settings::SettingScopeKind;

/// Module id.
pub const ID: &str = "mcp";

/// Setting (app scope): MCP clients may run write tools. Off = the server lists and runs the
/// read tools only.
pub const ALLOW_WRITES: &str = "mcp.server.allow_writes";

/// Permissions the [`fc_api::v1::oplog::Actor::Mcp`] actor holds (checked by the registry for
/// every call, including each call inside `batch.execute`).
pub const MCP_PERMISSIONS: &[&str] = &[
    "decks:read",
    "decks:write",
    "notetypes:read",
    "notes:read",
    "notes:write",
    "attributes:read",
    "attributes:write",
    "search:read",
    "batch:write",
    "settings:read",
    "modules:read",
    "mcp:read",
    // v1.4: the word index (`words.lookup/list/stats`, read-only tools).
    "words:read",
];

/// Descriptor.
pub fn descriptor() -> ModuleDescriptor {
    ModuleDescriptor {
        id: ID,
        name_key: "modules.mcp.name",
        description_key: "modules.mcp.description",
        enabled_by_default: true,
        can_disable: true,
        settings: vec![bool_setting(ALLOW_WRITES, true, &[SettingScopeKind::App])],
        ui: vec![main_menu("mcp.main", "nav.mcp", "plug", "/mcp", 80)],
    }
}

/// Registers the module and its methods.
pub fn register(reg: &mut Registry) {
    reg.module(descriptor());
    reg.query::<McpToolsList>(tools::list);
    reg.query::<McpServersList>(servers::list);
    reg.query::<McpSetupGet>(setup::get);
    reg.command::<McpServersAdd>(servers::add);
    reg.command::<McpServersUpdate>(servers::update);
    reg.command::<McpServersRemove>(servers::remove);
    reg.command::<McpServersRestore>(servers::restore);
}
