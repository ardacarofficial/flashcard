//! Module switches.

use super::*;

/// Module switch reads.
pub trait ModuleStateRead {
    /// Stored state of every module that has one (absent = module default).
    fn list_module_states(&self) -> StorageResult<Vec<(String, bool)>>;
}

/// Module switch writes.
pub trait ModuleStateWrite: ModuleStateRead {
    /// Upserts a module switch.
    fn set_module_enabled(&mut self, id: &str, enabled: bool, now: Timestamp) -> StorageResult<()>;
}
