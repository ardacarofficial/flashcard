//! `fc api serve`: the local HTTP API (docs/06 §3) without the desktop app — for scripts,
//! tests and the browser extension's end-to-end tests. Same server, same rules: loopback only,
//! the `local_api` module (`/v1/*`, clients pair and are approved in the app or with
//! `fc call local_api.clients.approve`) or the `subtitle` module (`/subtitle/v1/*`, v1.2:
//! access keys from `fc call subtitle.keys.create`) must be on.

use crate::i18n::Catalog;
use clap::Subcommand;
use fc_api::{ApiError, ErrorCode};
use fc_app::App;
use fc_core::i18n::Locale;
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

/// Opens the collection (never migrating it, like `fc mcp serve`) and serves it. Messages in
/// `lang` (default: the collection's `app.locale`; English before it is open).
pub fn serve(db: &Path, port: Option<u16>, lang: Option<Locale>) -> Result<(), ApiError> {
    if !db.is_file() {
        let text = Catalog::new(lang.unwrap_or(Locale::En));
        return Err(ApiError::new(
            ErrorCode::IoFile,
            text.with("cli.api.no_collection", "path", db.display().to_string()),
        ));
    }
    let app = Arc::new(App::open_sqlite_current(db).map_err(fc_app::AppError::into_api)?);
    let text = Catalog::new(lang.unwrap_or_else(|| app.locale()));
    // Either module keeps the server up; each answers only its own routes.
    fc_app::modules::local_api::ensure_enabled(&app)
        .or_else(|_| fc_app::modules::subtitle::ensure_enabled(&app))
        .map_err(|e| ApiError {
            message: text.text("cli.api.module_off", &Default::default()),
            ..e
        })?;
    let port = match port {
        Some(p) => p,
        None => fc_app::modules::local_api::port(&app)?,
    };
    let server = fc_http::Server::start(app.clone(), fc_http::ServerConfig::localhost(port))
        .map_err(|e| ApiError::new(ErrorCode::IoFile, e.to_string()))?;
    // The address ends the line in every language: scripts and tests read it from there.
    eprintln!(
        "{}",
        text.with("cli.api.listening", "addr", server.local_addr().to_string())
    );
    loop {
        std::thread::sleep(Duration::from_secs(2));
        // Module switches and settings changed by other processes (the desktop, `fc call`).
        let _ = app.poll_external_changes();
    }
}
