//! # fc-plugins
//!
//! The plugin system's machinery, independent of the app service (docs/05):
//!
//! * [`manifest`]: `plugin.toml` (id, version, `api_version`, permissions, contributions).
//! * [`permission`]: the permission vocabulary and its classification (API, hooks, UI,
//!   providers, sensitive ones).
//! * [`package`]: `.fcplugin` packages (zip: manifest, WASM entry, settings schema, locales).
//! * [`runtime`]: the WASM sandbox (wasmtime): **fuel** per call, a wall-clock **deadline**
//!   (epoch interruption), a **memory** cap per instance; host calls are serviced on the
//!   calling thread, so the host side may borrow anything (a write transaction included).
//! * [`abi`]: the versioned plugin ABI (exports, imports, JSON envelopes, operation names).
//! * [`source`]: [`source::PluginRegistrySource`] (store infrastructure; built-in: install from
//!   a local file, docs/08 K3).
//! * [`models`]: the model download manager (catalog entries, resumable download with SHA-256
//!   verification, disk usage, removal).
//!
//! What a host call *does* (the API surface filtered by permissions, plugin storage,
//! providers) is the app's business: it implements [`runtime::HostCalls`].
#![forbid(unsafe_code)]

pub mod abi;
pub mod manifest;
pub mod models;
pub mod package;
pub mod permission;
pub mod runtime;
pub mod source;
#[cfg(any(test, feature = "testing"))]
pub mod testing;

pub use manifest::{Contributes, Manifest, ManifestError};
pub use package::{Package, PackageError};
pub use permission::{HookPoint, Permission};
pub use runtime::{
    CompiledPlugin, HostCalls, HostError, Limits, PluginError, PluginErrorKind, Runtime,
};

/// The app version plugins are checked against (`min_app_version`): the workspace package
/// version (`[workspace.package] version` in the root `Cargo.toml`), which follows the
/// roadmap milestone.
pub const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
