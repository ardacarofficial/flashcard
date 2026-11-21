//! The local HTTP API server (v0.9, docs/06 §3). A thin transport over `fc-app`: every
//! decision about clients, scopes and methods is the `local_api` module's
//! ([`fc_app::modules::local_api`]); this crate only speaks HTTP and enforces what is HTTP's:
//!
//! * **Loopback only.** [`ServerConfig`] can only name `127.0.0.1` or `[::1]`; the bound socket
//!   is checked again. The `Host` header must name the loopback address (DNS-rebinding guard).
//! * **Origins.** No `Origin` (curl, scripts, extension GETs) is fine; a browser-extension
//!   origin is fine (CORS answered for it only); any web page origin is `403`. A token works
//!   only from the origin it paired from (or with none).
//! * **CSRF.** Beyond the `Origin` check: every request but `GET /v1/ping` — pairing and
//!   polling included — needs the custom header [`CLIENT_HEADER`] and every `POST` needs
//!   `Content-Type: application/json`. A page cannot send either cross-origin without a
//!   preflight, so even a browser that sent no `Origin` could not reach user data.
//! * **Tokens.** `Authorization: Bearer <token>`; made here from the OS random generator,
//!   handed to the client once after the user approves the pairing, never stored — the app
//!   keeps its SHA-256 hash.
//! * **Limits.** Body ≤ [`MAX_BODY_BYTES`] (`413`), [`CLIENT_RATE_PER_SEC`] requests per
//!   second per client (burst [`CLIENT_BURST`]), [`PAIR_PER_MINUTE`] pairing requests per
//!   minute and [`PAIR_PER_NAME`] per client name per [`PAIR_NAME_WINDOW_SECS`] (`429` +
//!   `Retry-After`).
//!
//! The same server also answers the Subtitle browser extension's contract under
//! `/subtitle/v1/*` (v1.2, module `subtitle`, see `subtitle.rs`): it runs while either module
//! is on, and `/v1/*` answers only while `local_api` is on (`503 CONFLICT_MODULE_DISABLED`
//! otherwise, `ping` included), `/subtitle/*` only while `subtitle` is on (`404`).
//!
//! App calls are blocking (SQLite), so they run on tokio's blocking pool.

mod routes;
mod subtitle;

use fc_app::App;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;

/// Largest request body accepted (bytes).
pub const MAX_BODY_BYTES: usize = 256 * 1024;
/// Sustained requests per second per client.
pub const CLIENT_RATE_PER_SEC: f64 = 10.0;
/// Burst of requests per client.
pub const CLIENT_BURST: f64 = 30.0;
/// Pairing requests per minute (whole server).
pub const PAIR_PER_MINUTE: f64 = 5.0;
/// Pairing requests one client name (per origin) may make within [`PAIR_NAME_WINDOW_SECS`].
pub const PAIR_PER_NAME: f64 = 3.0;
/// Window of [`PAIR_PER_NAME`] (seconds).
pub const PAIR_NAME_WINDOW_SECS: f64 = 600.0;
/// The header every request but `GET /v1/ping` must carry. A web page cannot add it to a
/// cross-origin request without a CORS preflight, which only extensions pass.
pub const CLIENT_HEADER: &str = "x-flashcard-client";

/// The loopback interface to listen on. There is deliberately no other choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Loopback {
    /// `127.0.0.1`.
    V4,
    /// `::1`.
    V6,
}

impl Loopback {
    /// The address.
    pub fn ip(self) -> IpAddr {
        match self {
            Self::V4 => IpAddr::V4(Ipv4Addr::LOCALHOST),
            Self::V6 => IpAddr::V6(Ipv6Addr::LOCALHOST),
        }
    }
}

/// Where the server listens: a loopback address and a port (`0` = any free port).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServerConfig {
    /// Interface.
    pub loopback: Loopback,
    /// Port.
    pub port: u16,
}

/// A listen address that is not loopback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotLoopback(pub String);

impl std::fmt::Display for NotLoopback {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} is not a loopback address; the local API listens on 127.0.0.1 or [::1] only",
            self.0
        )
    }
}

impl std::error::Error for NotLoopback {}

impl ServerConfig {
    /// `127.0.0.1:<port>`.
    pub fn localhost(port: u16) -> Self {
        Self {
            loopback: Loopback::V4,
            port,
        }
    }

    /// The socket address.
    pub fn addr(&self) -> SocketAddr {
        SocketAddr::new(self.loopback.ip(), self.port)
    }

    /// Parses `127.0.0.1:<port>`, `[::1]:<port>` or `localhost:<port>`; anything else
    /// (`0.0.0.0`, a LAN address, a host name) is refused.
    pub fn parse(s: &str) -> Result<Self, NotLoopback> {
        let s = s.trim();
        let (host, port) = s
            .rsplit_once(':')
            .ok_or_else(|| NotLoopback(s.to_owned()))?;
        let port: u16 = port.parse().map_err(|_| NotLoopback(s.to_owned()))?;
        let loopback = match host.to_ascii_lowercase().as_str() {
            "127.0.0.1" | "localhost" => Loopback::V4,
            "[::1]" => Loopback::V6,
            _ => return Err(NotLoopback(s.to_owned())),
        };
        Ok(Self { loopback, port })
    }
}

/// A running server (its own runtime thread). Dropping it stops the server.
pub struct Server {
    addr: SocketAddr,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Server {
    /// Binds `config` (synchronously, so a busy port is reported here) and serves `app` on a
    /// background thread.
    pub fn start(app: Arc<App>, config: ServerConfig) -> std::io::Result<Self> {
        let listener = std::net::TcpListener::bind(config.addr())?;
        let addr = listener.local_addr()?;
        if !addr.ip().is_loopback() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                NotLoopback(addr.to_string()),
            ));
        }
        listener.set_nonblocking(true)?;
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("fc-http")
            .enable_all()
            .build()?;
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let router = routes::router(app, addr.port());
        let thread = std::thread::Builder::new()
            .name("fc-http".into())
            .spawn(move || {
                runtime.block_on(async move {
                    let Ok(listener) = tokio::net::TcpListener::from_std(listener) else {
                        return;
                    };
                    let _ = axum::serve(listener, router)
                        .with_graceful_shutdown(async {
                            let _ = rx.await;
                        })
                        .await;
                });
            })?;
        Ok(Self {
            addr,
            shutdown: Some(tx),
            thread: Some(thread),
        })
    }

    /// The bound address (the real port when started with port `0`).
    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }

    /// Stops the server and waits for it.
    pub fn stop(mut self) {
        self.halt();
    }

    fn halt(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.halt();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_names_loopback_only() {
        assert_eq!(
            ServerConfig::parse("127.0.0.1:28735"),
            Ok(ServerConfig::localhost(28735))
        );
        assert_eq!(
            ServerConfig::parse("[::1]:80").map(|c| c.loopback),
            Ok(Loopback::V6)
        );
        assert_eq!(ServerConfig::parse("LOCALHOST:1").map(|c| c.port), Ok(1));
        for bad in [
            "0.0.0.0:28735",
            "192.168.1.5:28735",
            "[::]:1",
            "example.com:80",
            "127.0.0.1",
            "127.0.0.2:1",
        ] {
            assert!(ServerConfig::parse(bad).is_err(), "{bad}");
        }
        assert!(ServerConfig::localhost(1).addr().ip().is_loopback());
        assert!(Loopback::V6.ip().is_loopback());
    }
}
