//! # fc-app
//!
//! Service layer (docs/03 §5–§14). Everything that *does* something lives here; transports only
//! call [`App::invoke`] and subscribe to [`App::events`].
//!
//! * [`handler`]: `CommandHandler` / `QueryHandler` traits, contexts, the inverse (undo)
//!   contract.
//! * [`registry`]: method registry filled by [`modules`] (one file per module).
//! * [`app`]: [`App`], the dispatcher (transactions, oplog, events).
//! * [`events`], [`jobs`], [`settings`], [`module`]: event bus, job queue, settings schema and
//!   module descriptor contracts.
//! * [`system`]: the only place reading the system clock and OS randomness.
#![forbid(unsafe_code)]

pub mod app;
pub mod error;
pub mod events;
pub mod handler;
pub mod jobs;
pub mod module;
pub mod modules;
pub mod registry;
pub mod settings;
pub mod system;

pub use app::{App, AppDeps};
pub use error::{AppError, AppResult};
pub use handler::{
    inverse_call, CommandCtx, CommandHandler, CommandOutcome, QueryCtx, QueryHandler,
};
pub use registry::Registry;

/// Provider/strategy interfaces, re-exported for transports and modules.
pub use fc_providers as providers;

#[cfg(test)]
mod tests;
