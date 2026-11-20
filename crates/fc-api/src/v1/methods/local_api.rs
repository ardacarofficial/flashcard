//! Local HTTP API clients (v0.9). Security decisions are not undo steps (`undo: false`): an
//! undo must never revive a revoked client or a denied pairing.

use crate::v1::common::Empty;
use crate::v1::local_api::*;

methods! {
    // ---- local_api ----
    /// Clients (pending ones included), grantable scopes and the port.
    LocalApiClientsList = "local_api.clients.list", Query, "local_api", "local_api:read", Public, undo: false, Empty => ApiClientList;
    /// Approves a pending client with (a subset of) the scopes it asked for.
    LocalApiClientsApprove = "local_api.clients.approve", Command, "local_api", "local_api:write", Public, undo: false, ApproveApiClient => ApiClientDto;
    /// Refuses a pending client.
    LocalApiClientsDeny = "local_api.clients.deny", Command, "local_api", "local_api:write", Public, undo: false, ApiClientRef => ApiClientDto;
    /// Revokes an approved client (its token stops working at once).
    LocalApiClientsRevoke = "local_api.clients.revoke", Command, "local_api", "local_api:write", Public, undo: false, ApiClientRef => ApiClientDto;
    /// Deletes a denied or revoked client's record.
    LocalApiClientsRemove = "local_api.clients.remove", Command, "local_api", "local_api:write", Public, undo: false, ApiClientRef => ApiClientDto;
    /// Methods a local API client may call over HTTP (the allow-list).
    LocalApiMethods = "local_api.methods", Query, "local_api", "local_api:read", Public, undo: false, Empty => LocalApiMethodList;
    /// Internal: records a pairing request (HTTP server). Never an inverse: its inverse is
    /// empty, so it never enters the oplog (`replay: false`).
    LocalApiPair = "local_api.pair", Command, "local_api", "local_api:write", Internal, undo: true (replay: false), RecordPairing => ApiClientDto;
    /// Internal: notes a client's last request (HTTP server, at most once a minute). Like
    /// `local_api.pair`, never in the oplog.
    LocalApiTouch = "local_api.touch", Command, "local_api", "local_api:write", Internal, undo: true (replay: false), ApiClientRef => Empty;
    /// Internal: forgets every pending pairing request (HTTP server, when it starts: the
    /// tokens it handed out only lived in its memory). Never in the oplog.
    LocalApiExpirePending = "local_api.expire_pending", Command, "local_api", "local_api:write", Internal, undo: true (replay: false), Empty => Empty;
}
