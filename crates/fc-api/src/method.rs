//! Typed method descriptors: every command/query is a zero-sized type implementing
//! [`ApiMethod`], tying its wire name to its request/response types.

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Command (mutates state, recorded in the oplog, undoable) or query (read-only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum MethodKind {
    /// Mutates state. Runs in a write transaction, writes the oplog, emits events.
    Command,
    /// Read-only. Never writes.
    Query,
}

/// Who may call a method.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    /// Exposed on every transport (IPC, HTTP, MCP, plugins, CLI).
    Public,
    /// Only callable from inside the app (e.g. as an undo inverse). Transports must reject it.
    Internal,
}

/// A method of the API contract.
pub trait ApiMethod: 'static {
    /// Unversioned name, `<area>.<verb>` or `<area>.<sub>.<verb>`, e.g. `decks.create`.
    const NAME: &'static str;
    /// Command or query.
    const KIND: MethodKind;
    /// Owning module id (see `modules.list`).
    const MODULE: &'static str;
    /// Permission required (`<area>:read` / `<area>:write`); checked for plugins/local API.
    const PERMISSION: &'static str;
    /// Visibility.
    const VISIBILITY: Visibility;
    /// Commands only: the dispatcher records an oplog entry (with the handler's inverse) for
    /// this call. `false` for all queries and for meta commands that record their sub-calls
    /// instead (`batch.execute`, `oplog.undo`, `oplog.redo`) or are not undoable
    /// (`jobs.cancel`).
    const UNDOABLE: bool;
    /// Request params.
    type Request: Serialize + DeserializeOwned + TS + Send + 'static;
    /// Successful response.
    type Response: Serialize + DeserializeOwned + TS + Send + 'static;
}

/// Runtime description of a method (the registry's source of truth for names, kinds,
/// permissions; also drives TS generation).
#[derive(Clone, Copy)]
pub struct MethodDescriptor {
    /// See [`ApiMethod::NAME`].
    pub name: &'static str,
    /// See [`ApiMethod::KIND`].
    pub kind: MethodKind,
    /// See [`ApiMethod::MODULE`].
    pub module: &'static str,
    /// See [`ApiMethod::PERMISSION`].
    pub permission: &'static str,
    /// See [`ApiMethod::VISIBILITY`].
    pub visibility: Visibility,
    /// See [`ApiMethod::UNDOABLE`].
    pub undoable: bool,
    /// TS name of the request type.
    pub request_ts: fn(&ts_rs::Config) -> String,
    /// TS name of the response type.
    pub response_ts: fn(&ts_rs::Config) -> String,
    /// Exports request/response types (and dependencies) into `cfg.out_dir()`.
    pub export_ts: fn(&ts_rs::Config) -> Result<(), ts_rs::ExportError>,
}

impl MethodDescriptor {
    /// Descriptor of `M`.
    pub fn of<M: ApiMethod>() -> Self {
        fn export<M: ApiMethod>(cfg: &ts_rs::Config) -> Result<(), ts_rs::ExportError> {
            <M::Request as TS>::export_all(cfg)?;
            <M::Response as TS>::export_all(cfg)
        }
        Self {
            name: M::NAME,
            kind: M::KIND,
            module: M::MODULE,
            permission: M::PERMISSION,
            visibility: M::VISIBILITY,
            undoable: M::UNDOABLE,
            request_ts: <M::Request as TS>::name,
            response_ts: <M::Response as TS>::name,
            export_ts: export::<M>,
        }
    }

    /// Versioned wire name, e.g. `v1.decks.create`.
    pub fn qualified_name(&self) -> String {
        format!("{}.{}", crate::API_VERSION, self.name)
    }
}

impl std::fmt::Debug for MethodDescriptor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MethodDescriptor")
            .field("name", &self.name)
            .field("kind", &self.kind)
            .field("module", &self.module)
            .field("permission", &self.permission)
            .field("visibility", &self.visibility)
            .field("undoable", &self.undoable)
            .finish()
    }
}

/// Splits a qualified wire name (`v1.decks.create`) into version and method name.
/// Returns `None` if the version prefix is missing or not supported.
pub fn parse_qualified_name(wire: &str) -> Option<&str> {
    let (version, name) = wire.split_once('.')?;
    (version == crate::API_VERSION && !name.is_empty()).then_some(name)
}
