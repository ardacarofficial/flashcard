//! `fc dev serve` (v1.3): the **development** bridge. Serves one collection to a browser so the
//! desktop UI (`vite --mode bridge`) and the Playwright journeys (`apps/desktop/e2e`) run
//! against the real Rust core instead of the mock.
//!
//! This is a development tool, not a product feature: the module is compiled only into debug
//! builds or with the `dev-bridge` cargo feature, never into a release `fc`. It is **not** the
//! local HTTP API (`fc api serve`, `fc-http`), which is a restricted, client-scoped surface for
//! the browser extension: the bridge reaches every public method exactly like the desktop
//! webview does ([`fc_app::transport::dispatch`]: user actor, lock gating, error shape,
//! post-commit events). See [`bridge`] for the wire format and the guards.

pub mod bridge;

use crate::i18n::Catalog;
use clap::Subcommand;
use fc_api::{ApiError, ErrorCode};
use fc_app::App;
use fc_core::i18n::Locale;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

/// `fc dev` subcommands (development builds only).
#[derive(Debug, Subcommand)]
pub enum DevCommand {
    /// DEVELOPMENT ONLY: serve the collection (created and migrated if needed) to the desktop
    /// UI in a browser (`vite --mode bridge`) on 127.0.0.1 until stopped. Prints one JSON line
    /// `{"url": …, "token": …}` on stdout.
    Serve {
        /// Port (`0` = any free port).
        #[arg(long, default_value_t = 0)]
        port: u16,
        /// Access token (at least 16 characters; default: a fresh random one). Every request
        /// but `GET /health` must send it.
        #[arg(long, env = "FC_BRIDGE_TOKEN")]
        token: Option<String>,
    },
}

/// Opens (creating/migrating, like the desktop app) the collection at `db` and serves it
/// until the process is stopped.
pub fn serve(
    db: &Path,
    port: u16,
    token: Option<String>,
    lang: Option<Locale>,
) -> Result<(), ApiError> {
    let token = match token {
        Some(t) if t.chars().count() < bridge::MIN_TOKEN_CHARS => {
            let text = Catalog::new(lang.unwrap_or(Locale::En));
            return Err(ApiError::new(
                ErrorCode::ValidationInvalidParams,
                text.with(
                    "cli.dev.weak_token",
                    "min",
                    bridge::MIN_TOKEN_CHARS.to_string(),
                ),
            ));
        }
        Some(t) => t,
        None => bridge::random_token().map_err(|e| ApiError::new(ErrorCode::Internal, e))?,
    };
    if let Some(dir) = db.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)
            .map_err(|e| ApiError::new(ErrorCode::IoFile, e.to_string()))?;
    }
    let app = Arc::new(App::open_sqlite(db).map_err(fc_app::AppError::into_api)?);
    // Throwaway development profiles never touch the OS keychain: AI keys and the app lock
    // live in process memory for this run.
    app.set_secret_store(Arc::new(fc_ai::secrets::MemorySecrets::new()));
    // Imports, exports and backups are background jobs, as in the desktop app.
    let _worker = fc_app::jobs::spawn_worker(app.clone())
        .map_err(|e| ApiError::new(ErrorCode::Internal, e.to_string()))?;
    let server = bridge::Bridge::start(app.clone(), port, token)
        .map_err(|e| ApiError::new(ErrorCode::IoFile, e.to_string()))?;
    let text = Catalog::new(lang.unwrap_or_else(|| app.locale()));
    eprintln!("{}", text.with("cli.dev.listening", "url", server.url()));
    let line = serde_json::json!({ "url": server.url(), "token": server.token() }).to_string();
    crate::write_line(std::io::stdout().lock(), &line)
        .map_err(|e| ApiError::new(ErrorCode::IoFile, e.to_string()))?;
    loop {
        std::thread::sleep(Duration::from_secs(1));
        // Commits other processes (`fc call` from a journey) made: open screens refetch.
        let _ = app.poll_external_changes();
    }
}

#[cfg(test)]
mod tests;
