//! Local HTTP API clients (v0.9, docs/06 §3). The server (`fc-http`) binds to loopback only;
//! a client pairs (`POST /v1/pair`), the user approves it in the app with scoped permissions,
//! and it then calls a fixed allow-list of public methods as `Actor::LocalApi(<client id>)`.
//!
//! Also the transport-level DTOs of the HTTP server (ping, pairing), exported to TypeScript
//! for clients (the browser extension) by [`export_http_types`].

use fc_core::{ApiClientId, Timestamp};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// What kind of program a client is (shown in the approval dialog).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ApiClientKind {
    /// A browser extension.
    BrowserExtension,
    /// Another desktop program.
    App,
    /// A script.
    Script,
}

/// Client state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ApiClientStatus {
    /// Waiting for the user's decision.
    Pending,
    /// Approved: its token works.
    Approved,
    /// The user refused it.
    Denied,
    /// Approved earlier, revoked since.
    Revoked,
}

/// A local API client. The token itself is never stored or shown (only its hash is kept).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ApiClientDto {
    /// Id (also the oplog actor id: `{"kind":"local_api","id":…}`).
    pub id: ApiClientId,
    /// Name it gave.
    pub name: String,
    /// Kind it gave.
    pub kind: ApiClientKind,
    /// Browser `Origin` it paired from (`chrome-extension://…`); its token works only from
    /// there (or without an `Origin`).
    pub origin: Option<String>,
    /// Scopes it asked for.
    pub requested_scopes: Vec<String>,
    /// Scopes granted (empty unless approved).
    pub scopes: Vec<String>,
    /// State.
    pub status: ApiClientStatus,
    /// Paired.
    pub created_at: Timestamp,
    /// Last state change.
    pub updated_at: Timestamp,
    /// Last request (updated at most once a minute).
    pub last_used_at: Option<Timestamp>,
}

/// Response of `local_api.clients.list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ApiClientList {
    /// Clients, newest first.
    pub clients: Vec<ApiClientDto>,
    /// Scopes a client may be granted.
    pub grantable_scopes: Vec<String>,
    /// Port the server listens on (setting `local_api.port`).
    pub port: u16,
}

/// `local_api.clients.approve`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct ApproveApiClient {
    /// Client.
    pub id: ApiClientId,
    /// Scopes to grant (`null` = all it asked for); must be a subset of what it asked for.
    #[serde(default)]
    pub scopes: Option<Vec<String>>,
}

/// A client reference (`deny`, `revoke`, `remove`, internal `touch`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ApiClientRef {
    /// Client.
    pub id: ApiClientId,
}

/// Internal `local_api.pair`: records a pairing request (called by the HTTP server, which
/// made the token and passes only its SHA-256 hash).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct RecordPairing {
    /// Name.
    pub name: String,
    /// Kind.
    pub kind: ApiClientKind,
    /// Requested scopes.
    pub scopes: Vec<String>,
    /// Browser origin, if any.
    pub origin: Option<String>,
    /// Hex SHA-256 of the token.
    pub token_hash: String,
}

/// One method a client may call over HTTP.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct LocalApiMethod {
    /// Unversioned name (`inbox.add`): `POST /v1/<name>`.
    pub name: String,
    /// `query` or `command`.
    pub kind: crate::method::MethodKind,
    /// Scope it needs.
    pub permission: String,
}

/// Response of `GET /v1/methods` (and `local_api.methods`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct LocalApiMethodList {
    /// Methods, by name.
    pub methods: Vec<LocalApiMethod>,
}

// ------------------------------------------------------------------ HTTP transport DTOs

/// `GET /v1/ping`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PingResponse {
    /// Always `flashcard`.
    pub app: String,
    /// API version (`v1`).
    pub api_version: String,
    /// App version.
    pub version: String,
}

/// `POST /v1/pair`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct PairRequest {
    /// Name shown to the user (1–60 chars).
    pub client_name: String,
    /// Kind.
    pub client_kind: ApiClientKind,
    /// Scopes asked for (see `ApiClientList.grantable_scopes`).
    pub scopes: Vec<String>,
}

/// State of a pairing request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum PairStatus {
    /// Waiting for the user.
    Pending,
    /// Approved.
    Approved,
    /// Refused.
    Denied,
    /// Not decided in time, or the app restarted before the token was collected.
    Expired,
}

/// Response of `POST /v1/pair` (HTTP 201).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PairResponse {
    /// Secret id to poll with (`GET /v1/pair/{request_id}`).
    pub request_id: String,
    /// `pending`.
    pub status: PairStatus,
    /// When the request expires.
    pub expires_at: Timestamp,
}

/// Response of `GET /v1/pair/{request_id}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PairStatusResponse {
    /// State.
    pub status: PairStatus,
    /// Client id once approved.
    pub client_id: Option<ApiClientId>,
    /// Bearer token: only in the **first** approved response, never again.
    pub token: Option<String>,
    /// Scopes granted (empty unless approved).
    pub scopes: Vec<String>,
}

/// Exports the transport DTOs (not reachable from method signatures) to TypeScript.
pub fn export_http_types(cfg: &ts_rs::Config) -> Result<(), ts_rs::ExportError> {
    PingResponse::export_all(cfg)?;
    PairRequest::export_all(cfg)?;
    PairResponse::export_all(cfg)?;
    PairStatusResponse::export_all(cfg)?;
    LocalApiMethodList::export_all(cfg)?;
    Ok(())
}
