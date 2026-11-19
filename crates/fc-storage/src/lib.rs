//! # fc-storage
//!
//! SQLite persistence for Flashcard.
//!
//! * [`migrate`]: numbered SQL migrations (`migrations/NNNN_name.sql`), `user_version`-based.
//! * [`repo`]: repository traits — the **only** interface `fc-app` uses.
//! * [`store`]: [`Store`] / [`ReadTx`] / [`WriteTx`] handles and the [`SqliteStore`]
//!   implementation.
//!
//! Storage holds no business rules: it persists and loads `fc-core` models faithfully.
#![forbid(unsafe_code)]

pub mod error;
pub mod migrate;
pub mod repo;
pub mod store;

pub use error::{StorageError, StorageResult};
pub use repo::{Reads, Writes};
pub use store::{ReadTx, SqliteStore, Store, WriteTx};

#[cfg(test)]
mod tests;
