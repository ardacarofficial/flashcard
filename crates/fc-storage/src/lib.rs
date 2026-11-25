//! # fc-storage
//!
//! SQLite persistence for Flashcard.
//!
//! * [`migrate`]: numbered SQL migrations (`migrations/NNNN_name.sql`), recorded with
//!   checksums in `schema_migrations` (mirrored in `user_version`).
//! * [`repo`]: repository traits — the **only** interface `fc-app` uses.
//! * [`store`]: [`Store`] / [`ReadTx`] / [`WriteTx`] handles and the [`SqliteStore`]
//!   implementation (connection model documented there).
//! * [`interrupt`]: stopping long statements at a caller's deadline (plugin host calls).
//! * `sqlite` (private): the SQL behind every repository trait.
//!
//! Storage holds no business rules: it persists and loads `fc-core` models faithfully.
#![forbid(unsafe_code)]

#[cfg(feature = "crash-points")]
pub mod crash_points;
pub mod error;
pub mod interrupt;
pub mod media;
pub mod migrate;
pub mod repo;
pub mod rescue;
mod sqlite;
pub mod store;

pub use error::{is_disk_full, StorageError, StorageResult};
pub use repo::{Reads, Writes};
pub use sqlite::due_load::set_due_load_cache;
pub use sqlite::{fit_dump_to_schema, upgrade_dump, upgrade_dump_with};
pub use store::{ReadTx, SqliteStore, Store, WriteTx};

#[cfg(test)]
mod tests;
