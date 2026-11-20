//! The query language (docs/02 §9): one engine for search, saved filters, custom study, bulk
//! operations, statistics filters and automations.
//!
//! Pipeline: [`parse_query`] (text → [`Expr`], purely syntactic) → [`resolve`] (names → ids
//! against a [`Catalog`] snapshot; positioned [`QueryError`]s) → [`Filter`]. Storage compiles a
//! `Filter` to SQL; [`eval::matches`] is the in-memory reference semantics. Key reference:
//! [`resolve`] module docs.

pub mod ast;
pub mod canonical;
pub mod error;
pub mod eval;
pub mod filter;
pub mod parse;
pub mod resolve;
pub mod text;

pub use ast::{CmpOp, Expr, Span, Term};
pub use error::{QueryError, QueryErrorKind};
pub use filter::{AttrAlt, AttrTest, CardNum, Filter, Pred, TimeCol};
pub use parse::parse_query;
pub use resolve::{compile_query, resolve, Catalog};

use crate::id::SavedQueryId;
use crate::time::Timestamp;
use serde::{Deserialize, Serialize};

/// A named query shown in the sidebar (`saved_queries` row).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedQuery {
    /// Id.
    pub id: SavedQueryId,
    /// Display name (user text).
    pub name: String,
    /// Query text.
    pub query: String,
    /// Optional icon key.
    pub icon: Option<String>,
    /// Sort position.
    pub position: i64,
    /// Creation time.
    pub created_at: Timestamp,
    /// Last modification time.
    pub updated_at: Timestamp,
}

/// Maximum length of a saved query name (chars).
pub const MAX_SAVED_QUERY_NAME: usize = 100;

/// Trims and validates a saved query name (`None` if empty or too long).
pub fn normalize_saved_query_name(raw: &str) -> Option<String> {
    let name = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    (!name.is_empty() && name.chars().count() <= MAX_SAVED_QUERY_NAME).then_some(name)
}

#[cfg(test)]
mod tests;
