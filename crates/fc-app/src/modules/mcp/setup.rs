//! `mcp.setup` (v1.3): the real paths an MCP client's configuration needs, so the settings
//! screen shows a ready-to-paste snippet instead of `/path/to/collection.db`.
//!
//! The collection file is only known when the desktop app started this process
//! ([`App::start_maintenance`](crate::app::App::start_maintenance) records the profile's
//! folder); the `fc` program is looked up next to the running executable, then on `PATH`.

use crate::error::AppResult;
use crate::handler::QueryCtx;
use crate::modules::maintenance::MaintenanceServices;
use crate::modules::profiles::store::COLLECTION_FILE;
use fc_api::v1::common::Empty;
use fc_api::v1::mcp::McpSetup;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

/// File name of the command-line program on this platform.
pub const FC_PROGRAM: &str = if cfg!(windows) { "fc.exe" } else { "fc" };

/// `mcp.setup`.
pub(super) fn get(_: Empty, ctx: &QueryCtx<'_>) -> AppResult<McpSetup> {
    let db_path = ctx
        .registry
        .extension::<MaintenanceServices>()
        .and_then(|svc| svc.data_dir())
        .map(|dir| dir.join(COLLECTION_FILE).display().to_string());
    let exe = std::env::current_exe().ok();
    let path_var = std::env::var_os("PATH");
    let fc_path = find_fc(exe.as_deref(), path_var.as_deref(), FC_PROGRAM)
        .map(|p| p.display().to_string());
    Ok(McpSetup {
        db_path,
        fc_path,
        platform: std::env::consts::OS.to_owned(),
    })
}

/// The `program` file next to `exe`, else the first one in the `PATH`-style list `path_var`.
pub fn find_fc(exe: Option<&Path>, path_var: Option<&OsStr>, program: &str) -> Option<PathBuf> {
    let sibling = exe
        .and_then(Path::parent)
        .map(|dir| dir.join(program))
        .filter(|p| p.is_file());
    sibling.or_else(|| {
        path_var.and_then(|v| {
            std::env::split_paths(v)
                .filter(|dir| !dir.as_os_str().is_empty())
                .map(|dir| dir.join(program))
                .find(|p| p.is_file())
        })
    })
}
