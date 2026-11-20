//! Themes (v1.0).

use crate::v1::common::Empty;
use crate::v1::themes::*;

methods! {
    // ---- themes ----
    /// The effective theme: scheme, preset, accent and both schemes' colour tokens.
    ThemesCurrent = "themes.current", Query, "themes", "settings:read", Public, undo: false, Empty => ThemeDto;
    /// The built-in presets with their tokens (for previews).
    ThemesPresets = "themes.presets", Query, "themes", "settings:read", Public, undo: false, Empty => ThemePresetList;
    /// Changes scheme, preset and/or accent (validated; one undo step).
    ThemesSet = "themes.set", Command, "themes", "settings:write", Public, undo: true, SetTheme => ThemeDto;
}
