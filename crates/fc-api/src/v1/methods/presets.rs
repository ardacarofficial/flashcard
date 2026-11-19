//! Deck option presets.

use crate::v1::common::Empty;
use crate::v1::presets::*;

methods! {
    // ---- presets ----
    /// Every preset with its decks, and the built-in defaults.
    PresetsList = "presets.list", Query, "presets", "decks:read", Public, undo: false, Empty => PresetList;
    /// One preset.
    PresetsGet = "presets.get", Query, "presets", "decks:read", Public, undo: false, PresetRef => PresetDto;
    /// The options a deck studies with (own preset, else nearest ancestor's, else defaults).
    PresetsForDeck = "presets.for_deck", Query, "presets", "decks:read", Public, undo: false, PresetForDeck => EffectivePreset;
    /// Creates a preset.
    PresetsCreate = "presets.create", Command, "presets", "decks:write", Public, undo: true, CreatePreset => PresetDto;
    /// Renames / edits a preset.
    PresetsUpdate = "presets.update", Command, "presets", "decks:write", Public, undo: true, UpdatePreset => PresetDto;
    /// Copies a preset under a new name.
    PresetsClone = "presets.clone", Command, "presets", "decks:write", Public, undo: true, ClonePreset => PresetDto;
    /// Deletes a preset; its decks fall back to inheriting (undo reassigns them).
    PresetsDelete = "presets.delete", Command, "presets", "decks:write", Public, undo: true, PresetRef => PresetDto;
    /// Assigns a preset to decks (or clears it).
    PresetsAssign = "presets.assign", Command, "presets", "decks:write", Public, undo: true, AssignPreset => AssignResult;
    /// Internal inverse of delete.
    PresetsRestore = "presets.restore", Command, "presets", "decks:write", Internal, undo: true, RestorePreset => PresetDto;
}
