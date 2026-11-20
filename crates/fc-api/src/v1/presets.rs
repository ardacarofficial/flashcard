//! Deck option presets (docs/02 §10, v0.4). The config schema (`PresetConfig`, fc-core) is
//! documented in docs/10 §16.1; the study engine (v0.2) reads a deck's effective config.

use fc_core::preset::PresetConfig;
use fc_core::{DeckId, PresetId, Timestamp};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// A preset as seen by clients.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct PresetDto {
    /// Id.
    pub id: PresetId,
    /// Name.
    pub name: String,
    /// Options.
    pub config: PresetConfig,
    /// Live decks assigned to it directly (sub-decks without their own preset inherit it).
    pub deck_ids: Vec<DeckId>,
    /// Creation time.
    pub created_at: Timestamp,
    /// Last modification time.
    pub updated_at: Timestamp,
}

/// Response of `presets.list`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct PresetList {
    /// Presets by name.
    pub items: Vec<PresetDto>,
    /// Built-in options of decks without a preset (here or on an ancestor).
    pub defaults: PresetConfig,
}

/// A preset by id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct PresetRef {
    /// Id.
    pub id: PresetId,
}

/// `presets.create`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct CreatePreset {
    /// Name (unique, case-insensitive).
    pub name: String,
    /// Options (`null` = the built-in defaults).
    pub config: Option<PresetConfig>,
}

/// `presets.update`: replaces name and options.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct UpdatePreset {
    /// Id.
    pub id: PresetId,
    /// Name.
    pub name: String,
    /// Options.
    pub config: PresetConfig,
}

/// `presets.clone`: a new preset with the options of `id`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct ClonePreset {
    /// Source preset.
    pub id: PresetId,
    /// Name of the copy.
    pub name: String,
}

/// `presets.assign`: sets (or clears, `null`) the preset of decks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct AssignPreset {
    /// Decks.
    pub deck_ids: Vec<DeckId>,
    /// Preset (`null` = none: inherit from the parent deck, else the defaults).
    pub preset_id: Option<PresetId>,
}

/// Response of `presets.assign`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct AssignResult {
    /// Decks whose preset changed.
    pub changed: u32,
}

/// `presets.restore` (internal inverse of `presets.delete`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct RestorePreset {
    /// The preset as it was.
    pub preset: PresetDto,
    /// Decks (trashed ones included) to point at it again.
    pub deck_ids: Vec<DeckId>,
}

/// `presets.for_deck`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct PresetForDeck {
    /// Deck.
    pub deck_id: DeckId,
}

/// The options a deck studies with (what the study engine reads).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
pub struct EffectivePreset {
    /// Deck asked about.
    pub deck_id: DeckId,
    /// Preset in effect (`null` = built-in defaults).
    pub preset_id: Option<PresetId>,
    /// Deck the preset is assigned to (the deck itself or an ancestor; `null` = defaults).
    pub assigned_on: Option<DeckId>,
    /// Options.
    pub config: PresetConfig,
}
