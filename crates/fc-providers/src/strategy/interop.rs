use super::StrategyResult;
use crate::provider::ProviderInfo;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Options chosen in the import preview (delimiter, field mapping…). Format-specific.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ImportOptions {
    /// Format-specific options.
    pub options: serde_json::Value,
}

/// A note parsed from an import source, before being mapped to a note type.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ImportedNote {
    /// Column/field name → value.
    pub fields: BTreeMap<String, String>,
    /// Tags.
    pub tags: Vec<String>,
    /// Deck path, if the source has one.
    pub deck_path: Option<String>,
    /// Classification values by attribute key → label.
    pub attributes: BTreeMap<String, String>,
}

/// Parsed import data. Media and review history support arrive with v0.5.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ImportBundle {
    /// Notes in source order.
    pub notes: Vec<ImportedNote>,
    /// Non-fatal warnings (i18n keys).
    pub warnings: Vec<String>,
}

/// Data handed to an exporter.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ExportBundle {
    /// Notes to export.
    pub notes: Vec<ImportedNote>,
}

/// Import format (text/CSV, JSON, Quizlet, .apkg… — v0.5). Pure: bytes in, bundle out; the
/// app does the I/O and runs it as a job.
pub trait Importer: Send + Sync {
    /// Metadata.
    fn info(&self) -> ProviderInfo;
    /// Confidence 0.0–1.0 that `sample` is in this format (auto-detection).
    fn detect(&self, sample: &[u8]) -> f32;
    /// Parses `input`.
    fn parse(&self, input: &[u8], options: &ImportOptions) -> StrategyResult<ImportBundle>;
}

/// Export format.
pub trait Exporter: Send + Sync {
    /// Metadata.
    fn info(&self) -> ProviderInfo;
    /// File extension without dot.
    fn extension(&self) -> &'static str;
    /// Serializes `bundle`.
    fn export(&self, bundle: &ExportBundle, options: &serde_json::Value)
        -> StrategyResult<Vec<u8>>;
}
