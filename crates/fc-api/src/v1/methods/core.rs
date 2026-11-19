//! Core: undo/redo, batch, modules, method registry, settings, jobs.

use crate::v1::common::*;
use crate::v1::jobs::*;
use crate::v1::modules::*;
use crate::v1::oplog::*;
use crate::v1::settings::*;

methods! {
    // ---- core: undo/redo, batch ----
    /// Reverts the latest undoable batch.
    OplogUndo = "oplog.undo", Command, "core", "oplog:write", Public, undo: false, Empty => UndoResult;
    /// Re-applies the latest undone batch.
    OplogRedo = "oplog.redo", Command, "core", "oplog:write", Public, undo: false, Empty => UndoResult;
    /// What undo/redo would do now.
    OplogStatus = "oplog.status", Query, "core", "oplog:read", Public, undo: false, Empty => UndoStatus;
    /// Operation history (newest first).
    OplogList = "oplog.list", Query, "core", "oplog:read", Public, undo: false, ListOplog => OplogPage;
    /// Runs several commands atomically as one undo step (its sub-calls are what the oplog
    /// records, so the batch itself is `undo: false`).
    BatchExecute = "batch.execute", Command, "core", "batch:write", Public, undo: false, ExecuteBatch => BatchResult;

    // ---- core: modules, method registry, settings, jobs ----
    /// Modules with their UI contributions (frontend menus are drawn from this).
    ModulesList = "modules.list", Query, "core", "modules:read", Public, undo: false, Empty => ModuleList;
    /// Enables/disables a module.
    ModulesSetEnabled = "modules.set_enabled", Command, "core", "modules:write", Public, undo: true, SetModuleEnabled => ModuleDto;
    /// All methods (command palette, MCP tool list, docs).
    CommandsList = "commands.list", Query, "core", "modules:read", Public, undo: false, Empty => MethodList;
    /// Setting schemas of enabled modules.
    SettingsSchemaGet = "settings.schema", Query, "core", "settings:read", Public, undo: false, Empty => SettingsSchema;
    /// Resolved setting values.
    SettingsGet = "settings.get", Query, "core", "settings:read", Public, undo: false, GetSettings => SettingValues;
    /// Sets a setting at a scope.
    SettingsSet = "settings.set", Command, "core", "settings:write", Public, undo: true, SetSetting => SettingValueDto;
    /// Removes a setting override at a scope.
    SettingsReset = "settings.reset", Command, "core", "settings:write", Public, undo: true, ResetSetting => SettingValueDto;
    /// Background jobs.
    JobsList = "jobs.list", Query, "core", "jobs:read", Public, undo: false, Empty => JobList;
    /// One job.
    JobsGet = "jobs.get", Query, "core", "jobs:read", Public, undo: false, JobRef => JobDto;
    /// Requests cooperative cancellation.
    JobsCancel = "jobs.cancel", Command, "core", "jobs:write", Public, undo: false, JobRef => JobDto;
}
