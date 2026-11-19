//! # fc-api
//!
//! The versioned, transport-independent API contract (docs/03 §7):
//!
//! * [`v1`] DTOs and the method catalog ([`v1::catalog`]) — names like `decks.create`,
//!   `attributes.option.rename`, `oplog.undo`, `modules.list`;
//! * the error model ([`ApiError`], [`ErrorCode`]);
//! * transport envelopes ([`InvokeRequest`], [`InvokeResponse`]) and events
//!   ([`v1::events::EventEnvelope`]);
//! * TypeScript generation ([`tsgen`], binary `gen-ts`).
//!
//! This crate contains **no behavior**: `fc-app` implements a handler for every catalog entry
//! and transports (Tauri, HTTP, MCP, CLI) only move these shapes around.
#![forbid(unsafe_code)]

pub mod envelope;
pub mod error;
pub mod method;
pub mod tsgen;
pub mod v1;

pub use envelope::{InvokeRequest, InvokeResponse};
pub use error::{ApiError, ErrorCategory, ErrorCode};
pub use method::{ApiMethod, MethodDescriptor, MethodKind, Visibility};

/// Current API version prefix.
pub const API_VERSION: &str = "v1";

#[cfg(test)]
mod tests;
