//! Plugins (v0.7). Plugin management needs `plugins:*`, which is never grantable to plugins.

use crate::v1::common::Empty;
use crate::v1::plugins::*;

methods! {
    // ---- plugins: management ----
    /// Installed plugins.
    PluginsList = "plugins.list", Query, "plugins", "plugins:read", Public, undo: false, Empty => PluginList;
    /// Reads a package file (install dialog: what it is, what it asks for).
    PluginsInspect = "plugins.inspect", Query, "plugins", "plugins:read", Public, undo: false, InspectPackage => PackageInfo;
    /// Installs or updates a plugin from a package file with the approved permissions.
    PluginsInstall = "plugins.install", Command, "plugins", "plugins:write", Public, undo: true, InstallPlugin => PluginDto;
    /// Uninstalls a plugin (optionally deleting its private data; notes it made stay).
    PluginsUninstall = "plugins.uninstall", Command, "plugins", "plugins:write", Public, undo: true, UninstallPlugin => PluginDto;
    /// Switches a plugin on/off (on also clears an automatic disable).
    PluginsSetEnabled = "plugins.set_enabled", Command, "plugins", "plugins:write", Public, undo: true, SetPluginEnabled => PluginDto;
    /// Grants/revokes permissions.
    PluginsSetPermissions = "plugins.set_permissions", Command, "plugins", "plugins:write", Public, undo: true, SetPluginPermissions => PluginDto;
    /// A plugin's settings schema and values.
    PluginsSettingsGet = "plugins.settings.get", Query, "plugins", "plugins:read", Public, undo: false, PluginRef => PluginSettings;
    /// Sets (or resets) one plugin setting.
    PluginsSettingsSet = "plugins.settings.set", Command, "plugins", "plugins:write", Public, undo: true, SetPluginSetting => PluginSettings;
    /// v1.4: the decks a plugin may learn words from (`words:read`; `null` = every deck).
    PluginsWordDecksGet = "plugins.word_decks.get", Query, "plugins", "plugins:read", Public, undo: false, PluginRef => PluginWordDecks;
    /// v1.4: limits (or unlimits) the decks a plugin may learn words from.
    PluginsWordDecksSet = "plugins.word_decks.set", Command, "plugins", "plugins:write", Public, undo: true, SetPluginWordDecks => PluginWordDecks;
    /// Translations of active plugins.
    PluginsLocales = "plugins.locales", Query, "plugins", "plugins:read", Public, undo: false, GetPluginLocales => PluginLocales;
    /// A plugin's log channel (this run).
    PluginsLogs = "plugins.logs", Query, "plugins", "plugins:read", Public, undo: false, PluginRef => PluginLogs;
    // ---- plugins: running contributions (plugin code runs outside the write transaction;
    // what it writes is recorded as one undo step) ----
    /// Runs a contributed command.
    PluginsRun = "plugins.run", Command, "plugins", "plugins:write", Public, undo: false, RunPluginCommand => PluginCommandResult;
    /// Opens a view or study mode.
    PluginsUiOpen = "plugins.ui.open", Command, "plugins", "plugins:write", Public, undo: false, OpenPluginUi => UiSession;
    /// Sends an action to an open view / study mode.
    PluginsUiAct = "plugins.ui.act", Command, "plugins", "plugins:write", Public, undo: false, ActPluginUi => UiSession;
    /// Closes a view / study mode session.
    PluginsUiClose = "plugins.ui.close", Command, "plugins", "plugins:write", Public, undo: false, ClosePluginUi => Empty;
    // ---- plugins: model download manager ----
    /// Models declared by active plugins, with download state and disk usage.
    PluginsModelsList = "plugins.models.list", Query, "plugins", "plugins:read", Public, undo: false, Empty => ModelList;
    /// Downloads (or resumes) a model in the background (SHA-256 verified).
    PluginsModelsDownload = "plugins.models.download", Command, "plugins", "plugins:write", Public, undo: false, ModelRef => ModelDownload;
    /// Deletes a model's files.
    PluginsModelsDelete = "plugins.models.delete", Command, "plugins", "plugins:write", Public, undo: false, ModelRef => ModelDto;
    // ---- plugins: internal ----
    /// Internal inverse: puts a plugin row (and optionally its data) back exactly.
    PluginsRestore = "plugins.restore", Command, "plugins", "plugins:write", Internal, undo: true, PluginSnapshot => Empty;
    /// Internal: plugin private storage write (host `kv.set` / `kv.delete`). Bookkeeping: its
    /// inverse is empty, so it never becomes an undo step of its own.
    PluginsKvSet = "plugins.kv.set", Command, "plugins", "plugins:write", Internal, undo: true, SetPluginKv => Empty;
    /// Internal: failure bookkeeping and automatic disable.
    PluginsRecordFailure = "plugins.record_failure", Command, "plugins", "plugins:write", Internal, undo: true, RecordPluginFailure => Empty;
    /// Internal: rolls back what an interrupted plugin operation wrote (crash recovery of its
    /// undo group; `calls` are the inverses, newest first).
    PluginsRollback = "plugins.rollback", Command, "plugins", "plugins:write", Internal, undo: true, RollbackPluginOp => Empty;
}
