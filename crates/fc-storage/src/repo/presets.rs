//! Option presets.

use super::*;

/// Preset reads.
pub trait PresetRead {
    /// Preset by id.
    fn get_preset(&self, id: PresetId) -> StorageResult<Option<Preset>>;
    /// All presets.
    fn list_presets(&self) -> StorageResult<Vec<Preset>>;
}

/// Preset writes.
pub trait PresetWrite: PresetRead {
    /// Inserts or replaces.
    fn upsert_preset(&mut self, preset: &Preset) -> StorageResult<()>;
}
