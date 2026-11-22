//! Plugins (docs/05, v0.7): management, permissions, settings, contributed commands, the
//! declarative UI of views and study modes, and the model download manager.
//!
//! Plugin texts come from the plugin's own locale files (`plugins.locales`); the frontend
//! looks them up under `plugin.<plugin id>.<key>`.

use super::jobs::JobDto;
use fc_core::settings::SettingDef;
use fc_core::Timestamp;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use ts_rs::TS;

/// A permission as shown in the install / permissions dialog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PermissionDto {
    /// Permission (`notes:write`, `hooks:before:notes.create`, `ui:study_mode`…).
    pub permission: String,
    /// Currently granted.
    pub granted: bool,
    /// Asked separately (network, microphone, files, paid AI).
    pub sensitive: bool,
}

/// A main menu entry of a plugin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PluginMenuItem {
    /// Id (label: plugin key `menu.<id>`).
    pub id: String,
    /// View or study mode it opens.
    pub view: String,
    /// Icon key.
    pub icon: Option<String>,
}

/// A sidebar panel of a plugin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PluginPanel {
    /// Id (label: plugin key `panels.<id>`).
    pub id: String,
    /// View shown.
    pub view: String,
}

/// A card toolbar button of a plugin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PluginToolbarButton {
    /// Id (label: plugin key `toolbar.<id>`).
    pub id: String,
    /// Command it runs with `{card_id}`.
    pub command: String,
    /// Icon key.
    pub icon: Option<String>,
}

/// What a plugin adds. Lists only **active** contributions (permission granted).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, TS)]
pub struct PluginContributions {
    /// Command ids (run with `plugins.run` or `v1.plugin.<id>.<command>`).
    pub commands: Vec<String>,
    /// Study modes.
    pub study_modes: Vec<String>,
    /// Views.
    pub views: Vec<String>,
    /// Card type ids (`plugin.<id>.<type>`).
    pub card_types: Vec<String>,
    /// TTS provider ids (`plugin.<id>.<provider>`).
    pub tts: Vec<String>,
    /// Main menu entries.
    pub menu_items: Vec<PluginMenuItem>,
    /// Sidebar panels.
    pub sidebar_panels: Vec<PluginPanel>,
    /// Card toolbar buttons.
    pub card_toolbar: Vec<PluginToolbarButton>,
    /// Subscribed hooks (`before:notes.create`, `event:note_created`…).
    pub hooks: Vec<String>,
}

/// Run state of an installed plugin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum PluginStatus {
    /// Enabled and loaded.
    Active,
    /// Turned off by the user.
    Disabled,
    /// Turned off by the app after repeated failures (`last_error`); enabling resets it.
    AutoDisabled,
    /// Enabled but cannot run (module does not compile, ABI unsupported: `last_error`).
    Broken,
}

/// An installed plugin.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct PluginDto {
    /// Id (`com.example.autofill`).
    pub id: String,
    /// Name.
    pub name: String,
    /// Installed version.
    pub version: String,
    /// Description.
    pub description: String,
    /// Authors.
    pub authors: Vec<String>,
    /// Plugin API version it targets.
    pub api_version: String,
    /// Switched on by the user.
    pub enabled: bool,
    /// Run state.
    pub status: PluginStatus,
    /// Consecutive failures (reset by a successful call or by enabling).
    pub failures: u32,
    /// Last failure, developer-facing.
    pub last_error: Option<String>,
    /// Requested permissions and whether each is granted.
    pub permissions: Vec<PermissionDto>,
    /// Active contributions.
    pub contributes: PluginContributions,
    /// Has a settings page.
    pub has_settings: bool,
    /// Installed.
    pub installed_at: Timestamp,
    /// Last change (install, update, grants, switch).
    pub updated_at: Timestamp,
}

/// Response of `plugins.list`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct PluginList {
    /// Installed plugins by name.
    pub plugins: Vec<PluginDto>,
}

/// A plugin by id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct PluginRef {
    /// Plugin id.
    pub id: String,
}

/// `plugins.inspect`: reads a package file without installing it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct InspectPackage {
    /// Path of a `.fcplugin` file.
    pub path: String,
}

/// What a package contains (install dialog).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct PackageInfo {
    /// Plugin id.
    pub id: String,
    /// Name.
    pub name: String,
    /// Version.
    pub version: String,
    /// Description.
    pub description: String,
    /// Authors.
    pub authors: Vec<String>,
    /// Requested permissions (`granted` = currently granted if installed).
    pub permissions: Vec<PermissionDto>,
    /// Version installed now, if any.
    pub installed_version: Option<String>,
    /// SHA-256 of the WASM module.
    pub sha256: String,
}

/// `plugins.install`: installs or updates from a package file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct InstallPlugin {
    /// Path of a `.fcplugin` file.
    pub path: String,
    /// Permissions the user approved (subset of the requested ones).
    pub grant: Vec<String>,
}

/// `plugins.uninstall`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct UninstallPlugin {
    /// Plugin id.
    pub id: String,
    /// Also delete the plugin's private data (storage and settings). Notes it created stay.
    #[serde(default)]
    pub delete_data: bool,
}

/// `plugins.set_enabled`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SetPluginEnabled {
    /// Plugin id.
    pub id: String,
    /// New state (enabling clears an automatic disable).
    pub enabled: bool,
}

/// `plugins.set_permissions`: the granted set (subset of the requested ones).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SetPluginPermissions {
    /// Plugin id.
    pub id: String,
    /// Granted permissions.
    pub granted: Vec<String>,
}

/// `plugins.word_decks.set` (v1.4): the decks whose words a plugin may learn from the word
/// index (`words:read`). `null` = every deck (the default); a list = those decks and their
/// sub-decks only: a word shows only when every note behind it is in them, `words.stats` is
/// refused, and reading mode's word colours (`reading.analyze`, `reading.words.lookup`) too.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
#[ts(optional_fields = nullable)]
pub struct SetPluginWordDecks {
    /// Plugin id.
    pub id: String,
    /// Decks (`null` = no limit).
    #[serde(default)]
    pub deck_ids: Option<Vec<fc_core::DeckId>>,
}

/// A plugin's word-index deck limit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PluginWordDecks {
    /// Plugin id.
    pub id: String,
    /// Decks (`null` = no limit).
    pub deck_ids: Option<Vec<fc_core::DeckId>>,
}

/// Plugin settings: schema (keys local to the plugin) and resolved values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct PluginSettings {
    /// Plugin id.
    pub id: String,
    /// Schema (labels: plugin keys).
    pub schema: Vec<SettingDef>,
    /// Values (stored or default).
    #[ts(type = "Record<string, unknown>")]
    pub values: BTreeMap<String, Value>,
}

/// `plugins.settings.set`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SetPluginSetting {
    /// Plugin id.
    pub id: String,
    /// Setting key (local).
    pub key: String,
    /// New value; `null` = back to the default.
    #[ts(type = "unknown")]
    pub value: Option<Value>,
}

/// `plugins.locales`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct GetPluginLocales {
    /// Language (`tr`, `en`).
    pub lang: String,
}

/// Translations of active plugins: plugin id → key → text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PluginLocales {
    /// Per plugin.
    pub plugins: BTreeMap<String, BTreeMap<String, String>>,
}

/// Log level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum LogLevel {
    /// Debug.
    Debug,
    /// Info.
    Info,
    /// Warning.
    Warn,
    /// Error.
    Error,
}

/// One line of a plugin's log channel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PluginLogEntry {
    /// Time.
    pub at: Timestamp,
    /// Level.
    pub level: LogLevel,
    /// Message (developer-facing).
    pub message: String,
}

/// Response of `plugins.logs` (newest last; the channel keeps the latest 200 lines of this
/// run).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PluginLogs {
    /// Entries.
    pub entries: Vec<PluginLogEntry>,
}

/// `plugins.run`: runs a contributed command.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct RunPluginCommand {
    /// Plugin id.
    pub plugin_id: String,
    /// Command id.
    pub command: String,
    /// Params for the plugin.
    #[serde(default)]
    #[ts(type = "unknown")]
    pub params: Value,
}

/// A notice a plugin asked to show (`host.ui.notify`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct PluginNotice {
    /// Plugin id.
    pub plugin_id: String,
    /// Level.
    pub level: LogLevel,
    /// Text.
    pub text: UiText,
}

/// Result of a plugin command.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct PluginCommandResult {
    /// What the plugin returned.
    #[ts(type = "unknown")]
    pub result: Value,
    /// Notices raised while it ran.
    pub notices: Vec<PluginNotice>,
}

/// Text in plugin UI: a key of the plugin's locale files, or literal content (data such as a
/// card's word — never fixed UI wording).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(untagged)]
pub enum UiText {
    /// Plugin locale key with interpolation params.
    Key {
        /// Key (looked up as `plugin.<id>.<key>`).
        key: String,
        /// Params.
        #[serde(default)]
        params: BTreeMap<String, String>,
    },
    /// Literal content.
    Text {
        /// Text.
        text: String,
    },
}

/// One option of a choice node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct UiOption {
    /// Value sent back.
    pub value: String,
    /// Label.
    pub label: UiText,
}

/// Declarative UI (docs/05 §2): the plugin describes, the frontend draws it with its own
/// components. Input values come back in `plugins.ui.act` under the node `id`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UiNode {
    /// Vertical stack.
    Column {
        /// Children.
        children: Vec<UiNode>,
    },
    /// Horizontal stack.
    Row {
        /// Children.
        children: Vec<UiNode>,
    },
    /// Heading.
    Heading {
        /// Text.
        text: UiText,
    },
    /// Paragraph.
    Text {
        /// Text.
        text: UiText,
    },
    /// Rich content (sanitized by the app before it leaves the backend).
    Html {
        /// HTML.
        html: String,
    },
    /// Text input.
    Input {
        /// Value id.
        id: String,
        /// Label.
        #[serde(default)]
        label: Option<UiText>,
        /// Initial value.
        #[serde(default)]
        value: Option<String>,
        /// Multi-line.
        #[serde(default)]
        multiline: bool,
    },
    /// Single choice.
    Choice {
        /// Value id.
        id: String,
        /// Label.
        #[serde(default)]
        label: Option<UiText>,
        /// Options.
        options: Vec<UiOption>,
    },
    /// Microphone recorder (needs `audio:record`); the value is a media reference.
    AudioRecorder {
        /// Value id.
        id: String,
    },
    /// Audio player.
    Audio {
        /// `media:<sha256>` reference.
        src: String,
    },
    /// Button running `action` (with all input values).
    Button {
        /// Action id.
        action: String,
        /// Label.
        label: UiText,
        /// Primary style.
        #[serde(default)]
        primary: bool,
    },
    /// Correct / wrong feedback.
    Feedback {
        /// Correct?
        correct: bool,
        /// Text.
        text: UiText,
    },
    /// Progress bar.
    Progress {
        /// Done.
        value: u32,
        /// Total.
        max: u32,
    },
}

/// What a plugin returns for a UI step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct UiReply {
    /// Title.
    #[serde(default)]
    pub title: Option<UiText>,
    /// Content (`null` with `done`).
    #[serde(default)]
    pub node: Option<UiNode>,
    /// The session ended.
    #[serde(default)]
    pub done: bool,
    /// State handed back on the next step (instances do not survive between calls).
    #[serde(default)]
    #[ts(type = "unknown")]
    pub state: Value,
}

/// `plugins.ui.open`: opens a view or study mode.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct OpenPluginUi {
    /// Plugin id.
    pub plugin_id: String,
    /// View or study mode id.
    pub view: String,
    /// Input (e.g. `{deck_id}` for a study mode).
    #[serde(default)]
    #[ts(type = "unknown")]
    pub input: Value,
}

/// `plugins.ui.act`: sends an action with the current input values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ActPluginUi {
    /// Session.
    pub session_id: String,
    /// Action id (a button's `action`).
    pub action: String,
    /// Input values by node id.
    #[serde(default)]
    #[ts(type = "Record<string, unknown>")]
    pub values: BTreeMap<String, Value>,
}

/// `plugins.ui.close`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ClosePluginUi {
    /// Session.
    pub session_id: String,
}

/// A UI session step as the frontend draws it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct UiSession {
    /// Session id (pass to `plugins.ui.act`).
    pub session_id: String,
    /// Plugin id (for text lookup).
    pub plugin_id: String,
    /// View or study mode.
    pub view: String,
    /// Title.
    pub title: Option<UiText>,
    /// Content.
    pub node: Option<UiNode>,
    /// Finished (the session is closed).
    pub done: bool,
    /// Notices raised during this step.
    pub notices: Vec<PluginNotice>,
}

/// Download state of a model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ModelState {
    /// Not on disk.
    Missing,
    /// Partially downloaded (resumable).
    Partial,
    /// Verified and ready.
    Ready,
}

/// A model declared by a plugin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ModelDto {
    /// Owning plugin.
    pub plugin_id: String,
    /// Model id.
    pub id: String,
    /// Name.
    pub name: String,
    /// Size in bytes.
    pub size: u64,
    /// License.
    pub license: String,
    /// Source URL.
    pub url: String,
    /// State.
    pub state: ModelState,
    /// Bytes on disk.
    pub downloaded: u64,
}

/// Response of `plugins.models.list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ModelList {
    /// Models of active plugins.
    pub models: Vec<ModelDto>,
    /// Bytes used by all model files.
    pub disk_usage: u64,
}

/// A model by plugin and id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ModelRef {
    /// Owning plugin.
    pub plugin_id: String,
    /// Model id.
    pub id: String,
}

/// Response of `plugins.models.download`: the background job.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct ModelDownload {
    /// The queued job (`plugins.model_download`).
    pub job: JobDto,
}

/// Internal: a plugin row exactly (inverse of install/update/uninstall/switch/grants).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct PluginSnapshot {
    /// Plugin id.
    pub id: String,
    /// `null` = the plugin did not exist (undoing an install removes it).
    #[ts(type = "unknown")]
    pub row: Option<Value>,
    /// Private storage to put back (`null` = leave as is).
    #[serde(default)]
    #[ts(type = "unknown")]
    pub kv: Option<Value>,
    /// Settings to put back (`null` = leave as is).
    #[serde(default)]
    #[ts(type = "unknown")]
    pub settings: Option<Value>,
}

/// Internal: plugin storage write (`value: null` deletes).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SetPluginKv {
    /// Plugin id.
    pub plugin_id: String,
    /// Key.
    pub key: String,
    /// Value.
    #[ts(type = "unknown")]
    pub value: Option<Value>,
    /// Puts back an earlier value (undo, redo, rollback of a failed call): not checked
    /// against the storage quota, which the value already passed.
    #[serde(default)]
    pub restore: bool,
}

/// Internal: records a plugin failure (and the automatic disable).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct RecordPluginFailure {
    /// Plugin id.
    pub id: String,
    /// Consecutive failures now (0 = success, resets).
    pub failures: u32,
    /// Error text.
    pub error: Option<String>,
    /// Switch the plugin off.
    pub disable: bool,
    /// Why it is switched off, for the user (`plugins.notice.*`; none = repeated failures).
    #[serde(default)]
    pub reason: Option<super::common::I18nMessage>,
}

/// Internal: crash recovery of an interrupted plugin operation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct RollbackPluginOp {
    /// Inverse calls, newest first.
    #[serde(default)]
    pub calls: Vec<crate::envelope::InvokeRequest>,
}
