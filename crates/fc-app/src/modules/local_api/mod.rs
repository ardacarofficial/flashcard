//! `local_api` module (v0.9, docs/06 §3): clients of the local HTTP API (`fc-http`).
//!
//! **Off by default**: while the module is off the server does not listen. A client pairs
//! (`POST /v1/pair` → internal `local_api.pair`, a *pending* row holding only its token's
//! SHA-256 hash), the user approves it with a subset of the scopes it asked for, and from then
//! on its requests run as `Actor::LocalApi(<client id>)`:
//!
//! * [`Registry::permits`](crate::registry::Registry::permits) grants it exactly its approved
//!   scopes, and only those in [`GRANTABLE_SCOPES`] (no `io`, `plugins`, `mcp`, `ai*`,
//!   `oplog`, `settings:write`, `modules:write`, `batch`, `local_api` …), whatever a stored row
//!   says;
//! * the registry further limits it to [`HTTP_METHODS`] (`ensure_access`): public,
//!   undoable-and-replayable commands and plain reads only — `notes:write` would also cover
//!   `notes.delete`, but that method is not on the list, so it stays unreachable.
//!
//! Approving, denying and revoking are not undo steps (`undo: false`): an undo must never
//! revive a revoked client. Restoring a backup revokes every client (`io::backup`), so a
//! crafted backup cannot smuggle in an approved token.

#[cfg(test)]
mod tests;

use crate::app::App;
use crate::error::{AppError, AppResult};
use crate::handler::{CommandCtx, CommandOutcome, QueryCtx};
use crate::module::{main_menu, ModuleDescriptor};
use crate::registry::Registry;
use crate::settings::resolve_stored;
use crate::support::{err_id, summary};
use fc_api::envelope::InvokeRequest;
use fc_api::v1::common::Empty;
use fc_api::v1::events::DomainEvent;
use fc_api::v1::inbox::{AddInboxItem, InboxItemDto};
use fc_api::v1::local_api::*;
use fc_api::v1::methods::*;
use fc_api::v1::oplog::Actor;
use fc_api::{ApiError, ApiMethod, ErrorCode};
use fc_core::settings::{SettingDef, SettingKind, SettingScope, SettingScopeKind};
use fc_core::{ApiClientId, Timestamp};
use fc_storage::repo::ApiClientRecord;
use fc_storage::Reads;
use serde_json::{json, Value};
use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, RwLock};

/// Module id.
pub const ID: &str = "local_api";

/// Setting (app): the port the server listens on (loopback only).
pub const PORT: &str = "local_api.port";
/// Default port.
pub const DEFAULT_PORT: u16 = 28735;

/// Setting (app): most proposals one client may have waiting in the inbox.
pub const INBOX_MAX_ITEMS: &str = "local_api.inbox_max_items";
/// Default for [`INBOX_MAX_ITEMS`].
pub const DEFAULT_INBOX_MAX_ITEMS: u32 = 500;
/// Setting (app): most payload KiB one client may have waiting in the inbox.
pub const INBOX_MAX_KIB: &str = "local_api.inbox_max_kib";
/// Default for [`INBOX_MAX_KIB`] (4 MiB).
pub const DEFAULT_INBOX_MAX_KIB: u64 = 4096;
/// Setting (app): most proposals one client may send in 24 hours (any status: accepting or
/// rejecting them does not make room). Bounds a client that floods the inbox (and, with an
/// auto-accept inbox rule, the collection).
pub const INBOX_DAILY_MAX: &str = "local_api.inbox_daily_max";
/// Default for [`INBOX_DAILY_MAX`].
pub const DEFAULT_INBOX_DAILY_MAX: u32 = 200;
/// The window of [`INBOX_DAILY_MAX`].
pub const INBOX_DAILY_WINDOW_MS: i64 = 24 * 60 * 60 * 1000;

/// How long a pairing request waits for the user (ms). Older pending rows are hidden and
/// cannot be approved; the server forgets their tokens.
pub const PAIRING_TTL_MS: i64 = 10 * 60 * 1000;

/// Most pending pairing requests at once (a newer one evicts the oldest).
pub const MAX_PENDING: usize = 5;

/// Scopes a local API client may be granted.
pub const GRANTABLE_SCOPES: &[&str] = &[
    "inbox:write",
    "inbox:read",
    "decks:read",
    "decks:write",
    "notetypes:read",
    "notes:read",
    "notes:write",
    "attributes:read",
    "tags:read",
    "search:read",
];

/// Methods a local API client may call (`POST /v1/<name>`), whatever its scopes. Every one is
/// public; every command is undoable and replayable (no files, jobs or side channels).
pub const HTTP_METHODS: &[&str] = &[
    "inbox.add",
    "inbox.list",
    "decks.list",
    "decks.tree",
    "decks.create",
    "notetypes.list",
    "notes.get",
    "notes.create",
    "attributes.effective",
    "tags.list",
    "search.cards",
];

/// Browser origins that may pair (extension schemes). Web pages (`http(s)://`, `null`) never
/// may.
pub const EXTENSION_SCHEMES: &[&str] = &["chrome-extension://", "moz-extension://", "extension://"];

/// Whether `origin` is a browser extension's.
pub fn is_extension_origin(origin: &str) -> bool {
    EXTENSION_SCHEMES
        .iter()
        .any(|s| origin.len() > s.len() && origin.starts_with(s))
        && origin.len() <= 200
        && !origin.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// Whether a local API client may reach method `name` (see [`HTTP_METHODS`]).
pub fn method_reachable(name: &str) -> bool {
    HTTP_METHODS.contains(&name)
}

/// The scopes each authenticated client holds right now, by client id (refreshed from storage
/// on every request; cleared at once on deny/revoke/remove).
#[derive(Debug, Default)]
pub struct LocalApiGrants(RwLock<HashMap<String, Arc<BTreeSet<String>>>>);

impl LocalApiGrants {
    fn set(&self, client: &str, scopes: Option<BTreeSet<String>>) {
        if let Ok(mut m) = self.0.write() {
            match scopes {
                Some(s) => {
                    m.insert(client.to_owned(), Arc::new(s));
                }
                None => {
                    m.remove(client);
                }
            }
        }
    }

    fn holds(&self, client: &str, permission: &str) -> bool {
        self.0
            .read()
            .ok()
            .and_then(|m| m.get(client).cloned())
            .is_some_and(|s| s.contains(permission))
    }
}

/// [`Registry::permits`] for `Actor::LocalApi(client)`.
pub(crate) fn permits(registry: &Registry, client: &str, permission: &str) -> bool {
    GRANTABLE_SCOPES.contains(&permission)
        && registry
            .extension::<LocalApiGrants>()
            .is_some_and(|g| g.holds(client, permission))
}

fn grants(registry: &Registry) -> Option<Arc<LocalApiGrants>> {
    registry.extension::<LocalApiGrants>()
}

/// Descriptor.
pub fn descriptor() -> ModuleDescriptor {
    ModuleDescriptor {
        id: ID,
        name_key: "modules.local_api.name",
        description_key: "modules.local_api.description",
        enabled_by_default: false,
        can_disable: true,
        settings: vec![
            SettingDef {
                key: PORT.to_owned(),
                kind: SettingKind::Integer {
                    min: Some(1024),
                    max: Some(65535),
                },
                default: json!(DEFAULT_PORT),
                label_key: format!("settings.{PORT}.label"),
                description_key: Some(format!("settings.{PORT}.description")),
                advanced: true,
                scopes: vec![SettingScopeKind::App],
            },
            SettingDef {
                key: INBOX_MAX_ITEMS.to_owned(),
                kind: SettingKind::Integer {
                    min: Some(1),
                    max: Some(100_000),
                },
                default: json!(DEFAULT_INBOX_MAX_ITEMS),
                label_key: format!("settings.{INBOX_MAX_ITEMS}.label"),
                description_key: Some(format!("settings.{INBOX_MAX_ITEMS}.description")),
                advanced: true,
                scopes: vec![SettingScopeKind::App],
            },
            SettingDef {
                key: INBOX_MAX_KIB.to_owned(),
                kind: SettingKind::Integer {
                    min: Some(16),
                    max: Some(1_048_576),
                },
                default: json!(DEFAULT_INBOX_MAX_KIB),
                label_key: format!("settings.{INBOX_MAX_KIB}.label"),
                description_key: Some(format!("settings.{INBOX_MAX_KIB}.description")),
                advanced: true,
                scopes: vec![SettingScopeKind::App],
            },
            SettingDef {
                key: INBOX_DAILY_MAX.to_owned(),
                kind: SettingKind::Integer {
                    min: Some(1),
                    max: Some(100_000),
                },
                default: json!(DEFAULT_INBOX_DAILY_MAX),
                label_key: format!("settings.{INBOX_DAILY_MAX}.label"),
                description_key: Some(format!("settings.{INBOX_DAILY_MAX}.description")),
                advanced: true,
                scopes: vec![SettingScopeKind::App],
            },
        ],
        ui: vec![main_menu(
            "local_api.screen",
            "nav.local_api",
            "link",
            "/local-api",
            82,
        )],
    }
}

/// Registers the module and its methods.
pub fn register(reg: &mut Registry) {
    reg.module(descriptor());
    reg.provide(Arc::new(LocalApiGrants::default()));
    reg.query::<LocalApiClientsList>(list);
    reg.query::<LocalApiMethods>(methods);
    reg.command::<LocalApiClientsApprove>(approve);
    reg.command::<LocalApiClientsDeny>(deny);
    reg.command::<LocalApiClientsRevoke>(revoke);
    reg.command::<LocalApiClientsRemove>(remove);
    reg.command::<LocalApiPair>(pair);
    reg.command::<LocalApiTouch>(touch);
    reg.command::<LocalApiExpirePending>(expire_pending_handler);
}

// ------------------------------------------------------------------ conversions

fn kind_str(k: ApiClientKind) -> &'static str {
    match k {
        ApiClientKind::BrowserExtension => "browser_extension",
        ApiClientKind::App => "app",
        ApiClientKind::Script => "script",
    }
}

fn status_str(s: ApiClientStatus) -> &'static str {
    match s {
        ApiClientStatus::Pending => "pending",
        ApiClientStatus::Approved => "approved",
        ApiClientStatus::Denied => "denied",
        ApiClientStatus::Revoked => "revoked",
    }
}

fn internal(what: &str) -> AppError {
    AppError::api(ErrorCode::Internal, format!("bad api_clients row: {what}"))
}

fn dto(r: &ApiClientRecord) -> AppResult<ApiClientDto> {
    let kind = match r.kind.as_str() {
        "browser_extension" => ApiClientKind::BrowserExtension,
        "app" => ApiClientKind::App,
        "script" => ApiClientKind::Script,
        _ => return Err(internal("kind")),
    };
    let status = match r.status.as_str() {
        "pending" => ApiClientStatus::Pending,
        "approved" => ApiClientStatus::Approved,
        "denied" => ApiClientStatus::Denied,
        "revoked" => ApiClientStatus::Revoked,
        _ => return Err(internal("status")),
    };
    Ok(ApiClientDto {
        id: r.id,
        name: r.name.clone(),
        kind,
        origin: r.origin.clone(),
        requested_scopes: r.requested_scopes.clone(),
        scopes: r.scopes.clone(),
        status,
        created_at: r.created_at,
        updated_at: r.updated_at,
        last_used_at: r.last_used_at,
    })
}

fn require(reads: &dyn Reads, id: ApiClientId) -> AppResult<ApiClientRecord> {
    reads.get_api_client(id)?.ok_or_else(|| {
        err_id(
            ErrorCode::NotFoundApiClient,
            format!("local API client {id} not found"),
            id,
        )
    })
}

fn expired(r: &ApiClientRecord, now: Timestamp) -> bool {
    r.status == "pending" && r.created_at.0.saturating_add(PAIRING_TTL_MS) < now.0
}

fn state_conflict(r: &ApiClientRecord, now: Timestamp) -> AppError {
    let status = if expired(r, now) {
        "expired"
    } else {
        r.status.as_str()
    };
    AppError::Api(
        ApiError::new(
            ErrorCode::ConflictApiClientState,
            format!("local API client {} is {status}", r.id),
        )
        .with_detail("id", r.id.to_string())
        .with_detail("status", status),
    )
}

fn scope_error(scope: &str, message: &str) -> AppError {
    AppError::Api(
        ApiError::new(ErrorCode::ValidationApiScope, format!("{message}: {scope}"))
            .with_detail("scope", scope),
    )
}

fn client_error(field: &str, message: &str) -> AppError {
    AppError::Api(ApiError::new(ErrorCode::ValidationApiClient, message).with_detail("field", field))
}

/// Requested scopes, checked and normalised (sorted, unique).
fn checked_scopes(scopes: &[String]) -> AppResult<Vec<String>> {
    if scopes.is_empty() || scopes.len() > GRANTABLE_SCOPES.len() {
        return Err(scope_error("", "ask for at least one scope"));
    }
    let mut out = BTreeSet::new();
    for s in scopes {
        if !GRANTABLE_SCOPES.contains(&s.as_str()) {
            return Err(scope_error(s, "scope cannot be granted to a local API client"));
        }
        out.insert(s.clone());
    }
    Ok(out.into_iter().collect())
}

pub(crate) fn port_of(reads: &dyn Reads) -> AppResult<u16> {
    Ok(resolve_stored(reads, &SettingScope::App, PORT)?
        .and_then(|(v, _)| v.as_u64())
        .and_then(|p| u16::try_from(p).ok())
        .unwrap_or(DEFAULT_PORT))
}

/// What one client may have waiting in the inbox: (items, bytes), from the settings.
pub(crate) fn inbox_quota(reads: &dyn Reads) -> AppResult<(u32, u64)> {
    let get = |key: &str| -> AppResult<Option<u64>> {
        Ok(resolve_stored(reads, &SettingScope::App, key)?.and_then(|(v, _)| v.as_u64()))
    };
    let items = get(INBOX_MAX_ITEMS)?
        .and_then(|n| u32::try_from(n).ok())
        .unwrap_or(DEFAULT_INBOX_MAX_ITEMS);
    let kib = get(INBOX_MAX_KIB)?.unwrap_or(DEFAULT_INBOX_MAX_KIB);
    Ok((items, kib.saturating_mul(1024)))
}

/// [`INBOX_DAILY_MAX`]: proposals one client may send per 24 hours.
pub(crate) fn inbox_daily_max(reads: &dyn Reads) -> AppResult<u32> {
    Ok(resolve_stored(reads, &SettingScope::App, INBOX_DAILY_MAX)?
        .and_then(|(v, _)| v.as_u64())
        .and_then(|n| u32::try_from(n).ok())
        .unwrap_or(DEFAULT_INBOX_DAILY_MAX))
}

// ------------------------------------------------------------------ queries

fn list(_req: Empty, ctx: &QueryCtx<'_>) -> AppResult<ApiClientList> {
    let clients = ctx
        .tx
        .list_api_clients()?
        .iter()
        .filter(|c| !expired(c, ctx.now))
        .map(dto)
        .collect::<AppResult<_>>()?;
    Ok(ApiClientList {
        clients,
        grantable_scopes: GRANTABLE_SCOPES.iter().map(|s| (*s).to_owned()).collect(),
        port: port_of(ctx.tx)?,
    })
}

/// The allow-list as the registry describes it.
pub fn method_list(registry: &Registry) -> LocalApiMethodList {
    LocalApiMethodList {
        methods: HTTP_METHODS
            .iter()
            .filter_map(|n| registry.descriptor(n))
            .map(|d| LocalApiMethod {
                name: d.name.to_owned(),
                kind: d.kind,
                permission: d.permission.to_owned(),
            })
            .collect(),
    }
}

fn methods(_req: Empty, ctx: &QueryCtx<'_>) -> AppResult<LocalApiMethodList> {
    Ok(method_list(ctx.registry))
}

// ------------------------------------------------------------------ commands

fn outcome<R>(response: R, summary: fc_api::v1::common::I18nMessage, events: Vec<DomainEvent>) -> CommandOutcome<R> {
    CommandOutcome {
        response,
        inverse: vec![],
        summary,
        events,
    }
}

fn pair(req: RecordPairing, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<ApiClientDto>> {
    let name = req.name.trim();
    if name.is_empty() || name.chars().count() > 60 || name.chars().any(char::is_control) {
        return Err(client_error("client_name", "client name must be 1–60 printable characters"));
    }
    if req.token_hash.len() != 64
        || !req
            .token_hash
            .chars()
            .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
    {
        return Err(client_error("token_hash", "token hash must be 64 lowercase hex digits"));
    }
    if let Some(o) = &req.origin {
        if !is_extension_origin(o) {
            return Err(client_error("origin", "only browser extensions may pair from a browser"));
        }
    }
    let scopes = checked_scopes(&req.scopes)?;
    // Forget requests nobody decided in time. A client asking again replaces its own earlier
    // request (same name and origin); when the queue is full the oldest request makes room, so
    // a flood of requests can push out a real one but can never lock pairing.
    let mut pending = Vec::new();
    for c in ctx.reads().list_api_clients()? {
        if expired(&c, ctx.now) {
            ctx.tx.delete_api_client(c.id)?;
        } else if c.status == "pending" {
            if c.name == name && c.origin == req.origin {
                ctx.tx.delete_api_client(c.id)?;
            } else {
                pending.push(c);
            }
        }
    }
    pending.sort_by_key(|c| (c.created_at, c.id));
    let evict = (pending.len() + 1).saturating_sub(MAX_PENDING);
    for c in pending.iter().take(evict) {
        ctx.tx.delete_api_client(c.id)?;
    }
    let r = ApiClientRecord {
        id: ApiClientId::generate(ctx.ids),
        name: name.to_owned(),
        kind: kind_str(req.kind).to_owned(),
        origin: req.origin,
        requested_scopes: scopes,
        scopes: vec![],
        status: "pending".into(),
        token_hash: req.token_hash,
        created_at: ctx.now,
        updated_at: ctx.now,
        last_used_at: None,
    };
    ctx.tx.upsert_api_client(&r)?;
    let client = dto(&r)?;
    Ok(outcome(
        client.clone(),
        summary::<LocalApiPair>(),
        vec![DomainEvent::LocalApiPairing { client }],
    ))
}

/// The HTTP server starts with no memory of the tokens it handed out: requests still pending
/// cannot be completed any more, so they are dropped rather than left to be approved.
fn expire_pending_handler(_req: Empty, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<Empty>> {
    let mut any = false;
    for c in ctx.reads().list_api_clients()? {
        if c.status == "pending" {
            ctx.tx.delete_api_client(c.id)?;
            any = true;
        }
    }
    let events = if any {
        vec![DomainEvent::LocalApiClientsChanged]
    } else {
        vec![]
    };
    Ok(outcome(Empty {}, summary::<LocalApiExpirePending>(), events))
}

fn approve(
    req: ApproveApiClient,
    ctx: &mut CommandCtx<'_>,
) -> AppResult<CommandOutcome<ApiClientDto>> {
    let old = require(ctx.reads(), req.id)?;
    if old.status != "pending" || expired(&old, ctx.now) {
        return Err(state_conflict(&old, ctx.now));
    }
    let scopes = match &req.scopes {
        None => old.requested_scopes.clone(),
        Some(s) => {
            let s: BTreeSet<String> = s.iter().cloned().collect();
            if let Some(extra) = s.iter().find(|x| !old.requested_scopes.contains(x)) {
                return Err(scope_error(extra, "the client did not ask for this scope"));
            }
            if s.is_empty() {
                return Err(scope_error("", "grant at least one scope (or deny the client)"));
            }
            s.into_iter().collect()
        }
    };
    let r = ApiClientRecord {
        scopes,
        status: "approved".into(),
        updated_at: ctx.now,
        ..old
    };
    ctx.tx.upsert_api_client(&r)?;
    Ok(outcome(
        dto(&r)?,
        summary::<LocalApiClientsApprove>(),
        vec![DomainEvent::LocalApiClientsChanged],
    ))
}

fn set_status(
    ctx: &mut CommandCtx<'_>,
    id: ApiClientId,
    from: &[&str],
    to: ApiClientStatus,
) -> AppResult<ApiClientDto> {
    let old = require(ctx.reads(), id)?;
    if !from.contains(&old.status.as_str()) || (old.status == "pending" && expired(&old, ctx.now))
    {
        return Err(state_conflict(&old, ctx.now));
    }
    let r = ApiClientRecord {
        scopes: vec![],
        status: status_str(to).to_owned(),
        updated_at: ctx.now,
        ..old
    };
    ctx.tx.upsert_api_client(&r)?;
    // Stop at once (a rolled-back transaction only costs a re-read on the next request).
    if let Some(g) = grants(ctx.registry) {
        g.set(&id.to_string(), None);
    }
    dto(&r)
}

fn deny(req: ApiClientRef, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<ApiClientDto>> {
    let d = set_status(ctx, req.id, &["pending"], ApiClientStatus::Denied)?;
    Ok(outcome(
        d,
        summary::<LocalApiClientsDeny>(),
        vec![DomainEvent::LocalApiClientsChanged],
    ))
}

fn revoke(req: ApiClientRef, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<ApiClientDto>> {
    let d = set_status(ctx, req.id, &["approved"], ApiClientStatus::Revoked)?;
    Ok(outcome(
        d,
        summary::<LocalApiClientsRevoke>(),
        vec![DomainEvent::LocalApiClientsChanged],
    ))
}

fn remove(req: ApiClientRef, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<ApiClientDto>> {
    let old = require(ctx.reads(), req.id)?;
    ctx.tx.delete_api_client(old.id)?;
    if let Some(g) = grants(ctx.registry) {
        g.set(&old.id.to_string(), None);
    }
    Ok(outcome(
        dto(&old)?,
        summary::<LocalApiClientsRemove>(),
        vec![DomainEvent::LocalApiClientsChanged],
    ))
}

fn touch(req: ApiClientRef, ctx: &mut CommandCtx<'_>) -> AppResult<CommandOutcome<Empty>> {
    let old = require(ctx.reads(), req.id)?;
    ctx.tx.upsert_api_client(&ApiClientRecord {
        last_used_at: Some(ctx.now),
        ..old
    })?;
    Ok(outcome(Empty {}, summary::<LocalApiTouch>(), vec![]))
}

// ------------------------------------------------------------------ transport services
//
// What `fc-http` calls. The token never reaches the app: the server hashes it.

fn unauthenticated(reason: &str) -> ApiError {
    ApiError::new(
        ErrorCode::PermissionUnauthenticated,
        format!("no usable token ({reason})"),
    )
    .with_detail("reason", reason)
}

/// `CONFLICT_MODULE_DISABLED` unless the module is on.
pub fn ensure_enabled(app: &App) -> Result<(), ApiError> {
    if app.is_module_enabled(ID) {
        Ok(())
    } else {
        Err(ApiError::new(ErrorCode::ConflictModuleDisabled, "module local_api is disabled")
            .with_detail("module", ID))
    }
}

fn read_client(
    app: &App,
    f: impl FnOnce(&dyn Reads) -> fc_storage::StorageResult<Option<ApiClientRecord>>,
) -> Result<Option<ApiClientRecord>, ApiError> {
    let tx = app.store().read().map_err(crate::error::storage_to_api)?;
    f(&*tx).map_err(crate::error::storage_to_api)
}

/// The approved client whose token hashes to `token_hash`, used from `origin` (the request's
/// `Origin` header). Refreshes the client's grants for the registry.
///
/// * unknown hash → `PERMISSION_UNAUTHENTICATED` (`invalid`); pending, denied, revoked → the
///   same with that reason (an expired pending one is `invalid`);
/// * an `Origin` other than the one it paired from → `PERMISSION_ORIGIN_NOT_ALLOWED`; no
///   `Origin` (non-browser callers, extension GETs) is fine.
pub fn authenticate(
    app: &App,
    token_hash: &str,
    origin: Option<&str>,
) -> Result<ApiClientDto, ApiError> {
    ensure_enabled(app)?;
    let rec = read_client(app, |r| r.api_client_by_token_hash(token_hash))?
        .ok_or_else(|| unauthenticated("invalid"))?;
    let now = app.now();
    match rec.status.as_str() {
        "approved" => {}
        "pending" if expired(&rec, now) => return Err(unauthenticated("invalid")),
        other => return Err(unauthenticated(other)),
    }
    check_origin(rec.origin.as_deref(), origin)?;
    if let Some(g) = grants(app.registry()) {
        g.set(&rec.id.to_string(), Some(rec.scopes.iter().cloned().collect()));
    }
    dto(&rec).map_err(AppError::into_api)
}

/// A request from `origin` may use a client paired from `paired`: same origin, or none.
pub fn check_origin(paired: Option<&str>, origin: Option<&str>) -> Result<(), ApiError> {
    match origin {
        None => Ok(()),
        Some(o) if paired == Some(o) => Ok(()),
        Some(o) => Err(ApiError::new(
            ErrorCode::PermissionOriginNotAllowed,
            "this token may not be used from this origin",
        )
        .with_detail("origin", o)),
    }
}

/// The actor of a client.
pub fn actor(client: &ApiClientDto) -> Actor {
    Actor::LocalApi(client.id.to_string())
}

/// Calls `method` (unversioned) for an authenticated client. Methods off the allow-list are
/// `NOT_FOUND_METHOD` (their existence is not disclosed); the registry then checks the
/// client's scopes and the owning module's switch.
pub fn call(
    app: &App,
    client: &ApiClientDto,
    method: &str,
    params: Value,
) -> Result<Value, ApiError> {
    if !method_reachable(method) {
        return Err(ApiError::new(ErrorCode::NotFoundMethod, format!("unknown method {method}"))
            .with_detail("method", method));
    }
    app.invoke(&format!("{}.{method}", fc_api::API_VERSION), params, &actor(client))
}

/// `POST /v1/inbox`: adds an item; `true` when it was created now, `false` when an earlier
/// request with the same `client_ref` made it (idempotent resend).
pub fn add_inbox_item(
    app: &App,
    client: &ApiClientDto,
    item: AddInboxItem,
) -> Result<(InboxItemDto, bool), ApiError> {
    let existing = match item.client_ref.as_deref() {
        Some(r) => {
            let tx = app.store().read().map_err(crate::error::storage_to_api)?;
            tx.inbox_item_by_client_ref(client.id, r)
                .map_err(crate::error::storage_to_api)?
        }
        None => None,
    };
    let params = serde_json::to_value(&item)
        .map_err(|e| ApiError::new(ErrorCode::ValidationInvalidParams, e.to_string()))?;
    let v = call(app, client, InboxAdd::NAME, params)?;
    let dto: InboxItemDto =
        serde_json::from_value(v).map_err(|e| ApiError::new(ErrorCode::Internal, e.to_string()))?;
    Ok((dto, existing.is_none()))
}

/// Records a pairing request (`POST /v1/pair`).
pub fn record_pairing(
    app: &App,
    req: &PairRequest,
    origin: Option<&str>,
    token_hash: &str,
) -> Result<ApiClientDto, ApiError> {
    ensure_enabled(app)?;
    let params = serde_json::to_value(RecordPairing {
        name: req.client_name.clone(),
        kind: req.client_kind,
        scopes: req.scopes.clone(),
        origin: origin.map(str::to_owned),
        token_hash: token_hash.to_owned(),
    })
    .map_err(|e| ApiError::new(ErrorCode::Internal, e.to_string()))?;
    let v = app.invoke_trusted(
        &InvokeRequest {
            method: format!("{}.{}", fc_api::API_VERSION, LocalApiPair::NAME),
            params,
        },
        &Actor::System,
    )?;
    serde_json::from_value(v).map_err(|e| ApiError::new(ErrorCode::Internal, e.to_string()))
}

/// A client by id (pairing status polls); expired pending clients read as `None`.
pub fn client(app: &App, id: ApiClientId) -> Result<Option<ApiClientDto>, ApiError> {
    let now = app.now();
    read_client(app, |r| r.get_api_client(id))?
        .filter(|r| !expired(r, now))
        .map(|r| dto(&r).map_err(AppError::into_api))
        .transpose()
}

/// Drops every pending pairing request (the HTTP server calls this when it starts).
pub fn expire_pending(app: &App) -> Result<(), ApiError> {
    app.invoke_trusted(
        &InvokeRequest {
            method: format!("{}.{}", fc_api::API_VERSION, LocalApiExpirePending::NAME),
            params: json!({}),
        },
        &Actor::System,
    )
    .map(|_| ())
}

/// Notes a client's request time (the server calls this at most once a minute per client).
pub fn touch_client(app: &App, id: ApiClientId) -> Result<(), ApiError> {
    let params = serde_json::to_value(ApiClientRef { id })
        .map_err(|e| ApiError::new(ErrorCode::Internal, e.to_string()))?;
    app.invoke_trusted(
        &InvokeRequest {
            method: format!("{}.{}", fc_api::API_VERSION, LocalApiTouch::NAME),
            params,
        },
        &Actor::System,
    )
    .map(|_| ())
}

/// The configured port.
pub fn port(app: &App) -> Result<u16, ApiError> {
    let tx = app.store().read().map_err(crate::error::storage_to_api)?;
    port_of(&*tx).map_err(AppError::into_api)
}

/// Restoring a backup revokes every local API client (a crafted backup must not bring an
/// approved token with it): approved rows become `revoked` with no scopes.
pub(crate) fn forget_clients(dump: &mut fc_storage::repo::CollectionDump) {
    use fc_storage::repo::SqlValue;
    for t in dump.tables.iter_mut().filter(|t| t.name == "api_clients") {
        let col = |n: &str| t.columns.iter().position(|c| c == n);
        let (Some(status), Some(scopes)) = (col("status"), col("scopes")) else {
            continue;
        };
        for row in &mut t.rows {
            if matches!(&row[status], SqlValue::Text(s) if s == "approved") {
                row[status] = SqlValue::Text("revoked".into());
            }
            row[scopes] = SqlValue::Text("[]".into());
        }
    }
}
