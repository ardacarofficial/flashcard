//! Module registry DTOs (docs/03 §11). The frontend draws menus from `modules.list`.

use crate::method::{MethodKind, Visibility};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Where a module contributes UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum UiSlot {
    /// Main navigation entry.
    MainMenu,
    /// A full screen/route.
    Screen,
    /// A page in settings.
    SettingsPage,
    /// Context action on a deck.
    DeckAction,
    /// Context action on a note.
    NoteAction,
    /// A panel in the sidebar (v0.7, plugins).
    SidebarPanel,
    /// A button under the card in study and preview (v0.7, plugins); `method` runs with
    /// `{card_id}`.
    CardToolbar,
}

/// One UI contribution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct UiContribution {
    /// Slot.
    pub slot: UiSlot,
    /// Unique id (`decks.tree`).
    pub id: String,
    /// i18n key of the label.
    pub label_key: String,
    /// Icon key.
    pub icon: Option<String>,
    /// Frontend route (`/decks`).
    pub route: Option<String>,
    /// Method invoked by an action slot.
    pub method: Option<String>,
    /// Sort order within the slot.
    pub order: i32,
}

/// A module.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ModuleDto {
    /// Module id (`decks`, `attributes`, …).
    pub id: String,
    /// i18n key of the name.
    pub name_key: String,
    /// i18n key of the description.
    pub description_key: String,
    /// Currently enabled.
    pub enabled: bool,
    /// `false` for core modules that cannot be turned off.
    pub can_disable: bool,
    /// UI contributions (empty when disabled).
    pub ui: Vec<UiContribution>,
    /// Methods the module provides.
    pub methods: Vec<String>,
}

/// Response of `modules.list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct ModuleList {
    /// Modules in display order.
    pub modules: Vec<ModuleDto>,
}

/// `modules.set_enabled`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct SetModuleEnabled {
    /// Module id.
    pub id: String,
    /// New state.
    pub enabled: bool,
}

/// Public description of a method (`commands.list`: command palette, MCP tools, docs).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct MethodInfo {
    /// Qualified name (`v1.decks.create`).
    pub name: String,
    /// Command or query.
    pub kind: MethodKind,
    /// Owning module.
    pub module: String,
    /// Required permission.
    pub permission: String,
    /// Visibility.
    pub visibility: Visibility,
    /// Recorded in the oplog and undoable.
    pub undoable: bool,
    /// Enabled (module on).
    pub enabled: bool,
}

/// Response of `commands.list`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct MethodList {
    /// Methods sorted by name.
    pub methods: Vec<MethodInfo>,
}
