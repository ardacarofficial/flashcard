//! `fc api serve`: the local HTTP API (docs/06 §3) without the desktop app — for scripts,
//! tests and the browser extension's end-to-end tests. Same server, same rules: loopback only,
//! the `local_api` module must be on, clients pair and are approved in the app (or with
//! `fc call local_api.clients.approve`).

use clap::Subcommand;
use fc_api::{ApiError, ErrorCode};
use fc_app::App;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

/// `fc api` subcommands.
#[derive(Debug, Subcommand)]
pub enum ApiCommand {
    /// Serve the local HTTP API on 127.0.0.1 until stopped. Prints the address to stderr.
    Serve {
        /// Port (default: the `local_api.port` setting; `0` = any free port).
        #[arg(long)]
        port: Option<u16>,
    },
}

/// Opens the collection (never migrating it, like `fc mcp serve`) and serves it.
pub fn serve(db: &Path, port: Option<u16>) -> Result<(), ApiError> {
    if !db.is_file() {
        return Err(ApiError::new(
            ErrorCode::IoFile,
            format!("no collection at {}", db.display()),
        ));
    }
    let app = Arc::new(App::open_sqlite_current(db).map_err(fc_app::AppError::into_api)?);
    fc_app::modules::local_api::ensure_enabled(&app).map_err(|e| {
        ApiError {
            message: "the local_api module is off; turn it on first: fc call modules.set_enabled '{\"id\":\"local_api\",\"enabled\":true}'".into(),
            ..e
        }
    })?;
    let port = match port {
        Some(p) => p,
        None => fc_app::modules::local_api::port(&app)?,
    };
    let server = fc_http::Server::start(app.clone(), fc_http::ServerConfig::localhost(port))
        .map_err(|e| ApiError::new(ErrorCode::IoFile, e.to_string()))?;
    eprintln!("fc api: listening on http://{}", server.local_addr());
    loop {
        std::thread::sleep(Duration::from_secs(2));
        // Module switches and settings changed by other processes (the desktop, `fc call`).
        let _ = app.poll_external_changes();
    }
}
