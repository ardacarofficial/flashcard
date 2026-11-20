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
    /// Deletes a preset row (decks must not refer to it any more). Returns whether it existed.
    fn delete_preset(&mut self, id: PresetId) -> StorageResult<bool>;
}
