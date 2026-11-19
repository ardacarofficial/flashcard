//! Shared small DTOs.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

/// Request/response with no data (`{}`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct Empty {}

/// A translatable message: i18n key plus interpolation params. The backend never returns
/// user-facing text, only these.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
pub struct I18nMessage {
    /// i18n key, e.g. `oplog.summary.decks.create`.
    pub key: String,
    /// Interpolation params, e.g. `{"count": "50"}`.
    pub params: BTreeMap<String, String>,
}

impl I18nMessage {
    /// Message without params.
    pub fn key(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            params: BTreeMap::new(),
        }
    }

    /// Adds a param.
    pub fn param(mut self, name: &str, value: impl ToString) -> Self {
        self.params.insert(name.to_owned(), value.to_string());
        self
    }
}

pub use fc_core::attribute::EntityRef;

/// How many entities a (soft) delete or restore affected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
pub struct AffectedCount {
    /// Decks.
    pub decks: u32,
    /// Notes.
    pub notes: u32,
    /// Cards.
    pub cards: u32,
}
