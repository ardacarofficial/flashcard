//! Locale handling. The core holds no translation catalog: it returns message keys; only
//! built-in *data* (e.g. attribute templates) carries both TR and EN text.

use serde::{Deserialize, Serialize};

/// Supported UI locales.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub enum Locale {
    /// English.
    #[default]
    En,
    /// Turkish.
    Tr,
}

/// A piece of built-in text in every supported locale.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalizedText {
    /// English.
    pub en: &'static str,
    /// Turkish.
    pub tr: &'static str,
}

impl LocalizedText {
    /// Same text in all locales (e.g. `"A1"`).
    pub const fn same(s: &'static str) -> Self {
        Self { en: s, tr: s }
    }

    /// Text for `locale`.
    pub const fn get(&self, locale: Locale) -> &'static str {
        match locale {
            Locale::En => self.en,
            Locale::Tr => self.tr,
        }
    }
}
