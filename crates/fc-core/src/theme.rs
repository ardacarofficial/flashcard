//! Themes (docs/02 §31): light / dark / system, a few calm presets and a custom accent colour.
//!
//! A theme is a set of **colour tokens** (the CSS variables `--fc-<token>` the frontend uses;
//! components use nothing else). Every preset defines its tokens for both colour schemes; the
//! frontend picks the scheme (`light`, `dark`, or the OS one for `system`). An accent colour
//! chosen by the user replaces the preset's accent and everything derived from it, adjusted
//! per scheme so it stays readable: at least 3:1 against the background (WCAG 2.2 non-text
//! contrast), with a text colour on it of at least 4.5:1.
//!
//! Pure: no I/O; the app module (`themes`) reads the settings and calls [`resolve`].

use std::collections::BTreeMap;

/// Colour tokens every scheme defines, in CSS order (`--fc-bg`, `--fc-bg-subtle`, …).
pub const TOKENS: &[&str] = &[
    "bg",
    "bg-subtle",
    "surface",
    "text",
    "text-muted",
    "border",
    "accent",
    "accent-text",
    "danger",
    "success",
    "focus",
    "selected",
];

/// Preset of a fresh collection.
pub const DEFAULT_PRESET: &str = "default";

/// An sRGB colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    /// Parses `#rgb` or `#rrggbb` (any case). `None` for anything else.
    pub fn parse(s: &str) -> Option<Self> {
        let hex = s.trim().strip_prefix('#')?;
        if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let digit = |i: usize| u8::from_str_radix(&hex[i..=i], 16).ok();
        match hex.len() {
            3 => Some(Self(digit(0)? * 17, digit(1)? * 17, digit(2)? * 17)),
            6 => {
                let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
                Some(Self(byte(0)?, byte(2)?, byte(4)?))
            }
            _ => None,
        }
    }

    /// `#rrggbb`, lower case.
    pub fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.0, self.1, self.2)
    }

    /// WCAG relative luminance (0 = black, 1 = white).
    pub fn luminance(self) -> f64 {
        let lin = |c: u8| {
            let c = f64::from(c) / 255.0;
            if c <= 0.039_28 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * lin(self.0) + 0.7152 * lin(self.1) + 0.0722 * lin(self.2)
    }

    /// `self` moved `t` (0–1) of the way to `other`.
    pub fn mix(self, other: Rgb, t: f64) -> Rgb {
        let t = t.clamp(0.0, 1.0);
        let ch = |a: u8, b: u8| {
            let v = f64::from(a) + (f64::from(b) - f64::from(a)) * t;
            // In 0..=255 by construction.
            v.round().clamp(0.0, 255.0) as u8
        };
        Rgb(
            ch(self.0, other.0),
            ch(self.1, other.1),
            ch(self.2, other.2),
        )
    }
}

/// WCAG contrast ratio of two colours (1–21).
pub fn contrast(a: Rgb, b: Rgb) -> f64 {
    let (la, lb) = (a.luminance(), b.luminance());
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

const BLACK: Rgb = Rgb(0, 0, 0);
const WHITE: Rgb = Rgb(255, 255, 255);

/// Colour scheme.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scheme {
    /// Light background.
    Light,
    /// Dark background.
    Dark,
}

/// The base colours of a preset in one scheme (the rest is derived).
#[derive(Debug, Clone, Copy)]
struct Base {
    bg: &'static str,
    bg_subtle: &'static str,
    surface: &'static str,
    text: &'static str,
    text_muted: &'static str,
    border: &'static str,
    accent: &'static str,
    /// Focus ring colour when the accent fill itself is below 3:1 on `bg` (`None` = the accent).
    focus: Option<&'static str>,
    danger: &'static str,
    success: &'static str,
}

/// A built-in theme preset (name: i18n key `themes.preset.<id>`).
#[derive(Debug, Clone, Copy)]
pub struct Preset {
    /// Stable id, stored in the `themes.preset` setting.
    pub id: &'static str,
    light: Base,
    dark: Base,
}

const LIGHT_DANGER: &str = "#c0392b";
const LIGHT_SUCCESS: &str = "#2e7d4f";
const DARK_DANGER: &str = "#ef6f62";
const DARK_SUCCESS: &str = "#6cc28d";

/// The built-in presets: calm, low-saturation palettes (docs/01: no decoration for its own
/// sake) and one high-contrast palette.
pub const PRESETS: &[Preset] = &[
    // v1.2 design system (docs/11 §4, apps/desktop/src/lib/theme/tokens.css): warm neutrals,
    // dark-first, one amber accent (#F5B544) as the fill in both schemes. On the light
    // background the fill is paired with a darker amber for focus rings (`focus`), which
    // carries the 3:1 non-text contrast; text on the fill stays dark.
    Preset {
        id: DEFAULT_PRESET,
        light: Base {
            bg: "#fafaf8",
            bg_subtle: "#f3f3f0",
            surface: "#ffffff",
            text: "#1a1a1c",
            text_muted: "#5c5c63",
            border: "#e2e2dc",
            accent: "#f5b544",
            focus: Some("#9a5b00"),
            danger: "#c8323b",
            success: "#147a4b",
        },
        dark: Base {
            bg: "#0e0e10",
            bg_subtle: "#1c1c20",
            surface: "#151518",
            text: "#ededef",
            text_muted: "#a3a3ae",
            border: "#2a2a31",
            accent: "#f5b544",
            focus: None,
            danger: "#ff6369",
            success: "#3dd68c",
        },
    },
    Preset {
        id: "paper",
        light: Base {
            bg: "#fbf7ef",
            bg_subtle: "#f3ecdf",
            surface: "#fffdf8",
            text: "#2b2620",
            text_muted: "#6b6155",
            border: "#e4dacb",
            accent: "#9a5b2e",
            focus: None,
            danger: LIGHT_DANGER,
            success: LIGHT_SUCCESS,
        },
        dark: Base {
            bg: "#1d1a16",
            bg_subtle: "#26221c",
            surface: "#26221c",
            text: "#ece4d6",
            text_muted: "#a89d8c",
            border: "#3a342b",
            accent: "#d9a066",
            focus: None,
            danger: DARK_DANGER,
            success: DARK_SUCCESS,
        },
    },
    Preset {
        id: "sage",
        light: Base {
            bg: "#f7f9f6",
            bg_subtle: "#eef2ec",
            surface: "#ffffff",
            text: "#1d241f",
            text_muted: "#5b675e",
            border: "#dbe3d8",
            accent: "#3f7a5a",
            focus: None,
            danger: LIGHT_DANGER,
            success: LIGHT_SUCCESS,
        },
        dark: Base {
            bg: "#141916",
            bg_subtle: "#1b221d",
            surface: "#1b221d",
            text: "#e3ebe5",
            text_muted: "#9aa89e",
            border: "#2e3a32",
            accent: "#7cc19a",
            focus: None,
            danger: DARK_DANGER,
            success: DARK_SUCCESS,
        },
    },
    Preset {
        id: "slate",
        light: Base {
            bg: "#f6f8fa",
            bg_subtle: "#edf1f5",
            surface: "#ffffff",
            text: "#1b2430",
            text_muted: "#56636f",
            border: "#d9e0e8",
            accent: "#3b6ea8",
            focus: None,
            danger: LIGHT_DANGER,
            success: LIGHT_SUCCESS,
        },
        dark: Base {
            bg: "#12161b",
            bg_subtle: "#1a2027",
            surface: "#1a2027",
            text: "#e2e8ef",
            text_muted: "#94a1b0",
            border: "#2c3540",
            accent: "#7fa8d9",
            focus: None,
            danger: DARK_DANGER,
            success: DARK_SUCCESS,
        },
    },
    Preset {
        id: "contrast",
        light: Base {
            bg: "#ffffff",
            bg_subtle: "#f0f0f0",
            surface: "#ffffff",
            text: "#000000",
            text_muted: "#333333",
            border: "#595959",
            accent: "#0047b3",
            focus: None,
            danger: "#a30000",
            success: "#006b2e",
        },
        dark: Base {
            bg: "#000000",
            bg_subtle: "#121212",
            surface: "#000000",
            text: "#ffffff",
            text_muted: "#dddddd",
            border: "#a6a6a6",
            accent: "#8ab4ff",
            focus: None,
            danger: "#ff8a80",
            success: "#7ee2a8",
        },
    },
];

/// The preset with `id`.
pub fn preset(id: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|p| p.id == id)
}

/// Ids of every preset, in menu order.
pub fn preset_ids() -> Vec<&'static str> {
    PRESETS.iter().map(|p| p.id).collect()
}

/// `accent` adjusted for a scheme: moved toward the text colour in 5 % steps until it has at
/// least 3:1 contrast with `bg` (a dark accent on a dark background is lightened, a pale one
/// on a light background darkened). Colours that already pass are returned unchanged.
pub fn readable_accent(accent: Rgb, bg: Rgb, scheme: Scheme) -> Rgb {
    let toward = match scheme {
        Scheme::Light => BLACK,
        Scheme::Dark => WHITE,
    };
    (0..=20)
        .map(|step| accent.mix(toward, f64::from(step) * 0.05))
        .find(|c| contrast(*c, bg) >= 3.0)
        .unwrap_or(toward)
}

/// Text colour on an accent-filled control: black or white, whichever contrasts more (always
/// at least 4.5:1).
pub fn text_on(fill: Rgb) -> Rgb {
    if contrast(fill, BLACK) >= contrast(fill, WHITE) {
        BLACK
    } else {
        WHITE
    }
}

fn color(hex: &str) -> Rgb {
    // Preset colours are literals checked by the tests.
    Rgb::parse(hex).unwrap_or(BLACK)
}

/// Tokens of `preset` in `scheme`, with `accent` (already validated) replacing its accent.
pub fn tokens(preset: &Preset, scheme: Scheme, accent: Option<Rgb>) -> BTreeMap<String, String> {
    let base = match scheme {
        Scheme::Light => preset.light,
        Scheme::Dark => preset.dark,
    };
    let bg = color(base.bg);
    let (accent, focus) = match accent {
        Some(custom) => {
            let a = readable_accent(custom, bg, scheme);
            (a, a)
        }
        None => (color(base.accent), color(base.focus.unwrap_or(base.accent))),
    };
    let selected = accent.mix(
        bg,
        match scheme {
            Scheme::Light => 0.86,
            Scheme::Dark => 0.72,
        },
    );
    let values = [
        ("bg", bg),
        ("bg-subtle", color(base.bg_subtle)),
        ("surface", color(base.surface)),
        ("text", color(base.text)),
        ("text-muted", color(base.text_muted)),
        ("border", color(base.border)),
        ("accent", accent),
        ("accent-text", text_on(accent)),
        ("danger", color(base.danger)),
        ("success", color(base.success)),
        ("focus", focus),
        ("selected", selected),
    ];
    values
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v.hex()))
        .collect()
}

/// A theme ready for the frontend: both schemes' tokens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedTheme {
    /// Preset id actually used (an unknown stored id falls back to the default).
    pub preset: &'static str,
    /// Custom accent actually used (`#rrggbb`; `None` = the preset's).
    pub accent: Option<String>,
    /// Light scheme tokens.
    pub light: BTreeMap<String, String>,
    /// Dark scheme tokens.
    pub dark: BTreeMap<String, String>,
}

/// Resolves stored settings. Never fails: an unknown preset or an invalid accent (written
/// around the validating command) falls back to the defaults.
pub fn resolve(preset_id: &str, accent: Option<&str>) -> ResolvedTheme {
    let preset = preset(preset_id).unwrap_or(&PRESETS[0]);
    let accent = accent.and_then(Rgb::parse);
    ResolvedTheme {
        preset: preset.id,
        accent: accent.map(Rgb::hex),
        light: tokens(preset, Scheme::Light, accent),
        dark: tokens(preset, Scheme::Dark, accent),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    fn rgb(m: &BTreeMap<String, String>, k: &str) -> Rgb {
        Rgb::parse(&m[k]).unwrap()
    }

    #[test]
    fn parses_and_prints_hex() {
        assert_eq!(Rgb::parse("#2F6FDB"), Some(Rgb(0x2f, 0x6f, 0xdb)));
        assert_eq!(Rgb::parse("#abc"), Some(Rgb(0xaa, 0xbb, 0xcc)));
        assert_eq!(
            Rgb::parse(" #000000 ").map(Rgb::hex).as_deref(),
            Some("#000000")
        );
        for bad in [
            "", "#", "2f6fdb", "#12345", "#gggggg", "#1234567", "red", "#ççç",
        ] {
            assert_eq!(Rgb::parse(bad), None, "{bad}");
        }
        assert!((contrast(BLACK, WHITE) - 21.0).abs() < 1e-9);
    }

    /// Every preset is readable in both schemes: body text ≥ 7:1 (AAA), muted text ≥ 4.5:1,
    /// accent ≥ 3:1 against the background, accent text ≥ 4.5:1 on the accent, danger and
    /// success ≥ 3:1, and every token is defined.
    #[test]
    fn every_preset_is_readable_in_both_schemes() {
        assert_eq!(PRESETS[0].id, DEFAULT_PRESET);
        for p in PRESETS {
            for scheme in [Scheme::Light, Scheme::Dark] {
                let t = tokens(p, scheme, None);
                assert_eq!(
                    t.keys().map(String::as_str).collect::<Vec<_>>().len(),
                    TOKENS.len()
                );
                for k in TOKENS {
                    assert!(t.contains_key(*k), "{} {scheme:?}: no {k}", p.id);
                }
                let bg = rgb(&t, "bg");
                let check = |k: &str, min: f64| {
                    let c = contrast(rgb(&t, k), bg);
                    assert!(c >= min, "{} {scheme:?}: {k} {c:.2} < {min}", p.id);
                };
                check("text", 7.0);
                check("text-muted", 4.5);
                // The focus ring carries the non-text contrast; the accent fill does too unless
                // the preset pairs it with a separate, darker focus colour.
                check("focus", 3.0);
                let base = match scheme {
                    Scheme::Light => p.light,
                    Scheme::Dark => p.dark,
                };
                if base.focus.is_none() {
                    check("accent", 3.0);
                }
                check("danger", 3.0);
                check("success", 3.0);
                let on = contrast(rgb(&t, "accent-text"), rgb(&t, "accent"));
                assert!(on >= 4.5, "{} {scheme:?}: accent text {on:.2}", p.id);
                // Muted text stays readable on the subtle background too.
                let subtle = contrast(rgb(&t, "text-muted"), rgb(&t, "bg-subtle"));
                assert!(
                    subtle >= 4.5,
                    "{} {scheme:?}: muted on subtle {subtle:.2}",
                    p.id
                );
            }
        }
    }

    #[test]
    fn default_preset_matches_the_css_fallback() {
        // apps/desktop/src/lib/theme/tokens.css declares the same values (shown before the
        // backend answers and when the themes module is off).
        let t = resolve("default", None);
        assert_eq!(t.light["accent"], "#f5b544");
        assert_eq!(t.dark["accent"], "#f5b544");
        assert_eq!(t.light["bg"], "#fafaf8");
        assert_eq!(t.dark["bg"], "#0e0e10");
        assert_eq!(t.light["focus"], "#9a5b00");
        assert_eq!(t.dark["focus"], "#f5b544");
        // Dark text on the amber fill (tokens.css: #17130a).
        assert_eq!(t.light["accent-text"], "#000000");
    }

    #[test]
    fn unknown_settings_fall_back() {
        let t = resolve("neon", Some("not a colour"));
        assert_eq!(t.preset, DEFAULT_PRESET);
        assert_eq!(t.accent, None);
        assert_eq!(t, resolve(DEFAULT_PRESET, None));
    }

    #[test]
    fn a_custom_accent_replaces_the_preset_accent_and_its_derivatives() {
        let t = resolve("sage", Some("#C2185B"));
        assert_eq!(t.accent.as_deref(), Some("#c2185b"));
        assert_eq!(t.light["accent"], "#c2185b");
        assert_eq!(t.light["focus"], "#c2185b");
        assert_ne!(t.light["selected"], resolve("sage", None).light["selected"]);
        // The background stays the preset's.
        assert_eq!(t.light["bg"], resolve("sage", None).light["bg"]);
    }

    #[test]
    fn a_dark_accent_is_lightened_on_dark_backgrounds() {
        let t = resolve(DEFAULT_PRESET, Some("#1a237e"));
        assert_eq!(t.light["accent"], "#1a237e");
        assert_ne!(t.dark["accent"], "#1a237e");
        let c = contrast(rgb(&t.dark, "accent"), rgb(&t.dark, "bg"));
        assert!(c >= 3.0, "{c}");
    }

    proptest! {
        /// Any accent the user picks stays readable in every preset and scheme.
        #[test]
        fn any_custom_accent_is_readable(r in any::<u8>(), g in any::<u8>(), b in any::<u8>()) {
            let hex = Rgb(r, g, b).hex();
            for p in PRESETS {
                let t = resolve(p.id, Some(&hex));
                for m in [&t.light, &t.dark] {
                    let accent = rgb(m, "accent");
                    prop_assert!(contrast(accent, rgb(m, "bg")) >= 3.0);
                    prop_assert!(contrast(rgb(m, "accent-text"), accent) >= 4.5);
                }
            }
        }
    }
}
