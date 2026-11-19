//! # fc-storage
//!
//! SQLite persistence for Flashcard.
//!
//! * [`migrate`]: numbered SQL migrations (`migrations/NNNN_name.sql`), recorded with
//!   checksums in `schema_migrations` (mirrored in `user_version`).
//! * [`repo`]: repository traits — the **only** interface `fc-app` uses.
//! * [`store`]: [`Store`] / [`ReadTx`] / [`WriteTx`] handles and the [`SqliteStore`]
//!   implementation (connection model documented there).
//! * `sqlite` (private): the SQL behind every repository trait.
//!
//! Storage holds no business rules: it persists and loads `fc-core` models faithfully.
#![forbid(unsafe_code)]

pub mod error;
pub mod migrate;
pub mod repo;
mod sqlite;
pub mod store;

pub use error::{StorageError, StorageResult};
pub use repo::{Reads, Writes};
pub use sqlite::{upgrade_dump, upgrade_dump_with};
pub use store::{ReadTx, SqliteStore, Store, WriteTx};

#[cfg(test)]
mod tests;
